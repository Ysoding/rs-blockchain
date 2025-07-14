use std::{thread::sleep, time::Duration};

use anyhow::Result;
use rs_blockchain::Blockchain;

fn main() -> Result<()> {
    let mut bc = Blockchain::new();
    sleep(Duration::from_millis(10));
    bc.add_block("Send 1 BTC to Xmchx".to_owned())?;
    sleep(Duration::from_millis(30));
    bc.add_block("Send 2 more BTC to Xmchx".to_owned())?;

    println!("{:?}", bc.blocks);

    Ok(())
}
