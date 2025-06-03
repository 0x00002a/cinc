use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::debug;

use super::Result;

pub struct FilesystemStore {
    root: PathBuf,
}
impl FilesystemStore {
    pub fn new(root: PathBuf) -> Result<Self, std::io::Error> {
        if !std::fs::exists(&root)? {
            std::fs::create_dir_all(&root)?;
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
    pub async fn write_file(&self, at: &std::path::Path, bytes: &[u8]) -> Result<()> {
        let p = self.filename(at);
        debug!("writing to {p:?}");
        assert!(!p.is_dir());
        if !std::fs::exists(p.parent().unwrap())? {
            fs::create_dir_all(p.parent().unwrap()).await?;
        }
        Ok(fs::write(p, bytes).await?)
    }

    pub async fn read_file(&self, at: &Path) -> Result<Vec<u8>> {
        Ok(fs::read(self.filename(at)).await?)
    }

    pub async fn exists(&self, f: &Path) -> Result<bool> {
        Ok(std::fs::exists(self.filename(f))?)
    }
}
