use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    pub backend: BackendInfo,
    pub games: Vec<GameInfo>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BackendInfo {
    Filesystem { root: PathBuf },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GameInfo {
    pub steam_id: Option<SteamId>,
    pub save_dirs: Vec<PathBuf>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SteamId(u64);

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum BackendType {
    Filesystem,
}
