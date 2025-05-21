use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum, builder::PossibleValue};

use crate::config::{BackendInfo, BackendType};

#[derive(Parser)]
pub struct CliArgs {
    #[command(subcommand)]
    pub op: Operation,
}

#[derive(Subcommand, Clone)]
pub enum Operation {
    Daemon {
        #[arg(long = "config", help = "Config file to use")]
        config: PathBuf,
    },
    Launch(LaunchArgs),
}

#[derive(Args, Clone)]
pub struct LaunchArgs {
    #[arg(
        long = "save-dir",
        help = "Path of save files directory to sync",
        required = true
    )]
    pub save_dir: Vec<PathBuf>,

    #[command(flatten)]
    pub fs_backend_args: Option<FsBackendArgs>,

    #[arg(help = "Steam command, pass as %command%")]
    pub steam_command: Vec<String>,
}

impl LaunchArgs {
    pub fn uses_backends(&self) -> Vec<BackendInfo> {
        let mut used = Vec::new();
        if let Some(fs) = &self.fs_backend_args {
            used.push(BackendInfo::Filesystem {
                root: fs.root.to_owned(),
            })
        }
        used
    }
}

#[derive(Args, Clone)]
#[group(multiple = true, required = true)]
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
