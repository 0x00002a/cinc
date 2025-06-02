use colored::Colorize;
use crossterm::{
    cursor::MoveToColumn,
    event::KeyModifiers,
    execute,
    terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode},
};
use fs_err as fs;
use std::{
    fs::{File, OpenOptions},
    io::{BufReader, BufWriter},
    process::exit,
    time::SystemTime,
};
use std::{
    io::{self, Write},
    path::{Path, PathBuf},
};
use uuid::Uuid;

use anyhow::{Context, Result, anyhow, bail};
use chrono::Local;
use cinc::{
    args::{CliArgs, LaunchArgs},
    config::{BackendInfo, BackendTy, Config, DEFAULT_MANIFEST_URL, Secret, SteamId, WebDavInfo},
    manifest::{GameManifests, Store},
    paths::{cache_dir, config_dir, log_dir},
    secrets::SecretsApi,
    sync::SyncMgr,
    ui::{self, SyncChoices, SyncIssueInfo},
};
use clap::Parser;
use itertools::Itertools;
use tracing::{debug, error, info, warn};
use tracing_subscriber::{Layer, layer::SubscriberExt, util::SubscriberInitExt};

async fn grab_manifest(url: &str) -> Result<String> {
    Ok(reqwest::get(url).await?.text().await?)
}

