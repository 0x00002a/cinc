use std::{fmt::Display, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::paths::data_dir;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    #[serde(alias = "backend")]
    pub backends: Vec<BackendInfo>,
    /// Default backend to use, defaults to the first backend
    pub default_backend: Option<String>,

    #[serde(default = "default_manifest_url")]
    pub manifest_url: String,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            backends: vec![BackendInfo {
                name: "local-store".to_owned(),
                info: Default::default(),
            }],
            manifest_url: default_manifest_url(),
            default_backend: None,
        }
    }
}

pub fn default_manifest_url() -> String {
    "https://raw.githubusercontent.com/mtkennerly/ludusavi-manifest/master/data/manifest.yaml"
        .to_owned()
}

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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WebDavInfo {
    pub url: String,
    pub username: String,
    pub psk: Option<String>,
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
