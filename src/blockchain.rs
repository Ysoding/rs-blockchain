use anyhow::Result;
use bincode::{
    config::standard,
    serde::{decode_from_slice, encode_to_vec},
};
use log::info;

use crate::Block;

pub struct Blockchain {
    pub tip: [u8; 32],
    db: sled::Db,
}

impl Blockchain {
    pub fn new() -> Result<Self> {
        let db = sled::open("db")?;
        match db.get("l")? {
            Some(hash) => {
                info!("Found blockchain");
                let mut last_hash = [0u8; 32];
                last_hash.copy_from_slice(&hash);
                Ok(Blockchain { tip: last_hash, db })
            }
            None => {
                info!("No existing blockchain found. Creating a new one...");
                let block = Block::new_genesis_block();
                let hash = block.hash;
                db.insert(hash, encode_to_vec(block, standard())?)?;
                db.insert("l", &hash)?;
                db.flush()?;
                let bc = Blockchain { tip: hash, db };
                Ok(bc)
            }
        }
    }

    pub fn add_block(&mut self, data: String) -> Result<()> {
        info!("add new block");
        let hash = self.db.get("l")?.unwrap();
        let mut last_hash = [0u8; 32];
        last_hash.copy_from_slice(&hash);

        let new_block = Block::new(data, last_hash)?;
        let hash = new_block.hash;
        self.db
            .insert(hash, encode_to_vec(new_block, standard())?)?;
        self.db.insert("l", &hash)?;
        self.db.flush()?;

        self.tip = hash;
        Ok(())
    }

    pub fn iter(&self) -> BlockchainIterator {
        BlockchainIterator {
            current_hash: self.tip,
            bc: &self,
        }
    }
}

pub struct BlockchainIterator<'a> {
    bc: &'a Blockchain,
    current_hash: [u8; 32],
}

impl<'a> Iterator for BlockchainIterator<'a> {
    type Item = Block;

    fn next(&mut self) -> Option<Self::Item> {
        let encoded_block = self.bc.db.get(&self.current_hash).ok()??;

        let block: Block = decode_from_slice(&encoded_block, standard())
            .ok()
            .map(|(b, _)| b)?;

        self.current_hash = block.prev_block_hash;

        Some(block)
    }
}
