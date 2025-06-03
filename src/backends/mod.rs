use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use filesystem::FilesystemStore;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use typesum::sumtype;
use webdav::WebDavStore;

use crate::{
    config::{BackendInfo, BackendTy, WebDavInfo},
    manifest::{TemplateError, TemplateInfo, TemplatePath},
    secrets::SecretsApi,
};

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
    RonDe(#[from] ron::de::SpannedError),

    #[error(transparent)]
    RonSer(#[from] ron::Error),

    #[error(transparent)]
    Reqwuest(#[from] reqwest::Error),

    #[error(transparent)]
    SecretService(#[from] secret_service::Error),

    #[error("could not find secret '{0}' in system store")]
    CouldNotLocateSecret(String),
}

type Result<T, E = BackendError> = std::result::Result<T, E>;

pub const SYNC_TIME_FILE: &str = "mod-meta.ron";

#[derive(Clone, Deserialize, Serialize, Debug)]
pub struct SyncMetadata {
    pub last_write_timestamp: chrono::DateTime<Utc>,
    pub last_write_hostname: String,
    pub file_table: FileMetaTable,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FileMetaEntry {
    pub template: TemplatePath,
    pub remote_path: PathBuf,
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FileMetaTable {
    pub entries: Vec<FileMetaEntry>,
    /// Oldest modifed time of the files in the archive
    pub oldest_modified_time: DateTime<Utc>,
}
impl FileMetaTable {
    pub fn localise_entries(
        &self,
        info: &TemplateInfo,
    ) -> impl Iterator<Item = Result<PathBuf, TemplateError>> {
        self.entries
            .iter()
            .map(|e| e.template.apply_substs(info).map(PathBuf::from))
    }
}

impl SyncMetadata {
    pub fn from_sys_info(file_table: FileMetaTable) -> Self {
        let last_write_timestamp = chrono::Local::now().to_utc();
        let last_write_hostname = gethostname::gethostname()
            .to_str()
            .expect("failed to convert hostname to string")
            .to_owned();
        Self {
            last_write_timestamp,
            last_write_hostname,
            file_table,
        }
    }
}
#[sumtype]
pub enum StorageBackendTy<'s> {
    WebDav(WebDavStore<'s>),
    Fs(FilesystemStore),
}
pub struct StorageBackend<'s> {
    backend: StorageBackendTy<'s>,
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

impl<'s> StorageBackend<'s> {
    pub fn new(backend: impl Into<StorageBackendTy<'s>>) -> Self {
        Self {
            backend: backend.into(),
        }
    }
    forward!(fn write_file(at: &Path, bytes: &[u8]) -> ());
    forward!(fn read_file(at: &Path) -> Vec<u8>);
    forward!(fn exists(at: &Path) -> bool);

    pub async fn read_file_str(&self, at: &Path) -> Result<String> {
        Ok(String::from_utf8(self.read_file(at).await?)?)
    }
    pub async fn read_sync_time(&self) -> Result<Option<SyncMetadata>> {
        let sync_time_file = Path::new(SYNC_TIME_FILE);
        if !self.exists(sync_time_file).await? {
            return Ok(None);
        }
        let f = self.read_file(sync_time_file).await?;
        Ok(Some(ron::de::from_bytes(&f)?))
    }

    pub async fn write_sync_time(&self, metadata: &SyncMetadata) -> Result<()> {
        let data = ron::ser::to_string(metadata)?;
        self.write_file(Path::new(SYNC_TIME_FILE), data.as_bytes())
            .await
    }
}

impl BackendInfo {
    pub fn to_backend<'a>(
        &self,
        game_name: &str,
        secrets: &'a SecretsApi,
    ) -> Result<StorageBackend<'a>> {
        Ok(match &self.info {
            BackendTy::Filesystem { root } => {
                StorageBackend::new(FilesystemStore::new(root.join(game_name))?)
            }
            BackendTy::WebDav(web_dav_info) => StorageBackend::new(WebDavStore::new(
                WebDavInfo {
                    root: web_dav_info.root.join(game_name),
                    ..web_dav_info.to_owned()
                },
                secrets,
            )),
        })
    }
}
