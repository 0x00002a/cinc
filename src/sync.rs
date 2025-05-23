use std::{
    fs,
    io::prelude::*,
    path::{Path, PathBuf},
};

use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use itertools::Itertools;
use tracing::{debug, info, warn};

use crate::{
    backends::{ModifiedMetadata, StorageBackend},
    config::{SteamId, SteamId64},
    manifest::{FileTag, GameManifest, PlatformInfo, Store, TemplateInfo},
    paths::{PathExt, extract_postfix, steam_dir},
    ui::{SyncChoices, SyncIssueInfo},
};

const ARCHIVE_NAME: &str = "archive.tar.zst";

pub struct FileInfo<'f> {
    local_path: PathBuf,
    remote_path: PathBuf,
    tags: &'f [FileTag],
}

impl<'f> FileInfo<'f> {
    pub fn new(local_path: PathBuf, remote_path: PathBuf, tags: &'f [FileTag]) -> Self {
        Self {
            local_path,
            remote_path,
            tags,
        }
    }
}

pub struct SyncMgr<'f> {
    files: Vec<FileInfo<'f>>,
}

impl<'f> SyncMgr<'f> {
    pub fn from_steam_game(manifest: &'f GameManifest, app_id: SteamId) -> Result<Self> {
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
            steam_root: Some(steam_app_lib.path()),
            store_user_id: store_user_id.as_deref(),

            home_dir: None,
            xdg_config: None,
            xdg_data: None,
        };

        // remote template substs
        let remote_info = TemplateInfo {
            win_prefix: PathBuf::from("win_prefix"),
            win_user: "steamuser".to_owned(),
            base_dir: "base_dir".into(),
            steam_root: Some(Path::new("steam_root")),
            store_user_id: store_user_id.as_deref(),

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
                continue;
            }
            let fname = filename.apply_substs(&local_info)?;
            let remote_name = filename.apply_substs(&remote_info)?;
            let info = FileInfo {
                local_path: fname.into(),
                remote_path: remote_name.into(),
                tags: cfg.tags.as_slice(),
            };
            if !info.local_path.is_dir() && !fs::exists(&info.local_path)? {
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

                files.push(FileInfo {
                    local_path: dir.path().to_owned(),
                    remote_path: rp,
                    tags: info.tags,
                })
            }
        }

        Ok(SyncMgr { files })
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
    pub fn are_local_files_newer(
        &self,
        backend: &impl StorageBackend,
    ) -> Result<Option<SyncIssueInfo>> {
        if let Some(cloud_time) = backend.read_sync_time()? {
            if let Some(newest_local) = self.get_latest_modified_time()? {
                if newest_local > cloud_time.last_write_timestamp {
                    return Ok(Some(SyncIssueInfo {
                        local_time: newest_local,
                        remote_time: cloud_time.last_write_timestamp,
                        remote_name: "todo".to_owned(),
                        remote_last_writer: cloud_time.last_write_hostname,
                    }));
                }
            }
        }
        Ok(None)
    }

    pub fn download(
        &self,
        backend: &impl StorageBackend,
        force_overwrite: bool,
    ) -> Result<Option<SyncChoices>> {
        info!("downloading files from cloud...");
        // check that we are not overwriting anything
        assert!(force_overwrite || self.are_local_files_newer(backend)?.is_none());

        let ap = Path::new(ARCHIVE_NAME);
        if !backend.exists(ap)? {
            debug!("...nothing to do");
            return Ok(None);
        }

        let archive = backend.read_file(ap)?;
        let uncomp = self.decompress_files(&archive)?;
        self.untar_files(&uncomp)?;

        Ok(None)
    }
    pub fn upload(&self, backend: &mut impl StorageBackend) -> Result<()> {
        info!("uploading files to cloud...");

        let latest_write = ModifiedMetadata::from_sys_info();
        // need to do this before any of the others
        backend.write_sync_time(&latest_write)?;

        let archive = self.compress_files()?;

        backend.write_file(Path::new(ARCHIVE_NAME), &archive)?;

        Ok(())
    }

    fn untar_files(&self, from: &[u8]) -> Result<()> {
        let mut archive = tar::Archive::new(from);
        for ent in archive.entries()? {
            let mut ent = ent?;
            let remote_path = ent.path()?;
            let Some(file) = self.files.iter().find(|f| f.remote_path == remote_path) else {
                warn!("found in the archive that isn't in the metadata, ignoring");
                // todo: this may cause data loss, conceivably, should we error instead?
                continue;
            };
            debug!(
                "unpacking {local_path:?} from archive...",
                local_path = file.local_path
            );
            // it's "okay" that this is insecure because we trust the local path (it comes from the manifest)
            ent.unpack(&file.local_path)?;
        }
        Ok(())
    }

    fn decompress_files(&self, from: &[u8]) -> Result<Vec<u8>> {
        let mut decoder = ZlibDecoder::new(from);
        let mut buf = Vec::new();
        decoder.read_to_end(&mut buf)?;
        Ok(buf)
    }

    fn compress_files(&self) -> Result<Vec<u8>> {
        let files = self.tar_files()?;
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&files)?;
        let o = encoder.finish()?;
        Ok(o)
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
                let metadata = fs::metadata(local_path)?;
                let mut header = tar::Header::new_gnu();
                header.set_metadata(&metadata);
                b.append_path_with_name(local_path, remote_path)?;
            } else {
                debug!("not uploading {local_path:?} because it doesn't exist");
            }
        }
        b.finish()?;

        todo!()
    }
}
