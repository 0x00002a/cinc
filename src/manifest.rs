use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::config::SteamId;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GameManifest {
    pub steam: Option<SteamInfo>,
    pub launch: Option<HashMap<TemplatePath, Vec<LaunchConfig>>>,
    pub cloud: Option<HashMap<Store, bool>>,
    pub files: Option<HashMap<TemplatePath, FileConfig>>,
}

/// Path which may contain substitutions such as <base> or <winLocalAppData>
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TemplatePath(String);

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LaunchConfig {
    #[serde(rename = "when")]
    pub preds: Vec<LaunchPredicate>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LaunchPredicate {
    pub bit: Option<Arch>,
    pub os: Option<Os>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FileConfig {
    #[serde(rename = "when")]
    pub preds: Option<Vec<LaunchPredicate>>,
    pub tags: Option<Vec<FileTag>>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum FileTag {
    Save,
    Config,
    #[serde(other)]
    Other,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Os {
    Windows,
    Linux,
    Mac,
    Dos,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Copy)]
#[serde(rename_all = "snake_case")]
pub enum Store {
    Steam,
    Gog,
    Epic,
    #[serde(other)]
    Other,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Arch {
    #[serde(rename = "64")]
    X86_64,
    #[serde(rename = "32")]
    X86,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SteamInfo {
    pub id: SteamId,
}

/// Key is the name
pub type GameManifests = HashMap<String, GameManifest>;
