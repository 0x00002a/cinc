use fs_err as fs;
use std::path::{Path, PathBuf};
use tracing::debug;

use super::Result;

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

impl FilesystemStore {
    pub fn write_file(&mut self, at: &std::path::Path, bytes: &[u8]) -> Result<()> {
        let p = self.filename(at);
        debug!("writing to {p:?}");
        assert!(!p.is_dir());
        if !std::fs::exists(p.parent().unwrap())? {
            fs::create_dir_all(p.parent().unwrap())?;
        }
        Ok(fs::write(p, bytes)?)
    }

    pub fn read_file(&self, at: &Path) -> Result<Vec<u8>> {
        Ok(fs::read(self.filename(at))?)
    }

    pub fn exists(&self, f: &Path) -> Result<bool> {
        Ok(std::fs::exists(self.filename(f))?)
    }
}
