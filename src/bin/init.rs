use clap::Parser;
use sftpx::compression;

#[derive(Parser, Debug)]
#[command(version, about)]
pub struct InitArgs {
    #[arg(short, long)]
    pub place_type: String,
}

fn main() {
    let args = InitArgs::parse();

    println!(
        "Hello User! Initialized file transfer system type as {}!",
        args.place_type
    );

    compression::do_compression(&args.place_type);
}
