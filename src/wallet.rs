use std::collections::HashMap;

use anyhow::Result;
use base58::ToBase58;
use bincode::{
    config::standard,
    serde::{decode_from_slice, encode_to_vec},
};
use log::info;
use p256::{
    ecdsa::{SigningKey, VerifyingKey},
    elliptic_curve::rand_core::OsRng,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::hash_pub_key;

const VERSION: u8 = 0x00;
const ADDRESS_CHECKSUM_LEN: usize = 4;

pub struct Wallets {
    pub wallets: HashMap<String, Wallet>,
}

impl Wallets {
    pub fn new() -> Result<Wallets> {
        let mut waleets = Self {
            wallets: HashMap::default(),
        };
        waleets.load()?;
        Ok(waleets)
    }

    fn load(&mut self) -> Result<()> {
        let db = sled::open("db/wallets")?;
        for ele in db.into_iter() {
            let ele = ele?;
            let addr = String::from_utf8(ele.0.to_vec())?;
            let wallet: Wallet = decode_from_slice(&ele.1, standard()).map(|(w, _)| w)?;
            self.wallets.insert(addr, wallet);
        }
        Ok(())
    }

    pub fn get_addresses(&self) -> Vec<String> {
        let mut res = vec![];
        for addr in self.wallets.keys() {
            res.push(addr.clone());
        }
        res
    }

    pub fn get_wallet(&self, addr: &str) -> Option<&Wallet> {
        self.wallets.get(addr)
    }

    pub fn create_wallet(&mut self) -> String {
        let wallet = Wallet::new();
        let addr = wallet.get_address();
        self.wallets.insert(addr.to_owned(), wallet);
        info!("create wallet: {}", addr);
        addr
    }

    pub fn save(&self) -> Result<()> {
        let db = sled::open("db/wallets")?;
        for (addr, wallet) in &self.wallets {
            let data = encode_to_vec(wallet, standard())?;
            db.insert(addr, data)?;
        }
        db.flush()?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Wallet {
    pub private_key: Vec<u8>,
    pub public_key: Vec<u8>,
}

impl Default for Wallet {
    fn default() -> Self {
        Self::new()
    }
}

impl Wallet {
    pub fn new() -> Self {
        let (private_key, public_key) = new_key_pair();
        Self {
            private_key,
            public_key,
        }
    }

    pub fn get_address(&self) -> String {
        let pub_key_hash = hash_pub_key(&self.public_key);

        let mut versioned_payload = vec![VERSION];
        versioned_payload.extend_from_slice(&pub_key_hash);

        let checksum = checksum(&versioned_payload);

        let mut full_payload = versioned_payload;
        full_payload.extend_from_slice(&checksum);

        full_payload.to_base58()
    }
}

fn new_key_pair() -> (Vec<u8>, Vec<u8>) {
    let private = SigningKey::random(&mut OsRng);
    let private_key_bytes = private.to_bytes().to_vec();
    let public = VerifyingKey::from(&private);
    let pub_key_bytes = public.to_encoded_point(false).as_bytes().to_vec();
    (private_key_bytes, pub_key_bytes)
}

fn checksum(payload: &[u8]) -> Vec<u8> {
    let mut first_sha = Sha256::new();
    first_sha.update(payload);
    let first_hash = first_sha.finalize();

    let mut second_sha = Sha256::new();
    second_sha.update(first_hash);
    let second_hash = second_sha.finalize();

    second_hash[..ADDRESS_CHECKSUM_LEN].to_vec()
}
