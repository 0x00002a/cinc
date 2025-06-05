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

use anyhow::{Context, Result, bail};
use chrono::Local;
use cinc::{
    args::{CliArgs, LaunchArgs},
    config::{BackendInfo, BackendTy, Config, DEFAULT_MANIFEST_URL, Secret, WebDavInfo},
    curr_crate_ver,
    manifest::GameManifests,
    paths::{cache_dir, config_dir, log_dir},
    platform::{IncomaptibleCincVersionError, LaunchInfo},
    secrets::SecretsApi,
    ui::{self, SyncIssueInfo},
};
use clap::Parser;
use itertools::Itertools;
use tracing::{debug, info, warn};
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
        match bincode::serde::decode_from_std_read(
            &mut BufReader::new(File::open(path)?),
            bincode::config::standard(),
        ) {
            Ok(v) => Ok(v),
            Err(_) => {
                warn!(
                    "failed to decode manifest, assuming it is an old version and grabbing from the server again"
                );
                std::fs::remove_file(path)?;
                update_manifest(url).await
            }
        }
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

    init_file_logging().expect("failed to init file logging");

    let secrets = SecretsApi::new().await?;
    let cfg_file = args.config_path.map(Ok).unwrap_or_else(get_cfg_path)?;
    let cfg = read_config(&cfg_file)?;
    let cfg_errs = cfg.validate(&secrets).await;
    if !cfg_errs.is_empty() {
        bail!(
            "errors in config\n{}",
            cfg_errs
                .iter()
                .map(|e| format!("- {e}"))
                .collect_vec()
                .join("\n")
        );
    }

    let manifest_url = cfg.manifest_url.as_deref().unwrap_or(DEFAULT_MANIFEST_URL);
    if args.update {
        update_manifest(manifest_url).await?;
    }
    debug!("secrets available: {}", secrets.available());

    match &args.op {
        cinc::args::Operation::Launch(
            largs @ LaunchArgs {
                no_download,
                command,
                ..
            },
        ) => {
            debug!("launch command: {command:?}");
            if cfg.backends.is_empty() {
                bail!("invalid config: at least one backend must be specified");
            }
            if *no_download && !ui::show_no_download_confirmation()? {
                tracing::info!("aborting due to user deciding not to continue");
                return Ok(());
            }
            let manifest_start = SystemTime::now();
            let manifests = get_game_manifests(manifest_url).await?;
            let manifest_end = SystemTime::now();
            debug!(
                "parsing the manifest took {}ms",
                manifest_end.duration_since(manifest_start)?.as_millis()
            );
            let platform = LaunchInfo::new(&cfg, &manifests, &secrets, largs)?;

            if !args.dry_run {
                platform.sync_down().await?;
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
                platform.sync_up().await?;
            } else {
                debug!("not uploading due to --debug-no-upload or dry-run flag");
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
                                    "use system secrets API to store this password? (recommended) [Y/n]: ",
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
                    cfg.default_backend = name.to_owned();
                }
                write_cfg(&cfg, &cfg_file, args.dry_run)?;
                print_success!("successfully added backend '{name}'");
            }
            cinc::args::BackendsArgs::Remove { name } => {
                let mut cfg = cfg;
                if &cfg.default_backend == name {
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
                for b in cfg.backends.iter() {
                    println!(
                        "- {} {}",
                        b.pretty_print(),
                        if b.name == cfg.default_backend {
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
                cfg.default_backend = name.to_owned();
                write_cfg(&cfg, &cfg_file, args.dry_run)?;
                print_success!("successfully set backend '{name}' as the default backend");
            }
        },
        cinc::args::Operation::DebugVersionIncompat { read } => {
            let curr_v = curr_crate_ver();
            let new_v = semver::Version::new(curr_v.major + 1, curr_v.minor, curr_v.patch);
            Err(IncomaptibleCincVersionError {
                server_version: new_v,
                read: *read,
            })?;
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    if std::env::args().any(|s| matches!(s.as_str(), "--help" | "-h" | "help")) {
        CliArgs::parse(); // this will print the help to the console
    }
    let is_without_term =
        std::env::args().any(|a| matches!(a.as_str(), "launch" | "debug-version-incompat"));
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
                    wrap(ui::show_panic_dialog(msg, info.location()));
                }
            }

            prev_hook(info);
        }));
    }
    if let Err(e) = run().await {
        tracing::error!("{e:?}");
        if is_without_term {
            if let Some(e @ IncomaptibleCincVersionError { .. }) = e.downcast_ref() {
                wrap(ui::version_mismatch(e));
            } else {
                wrap(ui::show_error_dialog(&e));
            }
        } else {
            eprintln!("{}", format!("{e:?}").red());
        }
        std::process::exit(1);
    }

    fn wrap<E: std::fmt::Debug, T>(r: Result<T, E>) {
        if let Err(e) = r {
            tracing::error!("error while displaying dialog to the user, uh oh: {e:?}");
        }
    }
}
