use std::{
    env::home_dir,
    fs::File,
    io::{BufReader, BufWriter},
};

use anyhow::{Context, Result};
use cinc::{
    args::{CliArgs, LaunchArgs},
    config::{Config, SteamId, default_manifest_url},
    manifest::GameManifests,
    paths::cache_dir,
};
use clap::Parser;
use tracing::info;
use tracing_subscriber::{Layer, layer::SubscriberExt, util::SubscriberInitExt};

fn grab_manifest(url: &str) -> Result<String> {
    Ok(reqwest::blocking::get(url)?.text()?)
}
fn init_logging() {
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

fn main() -> anyhow::Result<()> {
    init_logging();
    let args = CliArgs::parse();

    match &args.op {
        cinc::args::Operation::Init {} => {
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
        cinc::args::Operation::Launch(LaunchArgs { steam_command, .. }) => {
            let home = home_dir().unwrap();
            let home = home.to_str().unwrap();
            let manifests = get_game_manifests()?;
            let app_id = steam_command
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

            std::fs::write(format!("/home/ash/dump.txt"), format!("{game:#?}")).unwrap();
            let mut c = std::process::Command::new(&steam_command[0])
                .args(steam_command.iter().skip(1))
                .spawn()
                .unwrap();
            c.wait().unwrap();
        }
    }
    Ok(())
}
