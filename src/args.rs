use std::path::PathBuf;

use clap::{ Parser, Subcommand};


#[derive(Parser)]
pub struct Args {
    #[command(subcommand)]
    pub op: Operation,

}

#[derive(Subcommand, Clone)]
pub enum Operation {
    Daemon {
        #[arg(long = "config", help = "Config file to use")]
        config: PathBuf,
    }

}
