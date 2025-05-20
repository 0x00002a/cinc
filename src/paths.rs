use steamlocate::SteamDir;

/// Get the steam directory info
///
/// # Panics
/// If steam does not exist on the system or the environment is incorrectly set up
pub fn steam_dir() -> anyhow::Result<SteamDir> {
    Ok(steamlocate::SteamDir::locate()?)
}
