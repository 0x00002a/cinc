use cinc::args::Args;
use clap::Parser;

fn main() {

    let args = Args::parse();

    match &args.op {
        cinc::args::Operation::Daemon { config } => {


        },
    }

}
