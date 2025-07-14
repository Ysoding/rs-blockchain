use std::process::exit;

use anyhow::Result;
use clap::Parser;
use env_logger::Env;
use rs_blockchain::{Blockchain, Cli, Commands};

fn main() -> Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let cli = Cli::parse();
    let mut bc = Blockchain::new()?;

    match cli.command {
        Commands::AddBlock { data } => {
            if data.is_empty() {
                exit(1);
            }
            bc.add_block(data)?;
        }
        Commands::PrintChain {} => {
            bc.iter().for_each(|b| println!("{:?}", b));
        }
    }
    Ok(())
}
