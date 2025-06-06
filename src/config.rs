use std::{fmt::Display, path::PathBuf, str::FromStr};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{paths::data_dir, secrets::SecretsApi};

#[derive(thiserror::Error, Debug)]
pub enum ConfigValidationError {
    #[error("manifest url is malformed")]
    MalformedManifestUrl,
    #[error("default backend specifies backend that does not exist '{0}'")]
    InvalidDefaultBackend(String),
    #[error("secret '{0}' for backend '{1}' does not exist in the system keyring")]
    SecretDoesNotExist(String, String),

    #[error("backend '{0}' uses secrets but the system keyring is unavailable")]
    SecretsUnavailable(String),

    #[error("failed to contact secrets service {0:?}")]
    FailedToGetSecrets(secret_service::Error),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    #[serde(alias = "backend")]
    pub backends: Vec<BackendInfo>,
    /// Default backend to use
    pub default_backend: String,

    pub manifest_url: Option<String>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            backends: vec![BackendInfo {
                name: "local-store".to_owned(),
                info: Default::default(),
            }],
            manifest_url: None,
            default_backend: "local-store".to_owned(),
        }
    }
}
impl Config {
    pub fn used_keyring_ids(&self) -> impl Iterator<Item = &str> {
        self.backends
            .iter()
            .filter_map(|b| {
                if let BackendTy::WebDav(i) = &b.info {
                    Some(i)
                } else {
                    None
                }
            })
            .filter_map(|i| i.psk.as_ref())
            .filter_map(|p| {
                if let Secret::SystemSecret(s) = p {
                    Some(s.as_str())
                } else {
                    None
                }
            })
    }
    pub async fn validate(&self, secrets: &SecretsApi<'_>) -> Vec<ConfigValidationError> {
        let mut errs = Vec::new();
        if self.backends.iter().all(|b| b.name != self.default_backend) {
            errs.push(ConfigValidationError::InvalidDefaultBackend(
                self.default_backend.clone(),
            ));
        }
        if self
            .manifest_url
            .as_ref()
            .map(|u| reqwest::Url::parse(u).is_err())
            .unwrap_or(false)
        {
            errs.push(ConfigValidationError::MalformedManifestUrl);
        }
        for b in &self.backends {
            if let BackendTy::WebDav(WebDavInfo {
                psk: Some(Secret::SystemSecret(key)),
                ..
            }) = &b.info
            {
                if !secrets.available() {
                    errs.push(ConfigValidationError::SecretsUnavailable(b.name.clone()));
                } else {
                    match secrets.get_item(key).await {
                        Ok(None) => errs.push(ConfigValidationError::SecretDoesNotExist(
                            key.clone(),
                            b.name.clone(),
                        )),
                        Err(e) => errs.push(ConfigValidationError::FailedToGetSecrets(e)),
                        _ => {}
                    }
                }
            }
        }
        errs
    }
}

pub const DEFAULT_MANIFEST_URL: &str =
    "https://raw.githubusercontent.com/mtkennerly/ludusavi-manifest/master/data/manifest.yaml";

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BackendTy {
    Filesystem { root: PathBuf },
    WebDav(WebDavInfo),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BackendInfo {
    /// Name of the backend
    pub name: String,
    #[serde(flatten)]
    pub info: BackendTy,
}

impl BackendInfo {
    /// Pretty print for console output
    pub fn pretty_print(&self) -> String {
        match &self.info {
            BackendTy::Filesystem { root } => format!("filesystem at '{root:?}'"),
            BackendTy::WebDav(web_dav_info) => format!(
                "webdav at '{url}/{root:?}' with username {username}",
                root = web_dav_info.root,
                username = web_dav_info.username,
                url = web_dav_info.url
            ),
        }
    }
}

impl Default for BackendTy {
    fn default() -> Self {
        Self::Filesystem {
            root: data_dir().join("local-store"),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(tag = "ty", content = "value")]
#[serde(rename_all = "snake_case")]
pub enum Secret {
    /// Secret stored in the systems keychain
    #[serde(rename = "keychain")]
    SystemSecret(String),

    /// Plaintext storage directly inline
    Plain(String),
}

impl Display for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("keyring:")?;
        f.write_str(match self {
            Secret::SystemSecret(s) => s,
            Secret::Plain(s) => s,
        })
    }
}

const SYS_SECRET_PREFIX: &str = "keyring:";

#[derive(Error, Debug)]
#[error("failed to parse secret {input}")]
pub struct SecretParseError {
    input: String,
}

impl FromStr for Secret {
    type Err = SecretParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(if let Some(s) = s.strip_prefix(SYS_SECRET_PREFIX) {
            Self::SystemSecret(s.to_owned())
        } else {
            Self::Plain(s.to_owned())
        })
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WebDavInfo {
    pub url: String,
    pub username: String,
    pub psk: Option<Secret>,
    pub root: PathBuf,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GameInfoConfig {
    pub steam_id: Option<SteamId>,
    pub save_dirs: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SteamId(u32);

impl SteamId {
    pub fn new(id: u32) -> Self {
        Self(id)
    }
    pub fn id(self) -> u32 {
        self.0
    }
}

#[derive(Error, Debug)]
pub enum SteamIdParseError {
    #[error("input was not a number")]
    NonNumeric,
}

impl FromStr for SteamId {
    type Err = SteamIdParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let id: u32 = s.parse().map_err(|_| SteamIdParseError::NonNumeric)?;
        Ok(Self::new(id))
    }
}

impl Display for SteamId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{}", self.0))
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SteamId64(u64);

impl SteamId64 {
    pub fn new(id: u64) -> Self {
        Self(id)
    }
    pub fn to_id3(self) -> u32 {
        // from: https://github.com/AlexHodgson/steamid-converter/blob/098e09c46b1e5e740be708d22c68a82f81632512/steamid_converter/Converter.py#L129
        const ID64_BASE: i64 = 76561197960265728;
        let with_off = self.0 as i64 - ID64_BASE;
        let acc_type = with_off % 2;
        let acc_id = ((with_off - acc_type) / 2) + acc_type;
        (acc_id * 2 - acc_type) as u32
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum BackendType {
    Filesystem,
    WebDav,
}

#[cfg(test)]
mod tests {
    use crate::config::Secret;

    use super::Config;

    #[test]
    fn secret_is_stripped_of_keyring_prefix() {
        let p = "keyring:hello";
        assert_eq!(
            p.parse::<Secret>().unwrap(),
            Secret::SystemSecret("hello".to_owned())
        );
    }

    #[test]
    fn plain_secret_serializes_with_no_prefix() {
        let p = "keyring:hello";
        assert_eq!(
            p.parse::<Secret>().unwrap(),
            Secret::SystemSecret("hello".to_owned())
        );
    }

    #[test]
    fn example_v0_1_config_can_parse() {
        let example_cfg = r#"
default_backend = "cloud"

[[backends]]
name = "local-store"
type = "filesystem"
root = "./cinc-data/data/local-store"

[[backends]]
name = "cloud"
type = "web_dav"
url = "https://webdav.example.com/files/"
username = "example@example.com"
psk = { ty = "plain", value = "bingle" } # should parse as plain
root = "/cinc"

        "#;
        let r: Config = toml::from_str(example_cfg).unwrap();
        assert_eq!(r.default_backend, "cloud".to_owned());
        assert_eq!(r.backends.len(), 2);
    }
}