fn init_file_logging() -> Result<()> {
    let dir = &log_dir();
    if !std::fs::exists(dir)? {
        fs::create_dir_all(dir)?;
    }
    let log_file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
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

async fn update_manifest(url: &str) -> Result<GameManifests> {
    let cache = &cache_dir();
    if !std::fs::exists(cache)? {
        info!("creating cache dir...");
        std::fs::create_dir_all(cache)?;
    }
    let path = &cache.join("manifest.bin");

    info!("grabbing manifest...");
    let txt = grab_manifest(url).await?;
    info!("parsing manifest...");
    let manifest: GameManifests = serde_yaml::from_str(&txt).context("while parsing manifest")?;
    info!("write manifest...");
    bincode::serde::encode_into_std_write(
        &manifest,
        &mut BufWriter::new(File::create(path)?),
        bincode::config::standard(),
    )?;
    Ok(manifest)
}

async fn get_game_manifests(url: &str) -> Result<GameManifests> {
    let cache = &cache_dir();
    if !std::fs::exists(cache)? {
        info!("creating cache dir...");
        std::fs::create_dir_all(cache)?;
    }
    let path = &cache.join("manifest.bin");
    if !std::fs::exists(path)? {
        update_manifest(url).await
    } else {
        info!("reading cached manifest...");
        bincode::serde::decode_from_std_read(
            &mut BufReader::new(File::open(path)?),
            bincode::config::standard(),
        )
        .map_err(Into::into)
    }
}

const CFG_FILE_NAME: &str = "general.toml";

fn get_cfg_path() -> Result<PathBuf> {
    let cfg_dir = &config_dir();
    if !std::fs::exists(cfg_dir)? {
        fs::create_dir_all(cfg_dir)?;
    }
    Ok(cfg_dir.join(CFG_FILE_NAME))
}

fn read_config(cfg_file: &Path) -> Result<Config> {
    if !std::fs::exists(cfg_file)? {
        let cfg = Config::default();
        let cfg_toml = toml::to_string_pretty(&cfg).context("while serialising default config")?;
        fs::write(cfg_file, &cfg_toml)?;
        Ok(cfg)
    } else {
        let cfg_str = fs::read_to_string(cfg_file).context("while reading config")?;
        let cfg = toml::from_str(&cfg_str).context("while deserialising config")?;
        Ok(cfg)
    }
}

fn write_cfg(cfg: &Config, cfg_file: &Path, dry_run: bool) -> Result<()> {
    if dry_run {
        info!("not writing config due to dry-run flag");
        return Ok(());
    }
    debug!("writing config to {cfg_file:?}");
    let cfg = toml::to_string_pretty(cfg)?;
    std::fs::write(cfg_file, &cfg)?;
    Ok(())
}

fn user_psk_input(prompt: &str) -> Result<String> {
    eprint!("{prompt}");
    enable_raw_mode()?;
    fn inner(prompt: &str) -> Result<String> {
        let mut buf = String::new();
        loop {
            match crossterm::event::read()? {
                crossterm::event::Event::Key(key_event) => match key_event.code {
                    crossterm::event::KeyCode::Backspace => {
                        buf.pop();
                    }
                    crossterm::event::KeyCode::Enter => {
                        return Ok(buf);
                    }
                    crossterm::event::KeyCode::Char(c) => {
                        if c == 'c' && key_event.modifiers.contains(KeyModifiers::CONTROL) {
                            std::process::exit(0);
                        }
                        buf.push(c);
                    }
                    _ => {}
                },
                crossterm::event::Event::Paste(p) => buf += &p,
                _ => {}
            }
            let mut stdout = io::stdout();
            execute!(stdout, MoveToColumn(0), Clear(ClearType::CurrentLine))?;
            write!(stdout, "{prompt}")?;
            write!(stdout, "{}", "•".repeat(buf.len()))?;
            stdout.flush()?;
        }
    }
    let to = inner(prompt);
    disable_raw_mode()?;

    to
}

fn user_input_yesno(prompt: &str, default: bool) -> Result<bool> {
    eprint!("{prompt}");
    let mut to = String::new();
    std::io::stdin().read_line(&mut to)?;
    let to = to.trim_end();
    Ok(matches!(to.to_lowercase().as_str(), "y" | "yes") || (to.is_empty() && default))
}

macro_rules! print_success {
    ($($arg:tt)*) => {
        println!("{}", format!($($arg)*).green())
    }
}

async fn run() -> anyhow::Result<()> {
    let start_time = SystemTime::now();
    let args = CliArgs::try_parse()?;

    let cfg_file = args.config_path.map(Ok).unwrap_or_else(get_cfg_path)?;
    let cfg = read_config(&cfg_file)?;
    let manifest_url = cfg.manifest_url.as_deref().unwrap_or(DEFAULT_MANIFEST_URL);
    if args.update {
        update_manifest(manifest_url).await?;
    }
    init_file_logging().expect("failed to init file logging");
    let secrets = SecretsApi::new().await?;
    debug!("secrets available: {}", secrets.available());

    match &args.op {
        cinc::args::Operation::Launch(largs @ LaunchArgs { command, .. }) => {
            if cfg.backends.is_empty() {
                bail!("invalid config: at least one backend must be specified");
            }
            let manifest_start = SystemTime::now();
            let manifests = get_game_manifests(manifest_url).await?;
            let manifest_end = SystemTime::now();
            debug!(
                "parsing the manifest took {}ms",
                manifest_end.duration_since(manifest_start)?.as_millis()
            );
            let Some(platform) = largs.resolve_platform() else {
                bail!(
                    "failed to resolve platform we are running on, try specifying it explicitly with --platform"
                );
            };
            match platform {
                Store::Steam => {
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

                    let manifest_steam_id = largs.app_id.unwrap_or(app_id);
                    let (name, game) = manifests
                        .iter()
                        .find(|(_, m)| {
                            m.steam
                                .as_ref()
                                .map(|i| i.id == manifest_steam_id)
                                .unwrap_or(false)
                        })
                        .expect("couldn't find game in manifest");
                    debug!("found game manifest for {name}\n{game:#?}");

                    let mut b = cfg
                        .backends
                        .iter()
                        .enumerate()
                        .find(|(i, b)| {
                            (cfg.default_backend.is_none() && *i == 0)
                                || (cfg.default_backend.as_ref() == Some(&b.name))
                        })
                        .map(|(_, b)| b.to_backend(name, &secrets))
                        .ok_or_else(|| anyhow!("no backends or default backend is invalid"))??;
                    if !args.dry_run {
                        let info = match SyncMgr::from_steam_game(game, app_id) {
                            Ok(v) => v,
                            Err(e) => {
                                error!("failed to get information about game: {e}");
                                return Err(e);
                            }
                        };
                        if let Some(sync_info) = info.are_local_files_newer(&b).await? {
                            warn!(
                                "found local files newer than local, showing confirmation box to the user..."
                            );

                            match ui::spawn_sync_confirm(sync_info)? {
                                SyncChoices::Download => {
                                    info.download(&b, true).await?;
                                }
                                SyncChoices::Continue => {}
                                SyncChoices::Exit => {
                                    return Ok(());
                                }
                            }
                        } else {
                            info.download(&b, false).await?;
                        }
                        drop(info); // its info is no longer valid after the command runs bc it may create new files
                    } else {
                        info!("not downloading files due to dry-run");
                    }

                    let launch_time = SystemTime::now();
                    debug!(
                        "we had an overhead of {}ms",
                        launch_time.duration_since(start_time)?.as_millis()
                    );
                    let mut c = std::process::Command::new(&command[0])
                        .args(command.iter().skip(1))
                        .spawn()
                        .unwrap();
                    c.wait().unwrap();

                    if args.dry_run || !largs.no_upload {
                        let info = match SyncMgr::from_steam_game(game, app_id) {
                            Ok(v) => v,
                            Err(e) => {
                                error!("failed to get information about game: {e}");
                                return Err(e);
                            }
                        };
                        info.upload(&mut b).await?;
                    } else {
                        debug!("not uploading due to --debug-no-upload or dry-run flag");
                    }
                }
                Store::Gog => todo!(),
                Store::Epic => todo!(),
                Store::Other => todo!(),
            }
        }
        cinc::args::Operation::DebugSyncDialog {
            remote_name,
            last_writer,
        } => {
            let now = Local::now().to_utc();
            let r = ui::spawn_sync_confirm(SyncIssueInfo {
                remote_name: remote_name.to_owned(),
                local_time: now,
                remote_time: now,
                remote_last_writer: last_writer.to_owned(),
            })?;
            println!("{r:?}");
        }
        cinc::args::Operation::DebugPskInput => {
            let psk = user_psk_input("example: ")?;
            println!("\n{psk}");
        }
        cinc::args::Operation::BackendsConfig(backends_args) => match backends_args {
            cinc::args::BackendsArgs::Add {
                name,
                ty,
                root,
                webdav_url,
                webdav_username,
                set_default,
            } => {
                let mut cfg = cfg;
                if cfg.backends.iter().any(|b| &b.name == name) {
                    bail!("a backend with the name '{name}' already exists!");
                }
                let backend_ty = match ty {
                    cinc::config::BackendType::Filesystem => BackendTy::Filesystem {
                        root: root.to_owned(),
                    },
                    cinc::config::BackendType::WebDav => {
                        let webdav_psk =
                            user_psk_input("enter webdav password, leave blank for no password: ")?;
                        let webdav_psk = if webdav_psk.is_empty() {
                            None
                        } else {
                            let use_secrets = secrets.available()
                                && user_input_yesno(
                                    "use system secrets API to store this password? (recommended) [Y/n]",
                                    true,
                                )?;
                            Some(if use_secrets {
                                let secret_name = Uuid::new_v4().to_string();
                                if !args.dry_run {
                                    secrets.add_item(&secret_name, &webdav_psk).await?;
                                }
                                Secret::SystemSecret(secret_name)
                            } else {
                                Secret::Plain(webdav_psk)
                            })
                        };
                        BackendTy::WebDav(WebDavInfo {
                            url: webdav_url.to_owned().expect("missing webdav url"),
                            username: webdav_username.to_owned().expect("missing webdav username"),
                            psk: webdav_psk,
                            root: root.to_owned(),
                        })
                    }
                };
                let new_backend = BackendInfo {
                    name: name.to_owned(),
                    info: backend_ty,
                };
                cfg.backends.push(new_backend);
                if *set_default {
                    cfg.default_backend = Some(name.to_owned());
                }
                write_cfg(&cfg, &cfg_file, args.dry_run)?;
                print_success!("successfully added backend '{name}'");
            }
            cinc::args::BackendsArgs::Remove { name } => {
                let mut cfg = cfg;
                if cfg.default_backend.as_deref() == Some(name) {
                    bail!("cannot remove backend '{name}' as it is currently the default backend");
                }

                let Some(i) = cfg
                    .backends
                    .iter()
                    .enumerate()
                    .find(|(_, b)| &b.name == name)
                    .map(|(i, _)| i)
                else {
                    bail!("cannot remove backend '{name}' as it does not exist");
                };
                cfg.backends.remove(i);
                if !args.dry_run && secrets.available() {
                    let used = cfg.used_keyring_ids().collect_vec();
                    secrets.garbage_collect(&used).await?;
                }
                write_cfg(&cfg, &cfg_file, args.dry_run)?;
                print_success!("successfully removed backend '{name}'");
            }
            cinc::args::BackendsArgs::List => {
                for (i, b) in cfg.backends.iter().enumerate() {
                    println!(
                        "- {} {}",
                        b.pretty_print(),
                        if Some(&b.name) == cfg.default_backend.as_ref()
                            || (cfg.default_backend.is_none() && i == 1)
                        {
                            "(default)"
                        } else {
                            ""
                        }
                    );
                }
            }
            cinc::args::BackendsArgs::SetDefault { name } => {
                let mut cfg = cfg;
                if !cfg.backends.iter().any(|b| &b.name == name) {
                    eprintln!("backend '{name}' does not exist");
                    exit(1);
                }
                cfg.default_backend = Some(name.to_owned());
                write_cfg(&cfg, &cfg_file, args.dry_run)?;
                print_success!("successfully set backend '{name}' as the default backend");
            }
        },
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    if std::env::args().any(|s| matches!(s.as_str(), "--help" | "-h" | "help")) {
        CliArgs::parse(); // this will print the help to the console
    }
    let is_without_term = std::env::args().contains("launch");
    if !std::env::args().contains("--no-panic-hook") {
        let prev_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            tracing::error!("panic! {info:?}");
            let msg = info
                .payload()
                .downcast_ref::<String>()
                .map(|s| s.to_owned())
                .or_else(|| {
                    info.payload()
                        .downcast_ref::<&str>()
                        .map(|s| (*s).to_owned())
                });
            if is_without_term {
                if let Some(msg) = msg {
                    let _ = ui::show_panic_dialog(msg, info.location());
                }
            }

            prev_hook(info);
        }));
    }
    if let Err(e) = run().await {
        tracing::error!("{e:?}");
        if is_without_term {
            let _ = ui::show_error_dialog(&e);
        } else {
            eprintln!("{}", format!("{e:?}").red());
        }
        std::process::exit(1);
    }
}
