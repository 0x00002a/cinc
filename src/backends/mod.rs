use std::path::Path;

use chrono::Utc;
use filesystem::FilesystemStore;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use webdav::WebDavStore;

use crate::config::{BackendInfo, BackendTy, WebDavInfo};

pub mod filesystem;
pub mod webdav;

#[derive(Debug, Error)]
pub enum BackendError {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    FromUtf8(#[from] std::string::FromUtf8Error),

    #[error(transparent)]
    ChronoParse(#[from] chrono::ParseError),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    Reqwuest(#[from] reqwest::Error),
}
type Result<T, E = BackendError> = std::result::Result<T, E>;

pub const SYNC_TIME_FILE: &str = "mod-meta.json";

#[derive(Clone, Deserialize, Serialize, Debug)]
pub struct ModifiedMetadata {
    pub last_write_timestamp: chrono::DateTime<Utc>,
    pub last_write_hostname: String,
}

impl ModifiedMetadata {
    pub fn from_sys_info() -> Self {
        let last_write_timestamp = chrono::Local::now().to_utc();
        let last_write_hostname = gethostname::gethostname()
            .to_str()
            .expect("failed to convert hostname to string")
            .to_owned();
        Self {
            last_write_timestamp,
            last_write_hostname,
        }
    }
}

pub trait StorageBackend {
    fn write_file(&mut self, at: &Path, bytes: &[u8]) -> Result<()>;
    fn read_file(&self, at: &Path) -> Result<Vec<u8>>;
    fn exists(&self, f: &Path) -> Result<bool>;

    fn read_file_str(&self, at: &Path) -> Result<String> {
        Ok(String::from_utf8(self.read_file(at)?)?)
    }
    fn read_sync_time(&self) -> Result<Option<ModifiedMetadata>> {
        let sync_time_file = Path::new(SYNC_TIME_FILE);
        if !self.exists(sync_time_file)? {
            return Ok(None);
        }
        let f = self.read_file(sync_time_file)?;
        Ok(Some(serde_json::from_slice(&f)?))
    }

    fn write_sync_time(&mut self, metadata: &ModifiedMetadata) -> Result<()> {
        let data = serde_json::to_vec(metadata)?;
        self.write_file(Path::new(SYNC_TIME_FILE), &data)
    }
}
impl StorageBackend for Box<dyn StorageBackend> {
    fn write_file(&mut self, at: &Path, bytes: &[u8]) -> Result<()> {
        self.as_mut().write_file(at, bytes)
    }

    fn read_file(&self, at: &Path) -> Result<Vec<u8>> {
        self.as_ref().read_file(at)
    }

    fn exists(&self, f: &Path) -> Result<bool> {
        self.as_ref().exists(f)
    }
}

impl BackendInfo {
    pub fn to_backend(&self, game_name: &str) -> Result<Box<dyn StorageBackend>> {
        Ok(match &self.info {
            BackendTy::Filesystem { root } => Box::new(FilesystemStore::new(root.join(game_name))?),
            BackendTy::WebDav(web_dav_info) => Box::new(WebDavStore::new(WebDavInfo {
                root: web_dav_info.root.join(game_name),
                ..web_dav_info.to_owned()
            })),
        })
    }
}
