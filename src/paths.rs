use std::path::{Path, PathBuf};
use steamlocate::SteamDir;

/// Get the steam directory info
///
/// # Panics
/// If steam does not exist on the system or the environment is incorrectly set up
pub fn steam_dir() -> anyhow::Result<SteamDir> {
    Ok(steamlocate::SteamDir::locate()?)
}

pub fn log_dir() -> PathBuf {
    cache_dir().join("logs")
}

macro_rules! dir_override {
    ($name:ident : $fname:ident) => {
        #[cfg(not(debug_assertions))]
        pub fn $fname() -> PathBuf {
            dirs::$fname().map(|c| c.join("cinc")).unwrap_or_else(|| {
                tracing::warn!(
                    "could not locate system {} directory, falling back to ~/.cinc/{}",
                    stringify!($name),
                    stringify!(name)
                );

                let home = std::env::home_dir().expect("could not locate home directory");
                home.join(".cinc").join(stringify!(name))
            })
        }

        #[cfg(debug_assertions)]
        pub fn $fname() -> PathBuf {
            concat!("./cinc-data/", stringify!($name)).into()
        }
    };
}
dir_override!(data : data_dir);
dir_override!(config : config_dir);
dir_override!(cache : cache_dir);

/// Extract the postfix of two paths
///
/// # Panics
/// If base is not a prefix of child
pub fn extract_postfix<'p>(base: &'p Path, child: &'p Path) -> &'p Path {
    assert!(
        base.components()
            .zip(child.components())
            .all(|(a, b)| a == b),
        "child is not prefix of base"
    );
    child.strip_prefix(base).unwrap()
}

/// Extract the common prefix of two paths
///
/// ```
/// use std::path::Path;
/// use cinc::paths::extract_prefix;
///
/// let p1 = Path::new("hello/world/bingle/splung");
/// let p2 = Path::new("bingle/splung");
/// assert_eq!(extract_prefix(p1, p2), Path::new("hello/world"));
/// ```
///
/// # Panics
/// If base is not a prefix of child
pub fn extract_prefix<'p>(base: &'p Path, child: &Path) -> &'p Path {
    let postfix_len = base
        .components()
        .rev()
        .zip(child.components().rev())
        .take_while(|(a, b)| a == b)
        .count();
    assert!(
        postfix_len >= 1,
        "base is not prefix of child {base:?} and {child:?}"
    );

    let nb_comps = base.components().count();
    let prefix_bytes = base
        .components()
        .take(nb_comps - postfix_len)
        .fold(PathBuf::new(), |p, c| p.join(c))
        .as_os_str()
        .as_encoded_bytes()
        .len();
    // Safety: We just go these from as encoded bytes and we KNOW that it must be on a boundary
    unsafe {
        Path::new(std::ffi::OsStr::from_encoded_bytes_unchecked(
            &base.as_os_str().as_encoded_bytes()[..prefix_bytes],
        ))
    }
}

pub trait PathExt {
    /// Join but without the overwriting that happens if other is an absolute path
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

    use crate::paths::{extract_postfix, extract_prefix};

    #[test]
    fn postfix_extract() {
        let base = Path::new("/");
        let child = base.join("yipee").join("yay");
        assert_eq!(
            extract_postfix(base, &child),
            Path::new("yipee").join("yay")
        );
    }

    #[test]
    fn prefix_extract_unicode() {
        let child = Path::new("🥀");
        let base = Path::new("💀").join("😔").join("🥀");
        assert_eq!(extract_prefix(&base, child), Path::new("💀").join("😔"));
    }
}
