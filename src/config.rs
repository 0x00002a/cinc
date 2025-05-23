use std::{fmt::Display, path::PathBuf};

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};

use crate::paths::steam_dir;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    #[serde(alias = "backend")]
    pub backends: Vec<BackendInfo>,

    #[serde(default = "default_manifest_url")]
    pub manifest_url: String,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            backends: vec![Default::default()],
            manifest_url: default_manifest_url(),
        }
    }
}

pub fn default_manifest_url() -> String {
    "https://raw.githubusercontent.com/mtkennerly/ludusavi-manifest/master/data/manifest.yaml"
        .to_owned()
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BackendInfo {
    Filesystem { root: PathBuf },
    WebDav(WebDavInfo),
}
impl Default for BackendInfo {
    fn default() -> Self {
        Self::Filesystem {
            root: dirs::data_dir().unwrap().join("cinc").join("local-store"),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WebDavInfo {
    pub url: String,
    pub username: String,
    pub psk: String,
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
}

/// Game info after all the paths have been resolved
#[derive(Debug, Clone)]
pub struct GameInfo {
    pub steam: Option<SteamInfo>,
    pub save_dirs: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct SteamInfo {
    pub app_id: SteamId,
    pub app: steamlocate::App,
    /// The steam library this app is located within
    pub library: steamlocate::Library,
}

impl SteamInfo {
    pub fn new(app_id: SteamId) -> Result<Self> {
        let steam = steam_dir()?;
        let (app, library) = steam.find_app(app_id.id())?.ok_or_else(|| {
            anyhow!("could not find information for steam app with id '{app_id}'")
        })?;
        Ok(Self {
            app_id,
            app,
            library,
        })
    }
}

impl GameInfoConfig {
    pub fn resolve(&self) -> anyhow::Result<GameInfo> {
        let steam = self.steam_id.map(SteamInfo::new).transpose()?;
        let save_dirs = self
            .save_dirs
            .iter()
            .map(|p| self.replace_path_vars(p, steam.as_ref()))
            .collect::<anyhow::Result<_>>()?;
        Ok(GameInfo { steam, save_dirs })
    }
    fn replace_path_vars(&self, p: &str, sinfo: Option<&SteamInfo>) -> anyhow::Result<PathBuf> {
        let compat_user_root = |sinfo: &SteamInfo| {
            sinfo.library.path().join(format!(
                "steamapps/compatdata/{steam_id}/pfx/drive_c/users/steamuser",
                steam_id = sinfo.app_id,
            ))
        };
        let sinfo_req =
            |var| sinfo.ok_or_else(|| anyhow!("variable ${var} requires a valid steam id to use"));
        let replacements = [("$APPDATA", |me: &Self| {
            Ok::<_, anyhow::Error>(compat_user_root(sinfo_req("APPDATA")?))
        })];
        let r = replacements
            .iter()
            .try_fold(p.to_owned(), |p, (var, f)| {
                Ok::<_, anyhow::Error>(
                    p.replace(
                        var,
                        f(self)?
                            .to_str()
                            .ok_or_else(|| anyhow!("failed to convert path to string"))?,
                    ),
                )
            })?
            .into();
        Ok(r)
    }
}
