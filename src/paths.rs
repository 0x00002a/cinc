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
pub fn extract_postfix<'p>(base: &'p Path, child: &'p Path) -> &'p Path {
    assert!(base.is_dir());
    assert!(
        base.components()
            .zip(child.components())
            .all(|(a, b)| a == b),
        "child is not prefix of base"
    );
    child.strip_prefix(base).unwrap()
}

pub trait PathExt {
    fn join_good(&self, other: impl Into<PathBuf>) -> PathBuf;
}
impl PathExt for Path {
    /// Version of join that doesn't remove replace if absolute
    fn join_good(&self, other: impl Into<PathBuf>) -> PathBuf {
        let other = other.into();
        if !other.is_absolute() {
            self.join(other)
        } else {
            self.join(".".to_owned() + other.to_str().unwrap())
        }
    }
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
