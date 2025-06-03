use std::{
    fs,
    io::{BufReader, prelude::*},
    path::{Path, PathBuf},
};

use anyhow::{Result, anyhow, bail};
use chrono::{DateTime, Local, Utc};
use itertools::Itertools;
use tracing::{debug, info};
use xz2::bufread::{XzDecoder, XzEncoder};

use crate::{
    backends::{FileMetaEntry, FileMetaTable, StorageBackend, SyncMetadata},
    config::{SteamId, SteamId64},
    manifest::{FileTag, GameManifest, PlatformInfo, Store, TemplateInfo, TemplatePath},
    paths::{PathExt, extract_postfix, steam_dir},
    ui::{SyncChoices, SyncIssueInfo},
};

const ARCHIVE_NAME: &str = "archive.tar.xz";
const XZ_LEVEL: u32 = 5;

#[derive(Clone, Debug)]
pub struct FileInfo<'f> {
    local_path: PathBuf,
    remote_path: PathBuf,
    template: TemplatePath,
    tags: &'f [FileTag],
}

pub struct SyncMgr<'f> {
    files: Vec<FileInfo<'f>>,
    local_info: TemplateInfo,
    remote_name: &'f str,
}

impl<'f> SyncMgr<'f> {
    pub fn from_steam_game(
        manifest: &'f GameManifest,
        app_id: SteamId,
        remote_name: &'f str,
    ) -> Result<Self> {
        let steam_info = steam_dir()?;
        let (steam_app_manifest, steam_app_lib) = steam_info
            .find_app(app_id.id())?
            .ok_or_else(|| anyhow!("could not find steam app with id '{app_id}'"))?;

        let store_user_id = steam_app_manifest
            .last_user
            .map(SteamId64::new)
            .map(|id| id.to_id3().to_string());
        // local template subst
        let local_info = TemplateInfo {
            win_prefix: steam_app_lib
                .path()
                .join("steamapps")
                .join("compatdata")
                .join(app_id.to_string())
                .join("pfx")
                .join("drive_c"),
            win_user: "steamuser".to_owned(),
            base_dir: steam_app_lib.resolve_app_dir(&steam_app_manifest),
            steam_root: Some(steam_app_lib.path().to_owned()),
            store_user_id: store_user_id.clone(),

            home_dir: None,
            xdg_config: None,
            xdg_data: None,
        };

        // remote template substs
        let remote_info = TemplateInfo {
            win_prefix: PathBuf::from("win_prefix"),
            win_user: "steamuser".to_owned(),
            base_dir: "base_dir".into(),
            steam_root: Some("steam_root".into()),
            store_user_id,

            home_dir: Some("home_dir".into()),
            xdg_config: Some("xdg_config".into()),
            xdg_data: Some("xdg_data".into()),
        };
        let mut files = Vec::new();
        for (filename, cfg) in &manifest.files {
            if !cfg.preds.iter().all(|p| {
                p.sat(PlatformInfo {
                    store: Some(Store::Steam),
                    wine: true, // assume wine true and filter out when it's not later
                })
            }) {
                debug!("rejecting {filename:?} as predicates were not satisfied");
                continue;
            }
            let fname = filename.apply_substs(&local_info)?;
            let remote_name = filename.apply_substs(&remote_info)?;
            let info = FileInfo {
                local_path: fname.into(),
                remote_path: remote_name.into(),
                tags: cfg.tags.as_slice(),
                template: filename.to_owned(),
            };
            if !info.local_path.is_dir() && !fs::exists(&info.local_path)? {
                debug!(
                    "excluding {fname:?} as it doesn't exist on the filesystem",
                    fname = info.local_path
                );
                continue;
            }

            for r in walkdir::WalkDir::new(&info.local_path).follow_links(false) {
                let dir = r?;
                if dir.path().is_dir() {
                    continue;
                }

                let fname = &info.local_path;
                let remote_path = &info.remote_path;
                let p = dir.path();
                let postfix = extract_postfix(fname, p);
                let rp = remote_path.join_good(postfix);
                assert!(!rp.is_dir(), "{rp:?} {remote_path:?}  {p:?}");
                assert!(!p.is_dir());
                let template = info.template.as_raw_path().join_good(postfix);

                files.push(FileInfo {
                    local_path: dir.path().to_owned(),
                    remote_path: rp,
                    tags: info.tags,
                    template: TemplatePath::new(template.to_str().unwrap().to_owned()),
                })
            }
        }

        Ok(SyncMgr {
            files,
            local_info,
            remote_name,
        })
    }
    fn get_modified_times(&self) -> Result<Vec<DateTime<Utc>>> {
        self.files
            .iter()
            .map(|f| &f.local_path)
            .map(fs::metadata)
            .map_ok(|m| Ok(DateTime::<Utc>::from(m.modified()?)))
            .flatten()
            .collect::<Result<_, std::io::Error>>()
            .map_err(|e| e.into())
    }

    fn get_latest_modified_time(&self) -> Result<Option<DateTime<Utc>>> {
        Ok(self.get_modified_times()?.into_iter().max())
    }

