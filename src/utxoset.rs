use std::collections::HashMap;

use crate::{Block, Blockchain, TXOutputs};
use anyhow::Result;
use bincode::{
    config::standard,
    serde::{decode_from_slice, encode_to_vec},
};

pub struct UTXOSet {
    pub bc: Blockchain,
}

impl UTXOSet {
    pub fn new(bc: Blockchain) -> Self {
        Self { bc }
    }

    pub fn reindex(&self) -> Result<()> {
        std::fs::remove_dir_all("db/utxos").ok();
        let db = sled::open("db/utxos")?;
        log::info!("Reindexing UTXO set");

        for (tx_id, outs) in self.bc.find_utxo() {
            let data = encode_to_vec(outs, standard())?;
            db.insert(tx_id.as_bytes(), data)?;
        }

        db.flush()?;
        log::info!("UTXO reindex completed at");

        Ok(())
    }

    pub fn find_spendable_outputs(
        &self,
        pub_key_hash: &[u8],
        amount: i32,
    ) -> Result<(i32, HashMap<String, Vec<i32>>)> {
        let mut unspent_outputs: HashMap<String, Vec<i32>> = HashMap::new();
        let mut accumulated = 0;
        let db = sled::open("db/utxos")?;

        for ele in db.iter() {
            let (k, v) = ele?;
            let tx_id = String::from_utf8(k.to_vec())?;
            let outs: TXOutputs = decode_from_slice(&v, standard()).map(|(w, _)| w)?;

            for (out_idx, out) in outs.outputs.iter().enumerate() {
                if out.is_locked_with_key(pub_key_hash) && accumulated < amount {
                    accumulated += out.value;
                    unspent_outputs
                        .entry(tx_id.to_owned())
                        .or_default()
                        .push(out_idx as i32);
                }

                if accumulated >= amount {
                    return Ok((accumulated, unspent_outputs));
                }
            }
        }

        Ok((accumulated, unspent_outputs))
    }

    pub fn find_utxo(&self, pub_key_hash: &[u8]) -> Result<TXOutputs> {
        let mut res = TXOutputs::default();
        let db = sled::open("db/utxos")?;

        for ele in db.iter() {
            let (_, v) = ele?;
            let outs: TXOutputs = decode_from_slice(&v, standard()).map(|(w, _)| w)?;
            for out in outs.outputs {
                if out.is_locked_with_key(pub_key_hash) {
                    res.outputs.push(out);
                }
            }
        }
        Ok(res)
    }

    pub fn update(&self, block: Block) -> Result<()> {
        let db = sled::open("db/utxos")?;

        for tx in block.transactions {
            if !tx.is_coinbase() {
                for vin in tx.v_in {
                    let outs: TXOutputs =
                        decode_from_slice(&db.get(&vin.tx_id)?.unwrap(), standard())
                            .map(|(w, _)| w)?;

                    let mut updated_outs = TXOutputs::default();
                    for (out_idx, out) in outs.outputs.iter().enumerate() {
                        if out_idx != vin.v_out as usize {
                            updated_outs.outputs.push(out.clone());
                        }
                    }

                    if updated_outs.outputs.is_empty() {
                        db.remove(&vin.tx_id)?;
                    } else {
                        db.insert(
                            vin.tx_id.as_bytes(),
                            encode_to_vec(updated_outs, standard())?,
                        )?;
                    }
                }
            }

            let mut new_outputs = TXOutputs::default();

            for out in tx.v_out {
                new_outputs.outputs.push(out);
            }
            db.insert(tx.id.as_bytes(), encode_to_vec(new_outputs, standard())?)?;
        }

        db.flush()?;
        Ok(())
    }
}
