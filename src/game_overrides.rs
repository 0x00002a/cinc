use std::collections::HashMap;

use include_directory::include_directory;
use serde::{Deserialize, Serialize};

use crate::manifest::TemplatePath;

pub type GameOverrides = Vec<GameOverride>;

const BUILTIN_OVERRIDES_DIR: include_directory::Dir<'_> = include_directory!("overrides");

pub fn builtin_overrides() -> GameOverrides {
    BUILTIN_OVERRIDES_DIR
        .files()
        .filter(|f| f.path().ends_with(".toml"))
        .map(|p| {
            let data = p.contents_utf8().unwrap();
            GameOverride::from_toml(data)
                .unwrap_or_else(|_| panic!("failed to parse built-in override file {:?}", p.path()))
        })
        .collect()
}

#[derive(PartialEq, Eq, Serialize, Deserialize, Debug)]
pub struct GameOverride {
    predicates: Predicates,
    #[serde(default)]
    vars: HashMap<String, TemplatePath>,
    #[serde(default)]
    manifest: Option<Manifest>,
    about: About,
}

impl GameOverride {
    pub fn matches(&self, launch_command: &str) -> bool {
        glob_match::glob_match(&self.predicates.executable, launch_command)
    }

    pub fn title_override(&self) -> Option<&str> {
        Some(&self.manifest.as_ref()?.title)
    }
    pub fn from_toml(s: &str) -> Result<Self, toml::de::Error> {
        toml::de::from_str(s)
    }
}

#[derive(PartialEq, Eq, Serialize, Deserialize, Debug, Default)]
pub struct Predicates {
    executable: String,
}

#[derive(PartialEq, Eq, Serialize, Deserialize, Debug, Default)]
pub struct Manifest {
    title: String,
}

#[derive(PartialEq, Eq, Serialize, Deserialize, Debug, Default)]
pub struct About {
    name: String,
    desc: String,
}
