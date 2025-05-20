use cinc::{args::Args, config::Config};
use clap::Parser;

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match &args.op {
        cinc::args::Operation::Daemon { config } => {
            let cfg: Config = toml::from_str(&std::fs::read_to_string(config)?)?;
            let backend = cfg.backend.to_backend();

            let games = cfg
                .games
                .iter()
                .map(|g| g.resolve())
                .collect::<anyhow::Result<Vec<_>>>()?;
            for game in &games {
                println!("{game:#?}");
            }
        }
    }
    Ok(())
}
