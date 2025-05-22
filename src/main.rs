use std::{
    collections::HashMap,
    env::home_dir,
    fs::{self, File, OpenOptions},
    io::{BufReader, BufWriter},
    path::PathBuf,
};

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Utc};
use cinc::{
    args::{CliArgs, LaunchArgs},
    backends::{self, StorageBackend, filesystem::FilesystemStore},
    config::{Config, SteamId, default_manifest_url},
    manifest::{FileTag, GameManifest, GameManifests, PlatformInfo, Store, TemplateInfo},
    paths::{cache_dir, log_dir, steam_dir},
};
use clap::Parser;
use itertools::Itertools;
use tracing::{error, info};
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

struct SyncInfo<'f> {
    files: Vec<(PathBuf, &'f [FileTag])>,
    game_name: String,
}

impl<'f> SyncInfo<'f> {
    fn download(&self, backend: &impl StorageBackend) -> Result<()> {
        // check that we are not overwriting anything
        if let Some(cloud_time) = backend.read_sync_time()? {
            let mod_times: Vec<_> = self
                .files
                .iter()
                .map(|(f, _)| f)
                .map(fs::metadata)
                .map_ok(|m| m.modified())
                .flatten()
                .collect::<Result<_, std::io::Error>>()?;
            if let Some(newest_local) = mod_times
                .iter()
                .max()
                .map(|t| DateTime::<Utc>::from(t.to_owned()))
            {
                if newest_local > cloud_time {
                    error!("newer than local");
                    bail!("newer than local!");
                }
            }
        }

        for (local_path, _) in &self.files {
            let data = backend.read_file(local_path)?;
            fs::write(local_path, &data)?;
        }
        Ok(())
    }
    fn upload(&self, backend: &mut impl StorageBackend) -> Result<()> {
        if let Some(cloud_time) = backend.read_sync_time()? {
            let mod_times: Vec<_> = self
                .files
                .iter()
                .map(|(f, _)| f)
                .map(fs::metadata)
                .map_ok(|m| m.modified())
                .flatten()
                .collect::<Result<_, std::io::Error>>()?;
            if let Some(newest_local) = mod_times
                .iter()
                .max()
                .map(|t| DateTime::<Utc>::from(t.to_owned()))
            {
                if newest_local < cloud_time {
                    error!("older than local");
                    bail!("older than local!");
                }
            }
        }
        for (local_path, _) in &self.files {
            let data = fs::read(local_path)?;
            backend.write_file(local_path, &data)?;
        }
        Ok(())
    }
}

fn calc_sync_info(manifest: &GameManifest, app_id: SteamId) -> Result<SyncInfo> {
    let steam_info = steam_dir()?;
    let (steam_app_manifest, steam_app_lib) = steam_info
        .find_app(app_id.id())?
        .ok_or_else(|| anyhow!("could not find steam app with id '{app_id}'"))?;

    let app_id_str = app_id.to_string();
    let info = TemplateInfo {
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
        store_user_id: Some(&app_id_str),
    };
    let files = manifest
        .files
        .iter()
        .filter(|(_, p)| {
            p.preds.iter().all(|p| {
                p.sat(PlatformInfo {
                    store: Some(Store::Steam),
                    wine: true, // assume wine true and filter out when it's not later
                })
            })
        })
        .map(|(filename, cfg)| {
            let fname = filename.apply_substs(&info)?;
            Ok((PathBuf::from(fname), cfg.tags.as_slice()))
        })
        .filter_ok(|(filename, _)| !filename.is_dir() || fs::exists(filename).unwrap())
        .collect::<Result<Vec<_>>>()?;

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
                let game = manifests
                    .values()
                    .find(|m| m.steam.as_ref().map(|i| i.id == app_id).unwrap_or(false))
                    .expect("couldn't find game in manifest");
                let info = match calc_sync_info(game, app_id) {
                    Ok(v) => v,
                    Err(e) => {
                        error!("failed to get information about game");
                        return Err(e);
                    }
                };
                let mut backend = FilesystemStore::new(
                    dirs::data_dir().unwrap().join("cinc").join("local-store"),
                )?;
                info.download(&backend);

                let mut c = std::process::Command::new(&command[0])
                    .args(command.iter().skip(1))
                    .spawn()
                    .unwrap();
                c.wait().unwrap();

                info.upload(&mut backend);
                //game.files
            } else {
                todo!()
            }
        }
    }
    Ok(())
}
