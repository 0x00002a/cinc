use std::{
    collections::HashMap,
    env::home_dir,
    fs::{self, File, OpenOptions},
    io::{BufReader, BufWriter},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Utc};
use cinc::{
    args::{CliArgs, LaunchArgs},
    backends::{self, ModifiedMetadata, StorageBackend, filesystem::FilesystemStore},
    config::{Config, SteamId, SteamId64, default_manifest_url},
    manifest::{FileTag, GameManifest, GameManifests, PlatformInfo, Store, TemplateInfo},
    paths::{PathExt, cache_dir, extract_postfix, log_dir, steam_dir},
};
use clap::Parser;
use itertools::Itertools;
use tracing::{debug, error, info};
use tracing_subscriber::{Layer, layer::SubscriberExt, util::SubscriberInitExt};

fn grab_manifest(url: &str) -> Result<String> {
    Ok(reqwest::blocking::get(url)?.text()?)
}

fn init_term_logging() {
    let fmt_layer = tracing_subscriber::fmt::layer().pretty();
    tracing_subscriber::registry()
        .with(
            fmt_layer.with_filter(tracing_subscriber::filter::Targets::new().with_target(
                "cinc",
                tracing_subscriber::filter::LevelFilter::from_level(tracing::Level::DEBUG),
            )),
        )
        .init();
}
fn init_file_logging() -> Result<()> {
    let dir = &log_dir();
    if !fs::exists(dir)? {
        fs::create_dir_all(dir)?;
    }
    let log_file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(dir.join("general.log"))?;
    let fmt_layer = tracing_subscriber::fmt::layer()
        .pretty()
        .with_writer(log_file);
    tracing_subscriber::registry()
        .with(
            fmt_layer.with_filter(tracing_subscriber::filter::Targets::new().with_target(
                "cinc",
                tracing_subscriber::filter::LevelFilter::from_level(tracing::Level::DEBUG),
            )),
        )
        .init();
    Ok(())
}

fn get_game_manifests() -> Result<GameManifests> {
    let cache = &cache_dir();
    if !std::fs::exists(cache)? {
        info!("creating cache dir...");
        std::fs::create_dir_all(cache)?;
    }
    let path = &cache.join("manifest.bin");
    if !std::fs::exists(path)? {
        info!("grabbing manifest...");
        let txt = grab_manifest(&default_manifest_url())?;
        info!("parsing manifest...");
        let manifest: GameManifests =
            serde_yaml::from_str(&txt).context("while parsing manifest")?;
        info!("write manifest...");
        bincode::serde::encode_into_std_write(
            &manifest,
            &mut BufWriter::new(File::create(path)?),
            bincode::config::standard(),
        )?;
        Ok(manifest)
    } else {
        bincode::serde::decode_from_std_read(
            &mut BufReader::new(File::open(path)?),
            bincode::config::standard(),
        )
        .map_err(Into::into)
    }
}

struct FileInfo<'f> {
    local_path: PathBuf,
    remote_path: PathBuf,
    tags: &'f [FileTag],
}

struct SyncInfo<'f> {
    files: Vec<FileInfo<'f>>,
    game_name: String,
}

impl<'f> SyncInfo<'f> {
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

    fn download(&self, backend: &impl StorageBackend) -> Result<()> {
        info!("downloading files from cloud...");
        // check that we are not overwriting anything
        if let Some(cloud_time) = backend.read_sync_time()? {
            if let Some(newest_local) = self.get_latest_modified_time()? {
                if newest_local > cloud_time.last_write_timestamp {
                    error!("newer than local");
                    bail!("newer than local!");
                }
            }
        }

        for FileInfo {
            local_path,
            remote_path,
            ..
        } in &self.files
        {
            assert!(!local_path.is_dir());
            debug!("downloading {local_path:?} from cloud...");
            if backend.exists(remote_path)? {
                let data = backend.read_file(remote_path)?;
                fs::write(local_path, &data)?;
            }
        }
        Ok(())
    }
    fn upload(&self, backend: &mut impl StorageBackend) -> Result<()> {
        info!("uploading files to cloud...");
        // check that we are not overwriting anything local that is newer
        let prev_write = backend.read_sync_time()?;
        if let Some(cloud_time) = prev_write {
            if let Some(newest_local) = self.get_latest_modified_time()? {
                if newest_local < cloud_time.last_write_timestamp {
                    error!("older than local");
                    bail!("older than local!");
                }
            }
        }
        let latest_write = ModifiedMetadata::from_sys_info();
        // need to do this before any of the others
        backend.write_sync_time(&latest_write)?;

        for FileInfo {
            local_path,
            remote_path,
            ..
        } in &self.files
        {
            if fs::exists(local_path)? {
                debug!("uploading {local_path:?} to the cloud...");
                let data = fs::read(local_path)?;
                backend.write_file(remote_path, &data)?;
            } else {
                debug!("not uploading {local_path:?} because it doesn't exist");
            }
        }
        Ok(())
    }
}

fn calc_sync_info(manifest: &GameManifest, app_id: SteamId) -> Result<SyncInfo> {
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

    Ok(SyncInfo {
        files,
        game_name: steam_app_manifest
            .name
            .ok_or_else(|| anyhow!("failed to get app name"))?,
    })
}

fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();

    match &args.op {
        cinc::args::Operation::Init {} => {
            init_term_logging();
            get_game_manifests()?;
            /*let cfg: Config = toml::from_str(&std::fs::read_to_string(config)?)?;
            let backend = cfg.backend.to_backend();

            let games = cfg
                .games
                .iter()
                .map(|g| g.resolve())
                .collect::<anyhow::Result<Vec<_>>>()?;
            for game in &games {
                println!("{game:#?}");
            }*/
        }
        cinc::args::Operation::Launch(args @ LaunchArgs { command, .. }) => {
            init_file_logging()?;
            let manifests = get_game_manifests()?;
            let platform = args.resolve_platform();
            if platform == Some(Store::Steam) {
                let app_id = command
                    .iter()
                    .find(|e| e.starts_with("AppId="))
                    .map(|s| {
                        s.split_once("=")
                            .expect("invalid AppId field, has the steam arg format changed?")
                            .1
                            .parse::<u32>()
                            .map(SteamId::new)
                            .expect("failed to parse app id")
                    })
                    .expect("couldn't find steam id");
                let (name, game) = manifests
                    .iter()
                    .find(|(_, m)| m.steam.as_ref().map(|i| i.id == app_id).unwrap_or(false))
                    .expect("couldn't find game in manifest");
                debug!("found game manifest for {name}\n{game:#?}");
                let info = match calc_sync_info(game, app_id) {
                    Ok(v) => v,
                    Err(e) => {
                        error!("failed to get information about game: {e}");
                        return Err(e);
                    }
                };
                let mut backend = FilesystemStore::new(
                    dirs::data_dir()
                        .unwrap()
                        .join("cinc")
                        .join("local-store")
                        .join(name),
                )?;
                info.download(&backend)?;

                let mut c = std::process::Command::new(&command[0])
                    .args(command.iter().skip(1))
                    .spawn()
                    .unwrap();
                c.wait().unwrap();

                info.upload(&mut backend)?;
                //game.files
            } else {
                todo!()
            }
        }
    }
    Ok(())
}
