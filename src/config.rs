use std::path::PathBuf;

use serde::{Deserialize, Serialize};


#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    pub games: Vec<GameInfo>
}


#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GameInfo {
    pub steam_id: Option<SteamId>,
    pub save_dirs: Vec<PathBuf>,
}


#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SteamId(u64);
