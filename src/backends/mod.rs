use std::path::Path;

use chrono::Utc;
use filesystem::FilesystemStore;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use typesum::sumtype;
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
#[sumtype]
pub enum StorageBackendTy {
    WebDav(WebDavStore),
    Fs(FilesystemStore),
}
pub struct StorageBackend {
    backend: StorageBackendTy,
}

macro_rules! forward {
    (fn $name:ident($($argname:ident : $argty:ty),*) -> $retr:ty) => {
        pub async fn $name (&self, $($argname : $argty),*) -> Result<$retr> {
            match &self.backend {
                StorageBackendTy::WebDav(b) => b.$name($($argname),*).await,
                StorageBackendTy::Fs(b) => b.$name($($argname),*).await,
            }
        }
    };

    (fn mut $name:ident($($argname:ident : $argty:ty),*) -> $retr:ty) => {
        pub async fn $name (&mut self, $($argname : $argty),*) -> Result<$retr> {
            match &mut self.backend {
                StorageBackendTy::WebDav(b) => b.$name($($argname),*).await,
                StorageBackendTy::Fs(b) => b.$name($($argname),*).await,
            }
        }
    }
}

impl StorageBackend {
    pub fn new(backend: impl Into<StorageBackendTy>) -> Self {
        Self {
            backend: backend.into(),
        }
    }
    forward!(fn mut write_file(at: &Path, bytes: &[u8]) -> ());
    forward!(fn read_file(at: &Path) -> Vec<u8>);
    forward!(fn exists(at: &Path) -> bool);

    pub async fn read_file_str(&self, at: &Path) -> Result<String> {
        Ok(String::from_utf8(self.read_file(at).await?)?)
    }
    pub async fn read_sync_time(&self) -> Result<Option<ModifiedMetadata>> {
        let sync_time_file = Path::new(SYNC_TIME_FILE);
        if !self.exists(sync_time_file).await? {
            return Ok(None);
        }
        let f = self.read_file(sync_time_file).await?;
        Ok(Some(serde_json::from_slice(&f)?))
    }

    pub async fn write_sync_time(&mut self, metadata: &ModifiedMetadata) -> Result<()> {
        let data = serde_json::to_vec(metadata)?;
        self.write_file(Path::new(SYNC_TIME_FILE), &data).await
    }
}

impl BackendInfo {
    pub fn to_backend(&self, game_name: &str) -> Result<StorageBackend> {
        Ok(match &self.info {
            BackendTy::Filesystem { root } => {
                StorageBackend::new(FilesystemStore::new(root.join(game_name))?)
            }
            BackendTy::WebDav(web_dav_info) => StorageBackend::new(WebDavStore::new(WebDavInfo {
                root: web_dav_info.root.join(game_name),
                ..web_dav_info.to_owned()
            })),
        })
    }
}
