use std::path::Path;

use filesystem::FilesystemStore;
use thiserror::Error;

use crate::config::BackendInfo;

pub mod filesystem;

#[derive(Debug, Error)]
pub enum BackendError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
type Result<T, E = BackendError> = std::result::Result<T, E>;

pub trait StorageBackend {
    fn store_file(&mut self, at: &Path, bytes: &[u8]) -> Result<()>;
}

impl BackendInfo {
    pub fn to_backend(&self) -> Box<dyn StorageBackend> {
        match self {
            BackendInfo::Filesystem { root } => Box::new(FilesystemStore::new(root.to_owned())),
        }
    }
}
