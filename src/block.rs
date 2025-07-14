use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use bincode::serde::encode_to_vec;
use log::info;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const TARGET_BITS: usize = 2;

#[derive(Serialize, Deserialize, Debug)]
pub struct Block {
    timestamp: u128,
    pub data: String,
    pub prev_block_hash: [u8; 32],
    pub hash: [u8; 32],
    nonce: u32,
}

impl Block {
    pub fn new_genesis_block() -> Self {
        Self::new("Genesis Block".to_owned(), [0u8; 32]).unwrap()
    }

    pub fn new(data: String, prev_block_hash: [u8; 32]) -> Result<Self> {
        let mut data = Self {
            timestamp: SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
            prev_block_hash,
            data,
            hash: [0u8; 32],
            nonce: 0,
        };
        data.run_proof_of_work()?;
        Ok(data)
    }

    fn prepare_hash_data(&self) -> Result<Vec<u8>> {
        let data_to_hash = (
            &self.prev_block_hash,
            &self.data,
            self.timestamp,
            TARGET_BITS,
            self.nonce,
        );
        let data = encode_to_vec(data_to_hash, bincode::config::standard())?;
        Ok(data)
    }

    fn validate(&self) -> Result<bool> {
        let hash = self.hash()?;
        let target = [0u8; TARGET_BITS];
        Ok(hash[0..TARGET_BITS] == target[..])
    }

    fn hash(&self) -> Result<[u8; 32]> {
        let data = self.prepare_hash_data()?;
        // Bitcoin uses double SHA-256: SHA256(SHA256(data))
        let mut hasher = Sha256::new();
        hasher.update(&data);
        let first_hash = hasher.finalize();
        let mut hasher = Sha256::new();
        hasher.update(&first_hash);
        Ok(hasher.finalize().into())
    }

    fn run_proof_of_work(&mut self) -> Result<()> {
        info!("Mining the block containing \"{}\"\n", self.data);
        loop {
            if self.validate()? {
                self.hash = self.hash()?;
                break;
            }
            self.nonce += 1;
        }
        Ok(())
    }
}
