use std::path::Path;

use thiserror::Error;

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
