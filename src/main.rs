use anyhow::Result;
use clap::Parser;
use env_logger::Env;
use rs_blockchain::{Blockchain, Cli, Commands, Transaction};

fn main() -> Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let cli = Cli::parse();

    match cli.command {
        Commands::PrintChain {} => {
            let bc = Blockchain::new("")?;
            bc.iter().for_each(|b| println!("{:?}", b));
        }
        Commands::GetBalance { address } => {
            let bc = Blockchain::new(&address)?;

            let mut balance = 0;
            for out in bc.find_utxo(&address) {
                balance += out.value;
            }
            println!("Balance of '{}': {}\n", address, balance)
        }
        Commands::CreateBlockChain { address } => {
            Blockchain::create(&address)?;
        }
        Commands::Send { amount, from, to } => {
            let mut bc = Blockchain::new(&from)?;

            let tx = Transaction::new_utxo(&from, &to, amount, &bc)?;
            bc.mine_block(vec![tx])?;
            println!("Success!");
        }
    }
    Ok(())
}
