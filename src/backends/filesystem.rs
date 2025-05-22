use std::{
    fs,
    path::{Path, PathBuf},
};

use super::{Result, StorageBackend};

pub struct FilesystemStore {
    root: PathBuf,
}
impl FilesystemStore {
    pub fn new(root: PathBuf) -> Result<Self, std::io::Error> {
        if !fs::exists(&root)? {
            fs::create_dir_all(&root)?;
        }
        Ok(Self { root })
    }

    fn filename(&self, f: &Path) -> PathBuf {
        self.root.join(f)
    }
}

impl StorageBackend for FilesystemStore {
    fn write_file(&mut self, at: &std::path::Path, bytes: &[u8]) -> Result<()> {
        Ok(fs::write(self.filename(at), bytes)?)
    }

    fn read_file(&self, at: &Path) -> Result<Vec<u8>> {
        Ok(fs::read(at)?)
    }

    fn exists(&self, f: &Path) -> Result<bool> {
        Ok(fs::exists(f)?)
    }
}
