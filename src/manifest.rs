use std::{
    collections::HashMap,
    env,
    ops::Deref,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{config::SteamId, paths::PathExt};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct GameManifest {
    pub steam: Option<SteamInfo>,
    pub gog: Option<GogInfo>,
    #[serde(default)]
    pub files: HashMap<TemplatePath, FileConfig>,
    #[serde(default)]
    pub launch: HashMap<TemplatePath, Vec<LaunchConfig>>,
    pub install_dir: Option<GameInstallDir>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
struct EmptyObj {}

#[derive(Serialize, Deserialize, Debug, Clone)]
// the manifest format is weird and has a hashmap here
pub struct GameInstallDir(HashMap<String, EmptyObj>);

impl Deref for GameInstallDir {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0.keys().next().unwrap().as_str()
    }
}

/// Path which may contain substitutions such as <base> or <winLocalAppData>
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TemplatePath(String);

#[derive(Default, Debug, Clone, Copy)]
pub struct PlatformInfo {
    pub store: Option<Store>,
    pub wine: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LaunchConfig {
    #[serde(rename = "when")]
    pub preds: Vec<LaunchPredicate>,
}
impl LaunchConfig {
    pub fn sat(&self, info: PlatformInfo) -> bool {
        self.preds.iter().all(|p| p.sat(info))
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LaunchPredicate {
    pub bit: Option<Arch>,
    pub os: Option<Os>,
    pub store: Option<Store>,
}

impl LaunchPredicate {
    pub fn sat(&self, info: PlatformInfo) -> bool {
        self.bit.map(|b| b.sat()).unwrap_or(true)
            && self.os.map(|o| o.sat(info.wine)).unwrap_or(true)
            && (info.store.is_none() || (self.store.is_none() || info.store == self.store))
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
    pub fn sat(self, wine: bool) -> bool {
        if wine && self == Os::Windows {
            return true;
        }

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
pub type SteamInfo = StoreInfo<SteamId>;
pub type GogInfo = StoreInfo<u32>;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StoreInfo<Id> {
    pub id: Id,
}

/// Key is the name
pub type GameManifests = HashMap<String, GameManifest>;

#[derive(Debug, Error)]
pub enum TemplateError {
    #[error("no closing deliminter in template string")]
    NoClosingDelim,
    #[error("failed to locate directory for '{0}'")]
    FailedToLocateDir(String),
    #[error("tried to substitute using a variable that is not available on this system '{0}'")]
    VariableNotAvailable(&'static str),
    #[error("unknown template variable '{0}'")]
    UnknownVariable(String),
}

pub struct TemplateInfo {
    pub win_prefix: PathBuf,
    pub win_user: String,
    pub base_dir: Option<PathBuf>,
    /// directory where games are installed. Under steam it should be the steam of the game otherwise the wine prefix usually
    pub root: Option<PathBuf>,
    pub store_user_id: Option<String>,
    pub home_dir: Option<PathBuf>,
    pub xdg_config: Option<PathBuf>,
    pub xdg_data: Option<PathBuf>,
    /// Install dir of the game in the manifest, or the game name
    pub install_dir: Option<PathBuf>,
}

impl TemplatePath {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    /// Get the raw path, note that is this almost certainly not a valid
    /// filesystem path and using it without applying substitutions WILL cause issues
    pub fn as_raw_path(&self) -> &Path {
        Path::new(&self.0)
    }

    fn do_repl(var: &str, info: &TemplateInfo) -> Result<PathBuf, TemplateError> {
        let repl = match var {
            "xdgData" => info
                .xdg_data
                .to_owned()
                .or_else(dirs::data_dir)
                .ok_or_else(|| TemplateError::FailedToLocateDir(var.to_owned()))?,
            "xdgConfig" => info
                .xdg_config
                .to_owned()
                .or_else(dirs::config_dir)
                .ok_or_else(|| TemplateError::FailedToLocateDir(var.to_owned()))?,

            "home" => info
                .home_dir
                .to_owned()
                .or_else(env::home_dir)
                .ok_or_else(|| TemplateError::FailedToLocateDir(var.to_owned()))?,
            "winAppData" => info
                .win_prefix
                .join("users") // linux capitalisation senstive filesystems require this to be lowercase and windows doesn't care
                .join(&info.win_user)
                .join("AppData")
                .join("Roaming"),

            "winLocalAppData" => info
                .win_prefix
                .join("users")
                .join(&info.win_user)
                .join("AppData")
                .join("Local"),
            "winDocuments" => info
                .win_prefix
                .join("users")
                .join(&info.win_user)
                .join("Documents"),
            "base" => info
                .base_dir
                .clone()
                .ok_or(TemplateError::VariableNotAvailable("<base>"))
                .or_else(|_| {
                    Ok(Self::do_repl("root", info)?.join_good(Self::do_repl("game", info)?))
                })?,
            "root" => info
                .root
                .to_owned()
                .ok_or(TemplateError::VariableNotAvailable("<root>"))?,
            "storeUserId" => info
                .store_user_id
                .as_ref()
                .ok_or(TemplateError::VariableNotAvailable("<storeUserId>"))?
                .to_owned()
                .into(),
            "game" => info
                .install_dir
                .as_ref()
                .ok_or(TemplateError::VariableNotAvailable("<game>"))?
                .to_owned(),
            _ => return Err(TemplateError::UnknownVariable(var.to_owned())),
        };
        Ok(repl)
    }

    pub fn apply_substs(&self, info: &TemplateInfo) -> Result<String, TemplateError> {
        let mut end = 0;
        let mut substs = Vec::new();
        while let Some(start) = self.0[end..].find('<').map(|s| s + end) {
            end = self.0[end..]
                .find('>')
                .ok_or(TemplateError::NoClosingDelim)?
                + end;
            let var = &self.0[start + 1..end];
            let repl = Self::do_repl(var, info)?;
            substs.push((start, end, repl));
            end += 1;
        }

        // rebuild string with substitions
        let mut out = String::new();
        let mut prev = 0;
        for (start, end, repl) in substs {
            out += &self.0[prev..start];
            out += repl.to_str().unwrap();
            prev = end + 1;
        }
        out += &self.0[prev..self.0.len()];
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{TemplateInfo, TemplatePath};

    #[test]
    fn repl_template() {
        let root = "hello";
        let user_id = "world";
        let expected = format!("{root}/hmm/{user_id}");
        let p = TemplatePath::new("<root>/hmm/<storeUserId>".to_owned());
        let got = p
            .apply_substs(&TemplateInfo {
                win_prefix: "".into(),
                win_user: "".to_owned(),
                base_dir: None,
                root: Some(PathBuf::from(root)),
                store_user_id: Some(user_id.to_owned()),
                home_dir: None,
                xdg_config: None,
                xdg_data: None,
                install_dir: None,
            })
            .unwrap();
        assert_eq!(expected, got);
    }

    #[test]
    fn repl_base_using_root_and_game() {
        let root = "hello";
        let install_dir = "world";
        let expected = format!("{root}/{install_dir}/hmm");
        let p = TemplatePath::new("<base>/hmm".to_owned());
        let got = p
            .apply_substs(&TemplateInfo {
                win_prefix: "".into(),
                win_user: "".to_owned(),
                base_dir: None,
                home_dir: None,
                xdg_config: None,
                xdg_data: None,
                root: Some(root.to_owned().into()),
                store_user_id: None,
                install_dir: Some(install_dir.to_owned().into()),
            })
            .unwrap();
        assert_eq!(expected, got);
    }
}
