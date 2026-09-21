use include_directory::include_directory;
use serde::{Deserialize, Serialize};

use crate::manifest::TemplateInfo;

pub type GameOverrides = Vec<GameOverride>;

const BUILTIN_OVERRIDES_DIR: include_directory::Dir<'_> = include_directory!("overrides");

pub fn builtin_overrides() -> GameOverrides {
    BUILTIN_OVERRIDES_DIR
        .files()
        .map(|p| {
            let data = p.contents_utf8().unwrap();
            GameOverride::from_toml(data)
                .unwrap_or_else(|_| panic!("failed to parse built-in override file {:?}", p.path()))
        })
        .collect()
}

#[derive(PartialEq, Eq, Serialize, Deserialize, Debug, Clone)]
pub struct GameOverride {
    predicates: Predicates,
    vars: TemplateInfo,
    #[serde(default)]
    manifest: Option<Manifest>,
    about: About,
}

impl GameOverride {
    pub fn matches(&self, launch_command: &str) -> bool {
        println!(
            "match {} against {}",
            self.predicates.executable, launch_command
        );
        fast_glob::glob_match(&self.predicates.executable, launch_command)
    }
    fn validate(&self) -> anyhow::Result<()> {
        fast_glob::validate(&self.predicates.executable)?;
        Ok(())
    }

    pub fn title_override(&self) -> Option<&str> {
        Some(&self.manifest.as_ref()?.title)
    }
    pub fn from_toml(s: &str) -> anyhow::Result<Self> {
        let me: Self = toml::de::from_str(s)?;
        me.validate()?;
        Ok(me)
    }
    pub fn apply_vars(&self, vs: &mut TemplateInfo) -> anyhow::Result<()> {
        if let Some(p) = &self.vars.base_dir {
            vs.base_dir = Some(shellexpand::full(p.to_str().unwrap())?.into_owned().into())
        }
        if let Some(p) = &self.vars.install_dir {
            vs.install_dir = Some(shellexpand::full(p.to_str().unwrap())?.into_owned().into())
        }
        if let Some(p) = &self.vars.root {
            vs.root = Some(shellexpand::full(p.to_str().unwrap())?.into_owned().into());
        }
        Ok(())
    }
}

#[derive(PartialEq, Eq, Serialize, Deserialize, Debug, Default, Clone)]
pub struct Predicates {
    executable: String,
}

#[derive(PartialEq, Eq, Serialize, Deserialize, Debug, Default, Clone)]
pub struct Manifest {
    title: String,
}

#[derive(PartialEq, Eq, Serialize, Deserialize, Debug, Default, Clone)]
pub struct About {
    name: String,
    desc: String,
}

#[cfg(test)]
mod tests {
    use crate::game_overrides::GameOverride;

    #[test]
    fn starsector_game_override_from_command() {
        let info = include_str!("../overrides/starsector-local.toml");
        let ov = GameOverride::from_toml(info).unwrap();
        assert!(ov.matches("./starsector.sh"));
        assert!(ov.matches("starsector.sh"));
    }

    #[test]
    fn t2() {
        assert!(fast_glob::glob_match("*/test", "bingus/test"));
        assert!(fast_glob::glob_match("*/test", "./test"));
        assert!(fast_glob::glob_match("*test", "test"));
        assert!(fast_glob::glob_match("{*,**,*/}test", "./test"));
        assert!(fast_glob::glob_match("{*,**}test", "test"));
    }
}
