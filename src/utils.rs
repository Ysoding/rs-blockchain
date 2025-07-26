use base58::FromBase58;
use ripemd::Ripemd160;

use sha2::{Digest, Sha256};

pub fn hash_pub_key(pub_key: &[u8]) -> Vec<u8> {
    let mut sha256 = Sha256::new();
    sha256.update(pub_key);
    let public_sha256 = sha256.finalize();

    let mut ripemd160 = Ripemd160::new();
    ripemd160.update(public_sha256);
    ripemd160.finalize().to_vec()
}

pub fn get_pub_key_hash(address: &str) -> Vec<u8> {
    let pub_key_hash = address.from_base58().unwrap();
    let pub_key_hash = &pub_key_hash[1..pub_key_hash.len() - 4];
    pub_key_hash.to_vec()
}
