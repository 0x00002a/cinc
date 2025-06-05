use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use filesystem::FilesystemStore;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use typesum::sumtype;
use webdav::WebDavStore;

use crate::{
    config::{BackendInfo, BackendTy, WebDavInfo},
    curr_crate_ver,
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
    #[serde(default = "default_last_write_cinc_version")]
    pub last_write_cinc_version: semver::Version,
}

impl SyncMetadata {
    /// Check that the version is compatible for a read
    ///
    /// This in practice requires that there is no breaking change difference between
    /// our current version and the one in the metadata. If there is then a read may not work
    /// and we should abort
    pub fn is_version_read_compatabible(&self) -> bool {
        check_version_compat_read(&self.last_write_cinc_version, &curr_crate_ver())
    }

    /// Check that the version is compatible for a write
    ///
    /// This in practice means we need to be either a non breaking change from the last writer
    /// OR a strictly younger breaking change, e.g. 0.3.0 is allowed to write when previousely
    /// 0.2.2 wrote but NOT the other way around as we want to enforce an upgrade here as jkkjk
    pub fn is_version_write_compatabible(&self) -> bool {
        check_version_compat_write(&self.last_write_cinc_version, &curr_crate_ver())
    }
}

const fn check_version_compat_read(curr: &semver::Version, prev: &semver::Version) -> bool {
    curr.major == prev.major && (curr.major != 0 || (curr.minor == prev.minor))
}

const fn check_version_compat_write(curr: &semver::Version, prev: &semver::Version) -> bool {
    curr.major >= prev.major && (curr.major != 0 || (curr.minor >= prev.minor))
}

fn default_last_write_cinc_version() -> semver::Version {
    semver::Version::new(0, 2, 1)
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
            last_write_cinc_version: curr_crate_ver(),
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

#[cfg(test)]
mod tests {
    use semver::Version;

    use crate::backends::{check_version_compat_read, check_version_compat_write};

    #[test]
    fn version_compat_read_leading_zero() {
        assert!(check_version_compat_read(
            &Version::parse("0.1.0").unwrap(),
            &Version::parse("0.1.1").unwrap()
        ));
        assert!(!check_version_compat_read(
            &Version::parse("0.1.0").unwrap(),
            &Version::parse("0.2.0").unwrap()
        ));
        assert!(!check_version_compat_read(
            &Version::parse("0.1.1").unwrap(),
            &Version::parse("0.2.1").unwrap()
        ));
    }

    #[test]
    fn version_compat_read_no_leading_zero() {
        assert!(check_version_compat_read(
            &Version::parse("1.0.0").unwrap(),
            &Version::parse("1.1.0").unwrap()
        ));
        assert!(!check_version_compat_read(
            &Version::parse("1.1.0").unwrap(),
            &Version::parse("0.2.0").unwrap()
        ));
        assert!(!check_version_compat_read(
            &Version::parse("0.1.0").unwrap(),
            &Version::parse("1.2.1").unwrap()
        ));
    }

    #[test]
    fn version_compat_write_leading_zero() {
        assert!(check_version_compat_write(
            &Version::parse("0.1.0").unwrap(),
            &Version::parse("0.1.1").unwrap()
        ));
        assert!(!check_version_compat_write(
            &Version::parse("0.1.0").unwrap(),
            &Version::parse("0.2.0").unwrap()
        ));
        assert!(check_version_compat_write(
            &Version::parse("0.2.1").unwrap(),
            &Version::parse("0.1.1").unwrap()
        ));
    }

    #[test]
    fn version_compat_write_no_leading_zero() {
        assert!(check_version_compat_write(
            &Version::parse("1.0.0").unwrap(),
            &Version::parse("1.1.0").unwrap()
        ));
        assert!(check_version_compat_write(
            &Version::parse("1.1.0").unwrap(),
            &Version::parse("0.2.0").unwrap()
        ));
        assert!(!check_version_compat_write(
            &Version::parse("0.1.0").unwrap(),
            &Version::parse("1.2.1").unwrap()
        ));
    }
}
