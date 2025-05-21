use std::{env::home_dir, fs::File, io::BufReader};

use cinc::{
    args::{CliArgs, LaunchArgs},
    config::{Config, SteamId, default_manifest_url},
    manifest::GameManifests,
};
use clap::Parser;

fn grab_manifest(url: &str) -> String {
    reqwest::blocking::get(url).unwrap().text().unwrap()
}

fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();

    match &args.op {
        cinc::args::Operation::Daemon { config } => {
            let path = "./manifest.yml";
            if !std::fs::exists(path)? {
                let txt = grab_manifest(&default_manifest_url());
                std::fs::write(path, &txt)?;
            }
            let manifest: GameManifests =
                serde_yaml::from_reader(BufReader::new(File::open(path)?)).unwrap();
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
                });

            std::fs::write(format!("/home/ash/dump.txt"), app_id.unwrap().to_string()).unwrap();
            let mut c = std::process::Command::new(&steam_command[0])
                .args(steam_command.iter().skip(1))
                .spawn()
                .unwrap();
            c.wait().unwrap();
        }
    }
    Ok(())
}
