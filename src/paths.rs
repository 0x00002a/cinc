use std::{
    env::home_dir,
    ffi::OsStr,
    os::unix::ffi::OsStrExt,
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

/// Extract the postfix of two paths
///
/// # Panics
/// If base is not a directory
/// If base is not a prefix of child
pub fn extract_postfix(base: &Path, child: &Path) -> PathBuf {
    assert!(base.is_dir());
    assert!(
        base.components()
            .zip(child.components())
            .all(|(a, b)| a == b),
        "child is not prefix of base"
    );

    let mut prefix_comp_len = child.components().count() - base.components().count();
    if !child.is_dir() {
        prefix_comp_len -= 1;
    }
    let prefix_len: usize = child
        .components()
        .take(prefix_comp_len)
        .map(|p| p.as_os_str().len())
        .sum();
    let postfix = &child.as_os_str().as_bytes()[prefix_len..];
    PathBuf::from(OsStr::from_bytes(postfix))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::paths::extract_postfix;

    #[test]
    fn postfix_extract() {
        let base = Path::new("/");
        let child = base.join("yipee").join("yay");
        assert_eq!(
            extract_postfix(base, &child),
            Path::new("yipee").join("yay")
        );
    }
}
