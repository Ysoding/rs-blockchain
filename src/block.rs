use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use bincode::serde::encode_to_vec;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Serialize, Deserialize)]
pub struct Block {
    timestamp: u128,
    pub data: String,
    pub prev_block_hash: String,
    pub hash: String,
}

impl Block {
    pub fn new_genesis_block() -> Self {
        Self::new("Genesis Block".to_owned(), "".to_owned()).unwrap()
    }

    pub fn new(data: String, prev_block_hash: String) -> Result<Self> {
        let mut data = Self {
            timestamp: SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
            prev_block_hash,
            data,
            hash: String::new(),
        };
        data.hash()?;
        Ok(data)
    }

    fn hash(&mut self) -> Result<()> {
        let data_to_hash = (self.timestamp, &self.data, &self.prev_block_hash);

        let data = encode_to_vec(data_to_hash, bincode::config::standard())?;

        let mut hasher = Sha256::new();
        hasher.update(data);
        let hash_result = hasher.finalize();

        self.hash = hex::encode(hash_result);
        Ok(())
    }
}
