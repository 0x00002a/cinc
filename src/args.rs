use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum, builder::PossibleValue};

use crate::{config::BackendType, manifest::Store};

#[derive(Parser)]
pub struct CliArgs {
    #[command(subcommand)]
    pub op: Operation,
}

#[derive(Subcommand, Clone)]
pub enum Operation {
    Init {},
    Launch(LaunchArgs),
    #[command(hide = true)]
    DebugSyncDialog,
}

#[derive(Args, Clone)]
pub struct LaunchArgs {
    #[arg(long = "save-dir", help = "Path of save files directory to sync")]
    pub save_dir: Vec<PathBuf>,

    #[arg(
        long = "platform",
        short = 'p',
        help = "Platform game is running on",
        required = false,
        default_value = "auto"
    )]
    pub platform: PlatformOpt,

    #[arg(help = "Command to run the game, e.g. for steam pass as %command%")]
    pub command: Vec<String>,
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PlatformOpt {
    Steam,
    #[default]
    Auto,
}
impl ValueEnum for PlatformOpt {
    fn value_variants<'a>() -> &'a [Self] {
        &[Self::Auto, Self::Steam]
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        match self {
            PlatformOpt::Steam => Some(
                PossibleValue::new("steam")
                    .help("force steam support, usually unnecessary as autodetect should find it"),
            ),
            PlatformOpt::Auto => {
                Some(PossibleValue::new("auto").help("attempt to autodetect launcher platform"))
            }
        }
    }
}

impl LaunchArgs {
    /*pub fn uses_backends(&self) -> Vec<BackendInfo> {
        let mut used = Vec::new();
        if let Some(fs) = &self.fs_backend_args {
            used.push(BackendInfo::Filesystem {
                root: fs.root.to_owned(),
            })
        }
        used
    }*/
    pub fn resolve_platform(&self) -> Option<Store> {
        match self.platform {
            PlatformOpt::Steam => Some(Store::Steam),
            PlatformOpt::Auto => {
                if self.command.iter().any(|s| s.starts_with("AppId=")) {
                    Some(Store::Steam)
                } else {
                    None
                }
            }
        }
    }
}

#[derive(Args, Clone)]
#[group(multiple = true)]
pub struct FsBackendArgs {
    #[arg(long = "fs-backend")]
    pub fs_backend: bool,
    #[arg(long = "fs-root")]
    pub root: PathBuf,
}

impl ValueEnum for BackendType {
    fn value_variants<'a>() -> &'a [Self] {
        &[BackendType::Filesystem]
    }

    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        match self {
            BackendType::Filesystem => {
                Some(PossibleValue::new("filesystem").alias("fs").help(
                    "filesystem backend which syncs the files to local folder, for debugging",
                ))
            }
        }
    }
}
