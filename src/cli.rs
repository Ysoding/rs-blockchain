use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Create a blockchain and send genesis block reward to ADDRESS
    #[command(name = "createblockchain")]
    CreateBlockChain {
        #[arg(long)]
        address: String,
    },
    /// Get balance of ADDRESS
    #[command(name = "getbalance")]
    GetBalance {
        #[arg(long)]
        address: String,
    },
    /// Print all the blocks of the blockchain
    #[command(name = "printchain")]
    PrintChain,
    /// Send AMOUNT of coins from FROM address to TO
    Send {
        /// Amount to send
        #[arg(long)]
        amount: i32,
        /// Source wallet address
        #[arg(long)]
        from: String,
        /// Destination wallet address
        #[arg(long)]
        to: String,
    },
    /// Generates a new key-pair and saves it into the wallet file
    #[command(name = "createwallet")]
    CreateWallet,
}
