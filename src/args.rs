use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum, builder::PossibleValue};

use crate::{
    config::{BackendType, SteamId},
    manifest::Store,
};

#[derive(Parser)]
pub struct CliArgs {
    /// Update the manifest
    ///
    /// Note this is a rather slow operation with the default manifest
    /// as it is several megabytes of yaml
    #[arg(long, default_value = "false")]
    pub update: bool,
    /// Debug flag, not displayed to the user
    #[arg(
        hide = true,
        long = "no-panic-hook",
        required = false,
        default_value_t = false
    )]
    pub no_panic_hook: bool,

    /// Don't write to the filesystem or any backends
    ///
    /// This obviousely has no effect for some commands that are purely
    /// query, e.g. listing backends
    #[arg(long, short = 'n', required = false, default_value_t = false)]
    pub dry_run: bool,

    /// Specify a config file to use
    #[arg(long = "config")]
    pub config_path: Option<PathBuf>,
    #[command(subcommand)]
    pub op: Operation,
}

#[derive(Subcommand, Clone)]
pub enum Operation {
    /// For launching a game
    ///
    /// This will download the files from the specified (or default) backend before launching the game,
    /// and upload them after. It may be used with steam as `cinc launch -- %command%`
    Launch(LaunchArgs),
    #[command(hide = true)]
    DebugSyncDialog {
        #[arg(default_value = "debug remote", long)]
        remote_name: String,
        #[arg(default_value = "debug writer", long)]
        last_writer: String,
    },
    /// Configure backends
    ///
    /// all others are just mirrors that
    /// are uploaded to. The one used for downloading can be specifically selected with --backend
    #[command(name = "backends", subcommand)]
    BackendsConfig(BackendsArgs),
}

#[derive(Subcommand, Clone)]
pub enum BackendsArgs {
    /// Add a backend to the config
    Add {
        /// Name of the backend
        #[arg(long = "name")]
        name: String,

        /// Type of the backend
        #[arg(long = "ty", alias = "type")]
        ty: BackendType,

        /// Root for the backend relative to the base root of the backend
        ///
        /// This will depend on the backend
        ///
        /// - for WebDav it is relative to the url given,
        ///
        /// - for the filesystem it is relative to /
        #[arg(long = "root", default_value = "/")]
        root: PathBuf,

        /// Set this backend as the default after adding it
        #[arg(long = "set-default", default_value = "false")]
        set_default: bool,

        /// Url for the webdav backend, required when type is webdev
        #[arg(long = "webdav-url")]
        webdav_url: Option<String>,

        /// Username for the webdav backend, required when type is webdev
        #[arg(long = "webdav-username")]
        webdav_username: Option<String>,
    },
    Remove {
        /// Name of the backend to remove
        #[arg()]
        name: String,
    },
    /// List all configured backends
    List,
    /// Set a backend as the default
    SetDefault {
        /// Name of the backend
        #[arg()]
        name: String,
    },
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
    /// Don't upload after closing, this is a debug flag and is hidden from the user
    #[arg(long = "debug-no-upload", hide = true, default_value = "false")]
    pub no_upload: bool,

    /// Specify the steam app id used to find the game in the manifest directly
    ///
    /// This is useful in case the actual app id on steam differs from the app id steam tells cinc,
    /// e.g. when you are launching a non-steam game through steam
    #[arg(long = "steam-app-id")]
    pub app_id: Option<SteamId>,

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
        &[BackendType::Filesystem, BackendType::WebDav]
    }

    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        match self {
            BackendType::Filesystem => Some(
                PossibleValue::new("filesystem")
                    .alias("fs")
                    .help("filesystem backend which copies the files to local folder"),
            ),
            BackendType::WebDav => Some(PossibleValue::new("webdav").help("webdav backend")),
        }
    }
}
