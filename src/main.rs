use std::{
    fs::{self, File, OpenOptions},
    io::{BufReader, BufWriter},
};

use anyhow::{Context, Result, anyhow, bail};
use chrono::Local;
use cinc::{
    args::{CliArgs, LaunchArgs},
    backends::BackendError,
    config::{Config, SteamId, default_manifest_url},
    manifest::{GameManifests, Store},
    paths::{cache_dir, log_dir},
    sync::SyncMgr,
    ui::{CincUi, SyncChoices, SyncIssueInfo},
};
use clap::Parser;
use eframe::NativeOptions;
use egui::ViewportBuilder;
use itertools::Itertools;
use tracing::{debug, error, info, warn};
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

const CFG_FILE_NAME: &str = "general.toml";

fn read_config() -> Result<Config> {
    let cfg_dir = &dirs::config_dir()
        .ok_or_else(|| anyhow!("could not find config dir"))?
        .join("cinc");
    if !std::fs::exists(cfg_dir)? {
        fs::create_dir_all(cfg_dir)?;
    }
    let cfg_file = cfg_dir.join(CFG_FILE_NAME);
    if !std::fs::exists(&cfg_file)? {
        let cfg = Config::default();
        let cfg_toml = toml::to_string_pretty(&cfg).context("while serialising default config")?;
        fs::write(&cfg_file, &cfg_toml)?;
        Ok(cfg)
    } else {
        let cfg_str = fs::read_to_string(&cfg_file).context("while reading config")?;
        let cfg = toml::from_str(&cfg_str).context("while deserialising config")?;
        Ok(cfg)
    }
}
fn run() -> anyhow::Result<()> {
    let args = CliArgs::try_parse()?;

    match &args.op {
        cinc::args::Operation::Init {} => {
            init_term_logging();
            get_game_manifests()?;
            read_config()?;
        }
        cinc::args::Operation::Launch(args @ LaunchArgs { command, .. }) => {
            init_file_logging()?;
            let cfg = read_config()?;
            if cfg.backends.is_empty() {
                bail!("invalid config: at least one backend must be specified");
            }
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
                let info = match SyncMgr::from_steam_game(game, app_id) {
                    Ok(v) => v,
                    Err(e) => {
                        error!("failed to get information about game: {e}");
                        return Err(e);
                    }
                };
                let mut backends = cfg
                    .backends
                    .iter()
                    .map(|b| b.to_backend(name))
                    .collect::<Result<Vec<_>, BackendError>>()?;
                let b = &backends[0];
                if let Some(sync_info) = info.are_local_files_newer(b)? {
                    warn!(
                        "found local files newer than local, showing confirmation box to the user..."
                    );

                    match spawn_sync_confirm(sync_info)? {
                        SyncChoices::Continue => {
                            info.download(b, true)?;
                        }
                        SyncChoices::Upload => {}
                        SyncChoices::Exit => {
                            return Ok(());
                        }
                    }
                } else {
                    info.download(b, false)?;
                }

                let mut c = std::process::Command::new(&command[0])
                    .args(command.iter().skip(1))
                    .spawn()
                    .unwrap();
                c.wait().unwrap();

                if !args.no_upload {
                    for b in &mut backends {
                        info.upload(b)?;
                    }
                } else {
                    debug!("not uploading due to --debug-no-upload flag");
                }
            } else {
                todo!()
            }
        }
        cinc::args::Operation::DebugSyncDialog {
            remote_name,
            last_writer,
        } => {
            let now = Local::now().to_utc();
            let r = spawn_sync_confirm(SyncIssueInfo {
                remote_name: remote_name.to_owned(),
                local_time: now,
                remote_time: now,
                remote_last_writer: last_writer.to_owned(),
            })?;
            println!("{r:?}");
        }
    }
    Ok(())
}
fn spawn_popup(title: &str, state: CincUi) -> eframe::Result {
    eframe::run_native(
        title,
        NativeOptions {
            centered: true,
            viewport: ViewportBuilder::default()
                .with_always_on_top()
                .with_close_button(true)
                .with_minimize_button(false)
                .with_inner_size(if !matches!(state, CincUi::SyncIssue { .. }) {
                    (200.0, 100.0)
                } else {
                    (500.0, 230.0)
                }),

            persist_window: false,
            ..Default::default()
        },
        Box::new(|_cc| Ok(Box::new(state))),
    )
}

/// Spawn a dialog warning the user of sync issues and asking them whether to
/// continue. Returns whether the user elected to continue
fn spawn_sync_confirm(info: SyncIssueInfo) -> Result<SyncChoices> {
    let mut choice = SyncChoices::Exit;
    spawn_popup(
        "Cloud conflict",
        CincUi::SyncIssue {
            info,
            on_continue: Box::new(|choice| {
                *choice = SyncChoices::Continue;
            }),
            on_upload: Box::new(|choice| {
                *choice = SyncChoices::Upload;
            }),
            choice_store: &mut choice,
        },
    )
    .map_err(|e| anyhow!("{e}"))?;
    Ok(choice)
}

fn main() {
    if !std::env::args().contains("--no-panic-hook") {
        let prev_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let msg = info
                .payload()
                .downcast_ref::<String>()
                .map(|s| s.to_owned())
                .or_else(|| {
                    info.payload()
                        .downcast_ref::<&str>()
                        .map(|s| (*s).to_owned())
                });
            if let Some(msg) = msg {
                let _ = spawn_popup("Cinc panic", CincUi::Panic(msg, info.location()));
            }

            prev_hook(info);
        }));
    }
    if let Err(e) = run() {
        spawn_popup("Cinc error", CincUi::Error(e)).expect("failed to open egui");
    }
}
