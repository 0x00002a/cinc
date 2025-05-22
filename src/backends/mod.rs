use std::{path::Path, time};

use chrono::Utc;
use filesystem::FilesystemStore;
use thiserror::Error;

use crate::config::BackendInfo;

pub mod filesystem;

#[derive(Debug, Error)]
pub enum BackendError {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    FromUtf8(#[from] std::string::FromUtf8Error),

    #[error(transparent)]
    ChronoParse(#[from] chrono::ParseError),
}
type Result<T, E = BackendError> = std::result::Result<T, E>;

pub const SYNC_TIME_FILE: &str = "sync-time";

pub trait StorageBackend {
    fn write_file(&mut self, at: &Path, bytes: &[u8]) -> Result<()>;
    fn read_file(&self, at: &Path) -> Result<Vec<u8>>;
    fn exists(&self, f: &Path) -> Result<bool>;

    fn read_file_str(&self, at: &Path) -> Result<String> {
        Ok(String::from_utf8(self.read_file(at)?)?)
    }
    fn read_sync_time(&self) -> Result<Option<chrono::DateTime<Utc>>> {
        let sync_time_file = Path::new(SYNC_TIME_FILE);
        if !self.exists(sync_time_file)? {
            return Ok(None);
        }
        let f = self.read_file_str(sync_time_file)?;
        Ok(Some(f.parse()?))
    }

    fn write_sync_time(&mut self, time: &chrono::DateTime<Utc>) -> Result<()> {
        let data = format!("{time:?}");
        self.write_file(Path::new(SYNC_TIME_FILE), data.as_bytes())
    }
}

impl BackendInfo {
    pub fn to_backend(&self) -> Result<Box<dyn StorageBackend>> {
        Ok(match self {
            BackendInfo::Filesystem { root } => Box::new(FilesystemStore::new(root.to_owned())?),
        })
    }
}
