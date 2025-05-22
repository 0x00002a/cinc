use fs_err as fs;
use std::path::{Path, PathBuf};
use tracing::debug;

use super::{Result, StorageBackend};

pub struct FilesystemStore {
    root: PathBuf,
}
impl FilesystemStore {
    pub fn new(root: PathBuf) -> Result<Self, std::io::Error> {
        if !std::fs::exists(&root)? {
            fs::create_dir_all(&root)?;
        }
        Ok(Self { root })
    }

    fn filename(&self, f: &Path) -> PathBuf {
        if !f.is_absolute() {
            self.root.join(f)
        } else {
            self.root.join(".".to_owned() + f.to_str().unwrap())
        }
    }
}

impl StorageBackend for FilesystemStore {
    fn write_file(&mut self, at: &std::path::Path, bytes: &[u8]) -> Result<()> {
        let p = self.filename(at);
        debug!("writing to {p:?}");
        if !std::fs::exists(p.parent().unwrap())? {
            fs::create_dir_all(p.parent().unwrap())?;
        }
        Ok(fs::write(p, bytes)?)
    }

    fn read_file(&self, at: &Path) -> Result<Vec<u8>> {
        Ok(fs::read(self.filename(at))?)
    }

    fn exists(&self, f: &Path) -> Result<bool> {
        Ok(std::fs::exists(self.filename(f))?)
    }
}
