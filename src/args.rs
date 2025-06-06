use std::path::{Path, PathBuf};

use clap::{
    ArgAction, Args, Parser, Subcommand, ValueEnum,
    builder::{PossibleValue, Styles, styling::AnsiColor},
};

use crate::config::{BackendType, SteamId};

#[derive(Parser, Debug)]
#[clap(styles = style())]
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
    ///
    /// Note this does NOT affect things like the manifest cache it is only for
    /// the files related to syncing
    #[arg(long, short = 'n', required = false, default_value_t = false)]
    pub dry_run: bool,

    /// Specify a config file to use
    #[arg(long = "config")]
    pub config_path: Option<PathBuf>,
    #[command(subcommand)]
    pub op: Option<Operation>,
}

#[derive(Subcommand, Clone, Debug)]
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
    /// Command to debug the version incompat screen, hidden from the user
    #[command(hide = true)]
    DebugVersionIncompat {
        /// Whether to display a read or write error dialog
        #[arg(default_value_t = true, long, action = ArgAction::Set)]
        read: bool,
    },

    /// Show a password input and echo it, for debugging
    #[command(hide = true)]
    DebugPskInput,
    /// Configure backends
    ///
    /// all others are just mirrors that
    /// are uploaded to. The one used for downloading can be specifically selected with --backend
    #[command(name = "backends", subcommand)]
    BackendsConfig(BackendsArgs),
}

#[derive(Subcommand, Clone, Debug)]
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

#[derive(Args, Clone, Debug)]
pub struct LaunchArgs {
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

    /// Do not download any files, only upload your local changes (on game close)
    ///
    /// Be aware this is A DESTRUCTIVE ACTION if you have made progress on another computer and not
    /// successfully run the game at least once on this one you will LOSE YOUR PROGRESS FROM THE
    /// OTHER COMPUTER
    #[arg(long = "upload-only", default_value_t = false)]
    pub no_download: bool,

    /// Specify the steam app id used to find the game in the manifest directly
    ///
    /// This is useful in case the actual app id on steam differs from the app id steam tells cinc,
    /// e.g. when you are launching a non-steam game through steam, or when cinc cannot find
    /// the game manifest (it will use this id to find the game's manifest)
    #[arg(long = "steam-app-id")]
    pub manifest_app_id_override: Option<SteamId>,

    #[arg(help = "Command to run the game, e.g. for steam pass as %command%")]
    pub command: Vec<String>,
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, ValueEnum)]
/// Force specific platform support, usually unnecessary as autodetect should find it
pub enum PlatformOpt {
    /// Force steam mode
    Steam,
    /// Force umu mode
    Umu,
    #[default]
    /// Attempt to autodetect launcher platform
    Auto,
}
const UMU_EXE_NAME: &str = "umu-run";
const WINE_EXE_NAME: &str = "wine";

impl LaunchArgs {
    /// Resolve the platform to one which is not auto
    pub fn resolve_platform(&self) -> Option<PlatformOpt> {
        match self.platform {
            PlatformOpt::Auto => {
                if self.command.iter().any(|s| s.starts_with("AppId=")) {
                    Some(PlatformOpt::Steam)
                } else if let Some(UMU_EXE_NAME | WINE_EXE_NAME) = self
                    .command
                    .first()
                    .and_then(|c| Path::new(c).file_name().and_then(|p| p.to_str()))
                {
                    Some(PlatformOpt::Umu)
                } else {
                    None
                }
            }
            v => Some(v),
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

fn style() -> Styles {
    Styles::styled()
        .header(AnsiColor::BrightGreen.on_default())
        .usage(AnsiColor::BrightMagenta.on_default())
        .literal(AnsiColor::BrightBlue.on_default())
        .placeholder(AnsiColor::BrightCyan.on_default())
}
