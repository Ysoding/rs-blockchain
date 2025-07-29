use anyhow::Result;
use clap::Parser;
use env_logger::Env;
use rs_blockchain::{
    Blockchain, Cli, Commands, Server, ServerBuilder, Transaction, UTXOSet, Wallets,
    get_pub_key_hash,
};

fn main() -> Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let cli = Cli::parse();

    match cli.command {
        Commands::PrintChain => {
            let bc = Blockchain::new()?;
            bc.iter().for_each(|b| println!("{:?}", b));
        }
        Commands::GetBalance { address } => {
            let bc = Blockchain::new()?;
            let mut balance = 0;
            let pub_key_hash = get_pub_key_hash(&address);

            let utxo_set = UTXOSet::new(bc);

            for out in utxo_set.find_utxo(&pub_key_hash)?.outputs {
                balance += out.value;
            }
            println!("Balance of '{}': {}\n", address, balance)
        }
        Commands::CreateBlockChain { address } => {
            let bc = Blockchain::create(&address)?;
            let utxo_set = UTXOSet::new(bc);
            utxo_set.reindex()?;
        }
        Commands::Send {
            amount,
            from,
            to,
            mine,
        } => {
            let bc = Blockchain::new()?;
            let mut utxo_set = UTXOSet::new(bc);
            let tx = Transaction::new_utxo(&from, &to, amount, &utxo_set)?;
            let cb_tx = Transaction::new_coinbase(&from, "".to_owned())?;
            if mine {
                let txs = vec![cb_tx, tx];
                let block = utxo_set.bc.mine_block(txs)?;
                utxo_set.update(block)?;
            } else {
                Server::send_transaction(&tx, utxo_set)?;
            }
            println!("Success!");
        }
        Commands::CreateWallet => {
            let mut ws = Wallets::new()?;
            let addr = ws.create_wallet();
            ws.save()?;
            println!("Your new address: {}", addr);
        }
        Commands::ListAddress => {
            let ws = Wallets::new()?;
            println!("addresses: ");
            for addr in ws.get_addresses() {
                println!("{}", addr);
            }
        }
        Commands::StartNode {
            port,
            miner_address,
        } => {
            println!("Start node");
            let bc = Blockchain::new()?;
            let utxo_set = UTXOSet::new(bc);
            let mut server_builder = ServerBuilder::new().port(&port).utxo(utxo_set);

            if let Some(address) = miner_address {
                println!("Starting miner node");
                server_builder = server_builder.miner_address(&address);
            } else {
                println!("Starting node");
            }

            let server = server_builder.build()?;
            server.start()?;
        }
    }
    Ok(())
}
