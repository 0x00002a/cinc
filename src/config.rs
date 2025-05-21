use std::{fmt::Display, path::PathBuf};

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};

use crate::paths::steam_dir;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    pub backend: BackendInfo,
    #[serde(alias = "game")]
    pub games: Vec<GameInfoConfig>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BackendInfo {
    Filesystem { root: PathBuf },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GameInfoConfig {
    pub steam_id: Option<SteamId>,
    pub save_dirs: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
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
