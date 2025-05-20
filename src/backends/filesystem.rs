use std::{
    fs,
    path::{Path, PathBuf},
};

use super::{Result, StorageBackend};

pub struct FilesystemStore {
    root: PathBuf,
}
impl FilesystemStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn filename(&self, f: &Path) -> PathBuf {
        self.root.join(f)
    }
}

impl StorageBackend for FilesystemStore {
    fn store_file(&mut self, at: &std::path::Path, bytes: &[u8]) -> Result<()> {
        Ok(fs::write(self.filename(at), bytes)?)
    }
}
