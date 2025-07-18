use std::collections::HashMap;

use anyhow::{Context, Ok, Result, anyhow};
use base58::FromBase58;
use bincode::{config::standard, serde::encode_to_vec};
use log::{debug, error};
use p256::ecdsa::{Signature, SigningKey, VerifyingKey, signature::SignerMut, signature::Verifier};
use serde::{Deserialize, Serialize};

use sha2::{Digest, Sha256};

use crate::{Blockchain, Wallets, hash_pub_key};

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

        let wallets = Wallets::new()?;
        let wallet = wallets.get_wallet(from).unwrap();
        let pub_key_hash = hash_pub_key(&wallet.public_key);

        let (acc, valid_outputs) = bc.find_spendable_outputs(&pub_key_hash, amount);

        if acc < amount {
            error!("Not enough funds");
            return Err(anyhow!("Not enough funds: {}", acc));
        }

        for (tx_id, outs) in valid_outputs {
            for out in outs {
                let input = TXInput {
                    tx_id: tx_id.to_owned(),
                    v_out: out,
                    signature: vec![],
                    pub_key: wallet.public_key.clone(),
                };
                inputs.push(input);
            }
        }

        outputs.push(TXOutput::new(amount, to));
        if acc > amount {
            outputs.push(TXOutput::new(acc - amount, from));
        }
        let mut tx = Transaction {
            id: "".to_owned(),
            v_in: inputs,
            v_out: outputs,
        };
        tx.set_id()?;
        bc.sign_transaction(&mut tx, &wallet.private_key)?;

        Ok(tx)
    }

    pub fn new_coinbase(to: &str, data: String) -> Result<Transaction> {
        let data = if data == "" {
            format!("Reward to '{}'", to).to_owned()
        } else {
            data
        };

        let tx_in = TXInput {
            tx_id: "".to_owned(),
            v_out: -1,
            signature: vec![],
            pub_key: data.into(),
        };

        let tx_out = TXOutput::new(SUBSIDY, to);
        let mut tx = Transaction {
            id: "".to_owned(),
            v_in: vec![tx_in],
            v_out: vec![tx_out],
        };
        tx.set_id()?;
        Ok(tx)
    }

    pub fn set_id(&mut self) -> Result<()> {
        self.id = hex::encode(self.hash()?);
        Ok(())
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

    pub fn sign(
        &mut self,
        private_key: &[u8],
        prev_txs: HashMap<String, Transaction>,
    ) -> Result<()> {
        if self.is_coinbase() {
            return Ok(());
        }

        let mut tx_copy = self.trimmed_copy();

        for in_id in 0..tx_copy.v_in.len() {
            let prev_tx = prev_txs.get(&tx_copy.v_in[in_id].tx_id).unwrap();
            tx_copy.v_in[in_id].signature.clear();
            tx_copy.v_in[in_id].pub_key = prev_tx.v_out[tx_copy.v_in[in_id].v_out as usize]
                .pub_key_hash
                .clone();
            tx_copy.set_id()?;
            tx_copy.v_in[in_id].pub_key = vec![];

            let mut signing_key = SigningKey::from_bytes(private_key.into())?;
            let signature: p256::ecdsa::Signature = signing_key.sign(tx_copy.id.as_bytes());

            let r = signature.r().to_bytes();
            let s = signature.s().to_bytes();

            let mut signature_bytes = Vec::new();
            signature_bytes.extend_from_slice(&r);
            signature_bytes.extend_from_slice(&s);

            self.v_in[in_id].signature = signature_bytes;
        }
        Ok(())
    }

    pub fn verify(&self, prev_txs: HashMap<String, Transaction>) -> Result<bool> {
        let mut tx_copy = self.trimmed_copy();

        for in_id in 0..tx_copy.v_in.len() {
            let prev_tx = prev_txs.get(&tx_copy.v_in[in_id].tx_id).unwrap();

            tx_copy.v_in[in_id].signature.clear();
            tx_copy.v_in[in_id].pub_key = prev_tx.v_out[tx_copy.v_in[in_id].v_out as usize]
                .pub_key_hash
                .clone();
            tx_copy.set_id()?;
            tx_copy.v_in[in_id].pub_key = vec![];

            // Extract signature (r, s)
            let signature_bytes = &self.v_in[in_id].signature;
            if signature_bytes.len() != 64 {
                debug!(
                    "Signature must be 64 bytes (32 for r, 32 for s) : {}",
                    signature_bytes.len()
                );
                return Ok(false);
            }
            let r_bytes: [u8; 32] = signature_bytes[0..32]
                .try_into()
                .context("Invalid r length")?;
            let s_bytes: [u8; 32] = signature_bytes[32..64]
                .try_into()
                .context("Invalid s length")?;
            let signature = Signature::from_scalars(r_bytes, s_bytes)
                .context("Failed to construct signature")?;

            // Handle public key
            let pub_key_bytes = &self.v_in[in_id].pub_key;
            let pub_key = VerifyingKey::from_sec1_bytes(&pub_key_bytes)
                .context("Invalid public key format")?;

            // Verify signature
            if pub_key.verify(tx_copy.id.as_bytes(), &signature).is_err() {
                debug!("Verify signature fail");
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn trimmed_copy(&self) -> Self {
        let mut inputs = vec![];
        let mut outputs = vec![];

        for ele in &self.v_in {
            inputs.push(TXInput {
                tx_id: ele.tx_id.clone(),
                v_out: ele.v_out,
                signature: vec![],
                pub_key: vec![],
            });
        }

        for ele in &self.v_out {
            outputs.push(TXOutput {
                value: ele.value,
                pub_key_hash: ele.pub_key_hash.clone(),
            });
        }

        let res = Transaction {
            id: self.id.clone(),
            v_in: inputs,
            v_out: outputs,
        };

        res
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TXOutput {
    pub value: i32,
    pub pub_key_hash: Vec<u8>,
}

impl TXOutput {
    pub fn new(value: i32, address: &str) -> Self {
        let mut v = Self {
            value,
            pub_key_hash: vec![],
        };
        v.lock(address);
        v
    }

    pub fn is_locked_with_key(&self, pub_key_hash: &[u8]) -> bool {
        self.pub_key_hash == pub_key_hash
    }

    pub fn lock(&mut self, address: &str) {
        let pub_key_hash = address.from_base58().unwrap();
        let pub_key_hash = &pub_key_hash[1..pub_key_hash.len() - 4];
        self.pub_key_hash = pub_key_hash.to_vec();
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TXInput {
    pub tx_id: String,
    pub v_out: i32,
    pub signature: Vec<u8>,
    pub pub_key: Vec<u8>,
}

impl TXInput {
    pub fn uses_key(&self, pub_key_hash: &[u8]) -> bool {
        let v = hash_pub_key(&self.pub_key);
        v == pub_key_hash
    }
}
