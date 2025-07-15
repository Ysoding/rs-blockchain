use anyhow::{Result, anyhow};
use bincode::{config::standard, serde::encode_to_vec};
use log::error;
use serde::{Deserialize, Serialize};

use sha2::{Digest, Sha256};

use crate::Blockchain;

const SUBSIDY: i32 = 10;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Transaction {
    pub id: String,
    pub v_in: Vec<TXInput>,
    pub v_out: Vec<TXOutput>,
}

impl Transaction {
    pub fn new_utxo(from: &str, to: &str, amount: i32, bc: &Blockchain) -> Result<Transaction> {
        let mut inputs = vec![];
        let mut outputs = vec![];

        let (acc, valid_outputs) = bc.find_spendable_outputs(from, amount);

        if acc < amount {
            error!("Not enough funds");
            return Err(anyhow!("Not enough funds: {}", acc));
        }

        for (tx_id, outs) in valid_outputs {
            for out in outs {
                let input = TXInput {
                    tx_id: tx_id.to_owned(),
                    v_out: out,
                    script_sig: from.to_owned(),
                };
                inputs.push(input);
            }
        }

        outputs.push(TXOutput {
            value: amount,
            script_pub_key: to.to_owned(),
        });
        if acc > amount {
            outputs.push(TXOutput {
                value: acc - amount,
                script_pub_key: from.to_owned(),
            });
        }
        let mut tx = Transaction {
            id: "".to_owned(),
            v_in: inputs,
            v_out: outputs,
        };
        tx.id = hex::encode(tx.hash()?);
        Ok(tx)
    }

    pub fn new_coinbase(to: String, data: String) -> Result<Transaction> {
        let data = if data == "" {
            format!("Reward to '{}'", to).to_owned()
        } else {
            data
        };

        let tx_in = TXInput {
            tx_id: "".to_owned(),
            v_out: -1,
            script_sig: data,
        };

        let tx_out = TXOutput {
            value: SUBSIDY,
            script_pub_key: to,
        };
        let mut tx = Transaction {
            id: "".to_owned(),
            v_in: vec![tx_in],
            v_out: vec![tx_out],
        };
        tx.id = hex::encode(tx.hash()?);
        Ok(tx)
    }

    pub fn hash(&self) -> Result<[u8; 32]> {
        let mut data = self.clone();
        data.id = "".to_owned();
        let data = encode_to_vec(data, standard())?;
        let mut hasher = Sha256::new();
        hasher.update(data);
        Ok(hasher.finalize().into())
    }

    pub fn is_coinbase(&self) -> bool {
        self.v_in.len() == 1 && self.v_in[0].tx_id.is_empty() && self.v_in[0].v_out == -1
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TXOutput {
    pub value: i32,
    pub script_pub_key: String,
}

impl TXOutput {
    pub fn can_be_unlocked_with(&self, unlocking_data: &str) -> bool {
        self.script_pub_key == unlocking_data
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TXInput {
    pub tx_id: String,
    pub v_out: i32,
    pub script_sig: String,
}

impl TXInput {
    pub fn can_unlock_output_with(&self, unlocking_data: &str) -> bool {
        self.script_sig == unlocking_data
    }
}
