use std::collections::HashMap;

use anyhow::Result;
use bincode::{
    config::standard,
    serde::{decode_from_slice, encode_to_vec},
};
use log::info;

use crate::{Block, TXOutput, Transaction};

const GENESIS_COINBASE_DATA: &str =
    "The Times 03/Jan/2009 Chancellor on brink of second bailout for banks";

pub struct Blockchain {
    pub tip: [u8; 32],
    db: sled::Db,
}

impl Blockchain {
    pub fn new(addr: &str) -> Result<Self> {
        let db = sled::open("db")?;
        match db.get("l")? {
            Some(hash) => {
                info!("Found blockchain");
                let mut last_hash = [0u8; 32];
                last_hash.copy_from_slice(&hash);
                Ok(Blockchain { tip: last_hash, db })
            }
            None => {
                info!("No existing blockchain found.");
                Self::create(addr)
            }
        }
    }

    pub fn create(addr: &str) -> Result<Self> {
        info!("Create new blockchain");

        let cbtx = Transaction::new_coinbase(addr.to_owned(), GENESIS_COINBASE_DATA.to_owned())?;
        let genesis = Block::new_genesis_block(cbtx);

        std::fs::remove_dir_all("db")?;

        let hash = genesis.hash;
        let db = sled::open("db")?;
        db.insert(hash, encode_to_vec(genesis, standard())?)?;
        db.insert("l", &hash)?;
        db.flush()?;

        let bc = Blockchain { tip: hash, db };
        Ok(bc)
    }

    pub fn find_spendable_outputs(
        &self,
        addr: &str,
        amount: i32,
    ) -> (i32, HashMap<String, Vec<i32>>) {
        let mut accumulated = 0;
        let mut unspent_outputs: HashMap<String, Vec<i32>> = HashMap::new();

        let unsped_txs = self.find_unspend_transactions(addr);

        for tx in unsped_txs {
            for (out_idx, out) in tx.v_out.iter().enumerate() {
                if out.can_be_unlocked_with(addr) && accumulated < amount {
                    accumulated += out.value;

                    unspent_outputs
                        .entry(tx.id.clone())
                        .or_insert_with(Vec::new)
                        .push(out_idx as i32);

                    if accumulated >= amount {
                        return (accumulated, unspent_outputs);
                    }
                }
            }
        }

        (accumulated, unspent_outputs)
    }

    fn find_unspend_transactions(&self, addr: &str) -> Vec<Transaction> {
        let mut unspend_txs = vec![];
        let mut spend_txos: HashMap<String, Vec<i32>> = HashMap::new();

        for block in self.iter() {
            for tx in block.transactions {
                for (out_idx, out) in tx.v_out.iter().enumerate() {
                    if let Some(ids) = spend_txos.get(&tx.id) {
                        if ids.contains(&(out_idx as i32)) {
                            continue;
                        }
                    }

                    if out.can_be_unlocked_with(addr) {
                        unspend_txs.push(tx.to_owned());
                    }
                }

                if !tx.is_coinbase() {
                    for in_ in tx.v_in {
                        if in_.can_unlock_output_with(addr) {
                            spend_txos
                                .entry(in_.tx_id)
                                .or_insert_with(Vec::new)
                                .push(in_.v_out);
                        }
                    }
                }
            }
        }

        unspend_txs
    }

    pub fn find_utxo(&self, addr: &str) -> Vec<TXOutput> {
        let mut res = vec![];

        let unspend_transactions = self.find_unspend_transactions(addr);

        for tx in unspend_transactions {
            for out in tx.v_out {
                if out.can_be_unlocked_with(addr) {
                    res.push(out);
                }
            }
        }

        res
    }

    fn add_block(&mut self, block: &Block) -> Result<()> {
        info!("add new block");

        let hash = block.hash;
        self.db.insert(hash, encode_to_vec(block, standard())?)?;
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

    pub fn mine_block(&mut self, transactions: Vec<Transaction>) -> Result<Block> {
        info!("mines a new block");

        let last_hash = self.get_last_hash()?;
        let new_block = Block::new(transactions, last_hash)?;

        self.add_block(&new_block)?;
        Ok(new_block)
    }

    fn get_last_hash(&self) -> Result<[u8; 32]> {
        let hash = self.db.get("l")?.unwrap();
        let mut last_hash = [0u8; 32];
        last_hash.copy_from_slice(&hash);
        Ok(last_hash)
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
