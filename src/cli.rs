use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    #[command(name = "addblock")]
    AddBlock {
        /// Block data
        data: String,
    },
    #[command(name = "printchain")]
    PrintChain {},
}

// let mut bc = Blockchain::new()?;
// // sleep(Duration::from_millis(10));
// // bc.add_block("Send 1 BTC to Xmchx".to_owned())?;
// // sleep(Duration::from_millis(30));
// // bc.add_block("Send 2 more BTC to Xmchx".to_owned())?;

// for block in bc.iter() {
//     println!("{:?}", block);
// }
