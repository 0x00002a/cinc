use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

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
impl LaunchConfig {
    pub fn sat(&self, curr_store: Option<Store>) -> bool {
        self.preds.iter().all(|p| p.sat(curr_store))
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LaunchPredicate {
    pub bit: Option<Arch>,
    pub os: Option<Os>,
    pub store: Option<Store>,
}

impl LaunchPredicate {
    pub fn sat(&self, curr_store: Option<Store>) -> bool {
        self.bit.map(|b| b.sat()).unwrap_or(true)
            && self.os.map(|o| o.sat()).unwrap_or(true)
            && (curr_store.is_none() || (self.store.is_none() || curr_store == self.store))
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FileConfig {
    #[serde(rename = "when", default)]
    pub preds: Vec<LaunchPredicate>,
    #[serde(default)]
    pub tags: Vec<FileTag>,
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

impl Os {
    pub fn sat(self) -> bool {
        #[cfg(target_os = "windows")]
        let target = Self::Windows;
        #[cfg(target_os = "macos")]
        let target = Self::Mac;
        #[cfg(target_os = "linux")]
        let target = Self::Linux;
        // dos isn't a valid rust target I think?

        self == target
    }
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

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Arch {
    #[serde(rename = "64")]
    X86_64,
    #[serde(rename = "32")]
    X86,
}
impl Arch {
    pub fn sat(self) -> bool {
        #[cfg(target_pointer_width = "64")]
        let target = Self::X86_64;
        #[cfg(target_pointer_width = "32")]
        let target = Self::X86;

        self == target
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SteamInfo {
    pub id: SteamId,
}

/// Key is the name
pub type GameManifests = HashMap<String, GameManifest>;

#[derive(Debug, Error)]
pub enum TemplateError {
    #[error("no closing deliminter in template string")]
    NoClosingDelim,
    #[error("failed to locate directory for '{0}'")]
    FailedToLocateDir(String),
    #[error("unknown template variable '{0}'")]
    UnknownVariable(String),
}

pub struct TemplateInfo {}

impl TemplatePath {
    pub fn new(s: String) -> Self {
        Self(s)
    }
    pub fn apply_substs(self, info: &TemplateInfo) -> Result<String, TemplateError> {
        let mut end = 0;
        while let Some(start) = self.0[end..].find('<').map(|s| s + end) {
            end = self.0[end..]
                .find('>')
                .ok_or(TemplateError::NoClosingDelim)?
                + end;
            println!("{start}-{end}");
            let var = &self.0[start + 1..end];
            let repl = match var {
                "xdgData" => dirs::data_dir()
                    .ok_or_else(|| TemplateError::FailedToLocateDir(var.to_owned()))?
                    .to_str()
                    .unwrap()
                    .to_owned(),
                _ => return Err(TemplateError::UnknownVariable(var.to_owned())),
            };
            end += 1;
            println!("repl: {repl}");
        }

        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::TemplatePath;

    #[test]
    fn template_apply_substs() {
        let p = TemplatePath::new("hello <xdgData> <there>".to_owned());
        p.apply_substs(&super::TemplateInfo {}).unwrap();
    }
}
