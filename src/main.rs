use anyhow::Result;
use rs_blockchain::Blockchain;

fn main() -> Result<()> {
    let mut bc = Blockchain::new();
    bc.add_block("Send 1 BTC to Xmchx".to_owned())?;
    bc.add_block("Send 2 more BTC to Xmchx".to_owned())?;

    for b in &bc.blocks {
        println!("Prev. hash: {}", b.prev_block_hash);
        println!("Data: {}", b.data);
        println!("Hash: {}", b.hash);
        println!()
    }

    Ok(())
}
