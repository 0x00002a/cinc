use std::{
    env::home_dir,
    path::{Path, PathBuf},
};

use steamlocate::SteamDir;
use tracing::warn;

/// Get the steam directory info
///
/// # Panics
/// If steam does not exist on the system or the environment is incorrectly set up
pub fn steam_dir() -> anyhow::Result<SteamDir> {
    Ok(steamlocate::SteamDir::locate()?)
}

#[cfg(not(debug_assertions))]
pub fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .map(|d| d.join("cinc"))
        .unwrap_or_else(|| {
            warn!("could not locate system cache directory, falling back to ~/.cinc/cache");
            let home = home_dir().expect("could not locate home directory");
            home.join(".cinc").join("cache")
        })
}
#[cfg(debug_assertions)]
pub fn cache_dir() -> PathBuf {
    "./cinc-data/cache".into()
}

pub fn log_dir() -> PathBuf {
    cache_dir().join("logs")
}