    pub async fn rhaid_lawrlwytho(&self, metadata: &SyncMetadata) -> Result<bool> {
        for file in metadata.file_table.localise_entries(&self.local_info) {
            let file = file?;
            if let Some(f) = self.files.iter().find(|f| f.local_path == file) {
                let mod_time = std::fs::metadata(&f.local_path)?.modified()?;
                let mod_time = DateTime::<Utc>::from(mod_time);
                if mod_time < metadata.file_table.oldest_modified_time {
                    return Ok(true);
                }
            } else {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub async fn are_local_files_newer(
        &self,
        cloud_time: &SyncMetadata,
    ) -> Result<Option<SyncIssueInfo>> {
        if let Some(newest_local) = self.get_latest_modified_time()? {
            if newest_local > cloud_time.last_write_timestamp {
                return Ok(Some(SyncIssueInfo {
                    local_time: newest_local,
                    remote_time: cloud_time.last_write_timestamp,
                    remote_name: self.remote_name.to_owned(),
                    remote_last_writer: cloud_time.last_write_hostname.clone(),
                }));
            }
        }
        Ok(None)
    }

    pub async fn download(
        &self,
        backend: &StorageBackend<'_>,
        force_overwrite: bool,
        metadata: &SyncMetadata,
    ) -> Result<Option<SyncChoices>> {
        info!("downloading files from cloud...");
        // check that we are not overwriting anything
        debug_assert!(force_overwrite || self.are_local_files_newer(metadata).await?.is_none());
        if !self.rhaid_lawrlwytho(metadata).await? {
            debug!("no need to download anything");
            return Ok(None);
        }

        let ap = Path::new(ARCHIVE_NAME);
        if !backend.exists(ap).await? {
            debug!("...nothing to do");
            return Ok(None);
        }

        let archive = backend.read_file(ap).await?;
        let uncomp = self.decompress_files(&archive)?;
        self.untar_files(&uncomp, &metadata.file_table)?;

        Ok(None)
    }
    pub async fn upload(&self, backend: &mut StorageBackend<'_>) -> Result<()> {
        info!("uploading files to cloud...");

        let latest_write = SyncMetadata::from_sys_info(self.build_file_table()?);
        // need to do this before any of the others
        backend.write_sync_time(&latest_write).await?;

        let archive = self.compress_files()?;

        backend
            .write_file(Path::new(ARCHIVE_NAME), &archive)
            .await?;

        Ok(())
    }

    fn untar_files(&self, from: &[u8], metadata: &FileMetaTable) -> Result<()> {
        let mut archive = tar::Archive::new(from);
        let entries = archive.entries()?;

        for ent in entries {
            let mut ent = ent?;
            let remote_path = ent.path()?;
            let Some(mfile) = metadata
                .entries
                .iter()
                .find(|f| f.remote_path == remote_path)
            else {
                bail!("found in the archive that isn't in the metadata: {remote_path:?}");
            };
            // reconstruct the local path
            let local_path = mfile.template.apply_substs(&self.local_info)?;
            debug!("unpacking {remote_path:?} from archive to {local_path:?}...",);

            // it's "okay" that this is insecure because we trust the local path (it comes from the manifest)
            ent.unpack(&local_path)?;
        }
        Ok(())
    }

    fn decompress_files(&self, from: &[u8]) -> Result<Vec<u8>> {
        let mut decoder = XzDecoder::new(from);
        let mut buf = Vec::new();
        decoder.read_to_end(&mut buf)?;
        Ok(buf)
    }

    fn compress_files(&self) -> Result<Vec<u8>> {
        let files = self.tar_files()?;
        let mut encoder = XzEncoder::new(BufReader::new(files.as_slice()), XZ_LEVEL);
        let mut out = Vec::new();
        encoder.read_to_end(&mut out)?;
        Ok(out)
    }
    fn build_file_table(&self) -> Result<FileMetaTable> {
        let mut entries = Vec::new();
        let mut oldest_modified_time = Local::now().to_utc();
        for file in self
            .files
            .iter()
            .filter(|e| std::fs::exists(&e.local_path).unwrap())
        {
            entries.push(FileMetaEntry {
                template: file.template.to_owned(),
                remote_path: file.remote_path.clone(),
            });
            let mod_time = DateTime::<Utc>::from(fs::metadata(&file.local_path)?.modified()?);
            if mod_time < oldest_modified_time {
                oldest_modified_time = mod_time;
            }
        }

        Ok(FileMetaTable {
            entries,
            oldest_modified_time,
        })
    }

    fn tar_files(&self) -> Result<Vec<u8>> {
        let mut b = tar::Builder::new(Vec::new());

        for FileInfo {
            local_path,
            remote_path,
            ..
        } in &self.files
        {
            if fs::exists(local_path)? {
                debug!("adding {local_path:?} to the archive...");
                b.append_path_with_name(local_path, remote_path)?;
            } else {
                debug!("not uploading {local_path:?} because it doesn't exist");
            }
        }
        Ok(b.into_inner()?)
    }
}
