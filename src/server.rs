use std::{
    collections::{HashMap, HashSet},
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, RwLock},
    thread,
    time::Duration,
};

use anyhow::{Result, anyhow};
use bincode::{
    config::standard,
    serde::{decode_from_slice, encode_to_vec},
};
use log::{error, info};
use serde::{Deserialize, Serialize};

use crate::{Block, HashType, Transaction, UTXOSet};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Message {
    Addr {
        nodes: HashSet<String>,
    },
    Block {
        addr_from: String,
        block: Block,
    },
    Inv {
        addr_from: String,
        kind: String,
        items: Vec<HashType>,
    },
    GetBlocks {
        addr_from: String,
    },
    GetData {
        addr_from: String,
        kind: String,
        id: HashType,
    },
    Tx {
        addr_from: String,
        transaction: Transaction,
    },
    Version {
        addr_from: String,
        version: i32,
        best_height: i32,
    },
}

impl Message {
    #[allow(dead_code)]
    fn addr_from(&self) -> &str {
        match self {
            Message::Addr { .. } => "", // No addr_from for Addr message
            Message::Block { addr_from, .. } => addr_from,
            Message::Inv { addr_from, .. } => addr_from,
            Message::GetBlocks { addr_from, .. } => addr_from,
            Message::GetData { addr_from, .. } => addr_from,
            Message::Tx { addr_from, .. } => addr_from,
            Message::Version { addr_from, .. } => addr_from,
        }
    }
}

trait MessageHandler {
    fn handle(&self, server: &Server) -> Result<()>;
}

impl MessageHandler for Message {
    fn handle(&self, server: &Server) -> Result<()> {
        match self {
            Message::Addr { nodes } => {
                log::info!("Receive address msg: {:?}", nodes);
                for node in nodes {
                    server.add_node(node);
                }
                Ok(())
            }
            Message::Block { addr_from, block } => {
                log::info!("Receive block msg: {}, {:?}", addr_from, block,);
                server.add_block(block)?;
                let mut in_transit = server.get_in_transit();
                if !in_transit.is_empty() {
                    let block_hash = in_transit[0];
                    server.send_message(
                        addr_from,
                        Message::GetData {
                            addr_from: server.node_address.clone(),
                            kind: "block".to_string(),
                            id: block_hash,
                        },
                    )?;
                    in_transit.remove(0);
                    server.replace_in_transit(in_transit);
                } else {
                    server.utxo_reindex()?;
                }
                Ok(())
            }
            Message::Inv {
                addr_from,
                kind,
                items,
            } => {
                log::info!(
                    "Receive inv msg: addr_from={}, kind={}, items={:?}",
                    addr_from,
                    kind,
                    items
                );
                if kind == "block" {
                    let block_hash = items[0];
                    server.send_message(
                        addr_from,
                        Message::GetData {
                            addr_from: server.node_address.clone(),
                            kind: "block".to_string(),
                            id: block_hash,
                        },
                    )?;
                    let new_in_transit: Vec<HashType> = items
                        .iter()
                        .filter(|b| **b != block_hash)
                        .cloned()
                        .collect();
                    server.replace_in_transit(new_in_transit);
                } else if kind == "tx" {
                    let txid = items[0];
                    match server.get_mempool_tx(&txid) {
                        Some(tx) if tx.id.is_empty() => {
                            server.send_message(
                                addr_from,
                                Message::GetData {
                                    addr_from: server.node_address.clone(),
                                    kind: "tx".to_string(),
                                    id: txid,
                                },
                            )?;
                        }
                        None => server.send_message(
                            addr_from,
                            Message::GetData {
                                addr_from: server.node_address.clone(),
                                kind: "tx".to_string(),
                                id: txid,
                            },
                        )?,
                        _ => {}
                    }
                }
                Ok(())
            }
            Message::GetBlocks { addr_from } => {
                log::info!("Receive get blocks msg: addr_from={}", addr_from);
                let block_hashs = server.get_block_hashs();
                server.send_message(
                    addr_from,
                    Message::Inv {
                        addr_from: server.node_address.clone(),
                        kind: "block".to_string(),
                        items: block_hashs,
                    },
                )?;
                Ok(())
            }
            Message::GetData {
                addr_from,
                kind,
                id,
            } => {
                log::info!(
                    "Receive get data msg: addr_from={}, kind={}, id={}",
                    addr_from,
                    kind,
                    hex::encode(id)
                );
                if kind == "block" {
                    let block = server.get_block(id)?;
                    server.send_message(
                        addr_from,
                        Message::Block {
                            addr_from: server.node_address.clone(),
                            block,
                        },
                    )?;
                } else if kind == "tx" {
                    if let Some(tx) = server.get_mempool_tx(id) {
                        server.send_message(
                            addr_from,
                            Message::Tx {
                                addr_from: server.node_address.clone(),
                                transaction: tx,
                            },
                        )?;
                    }
                }
                Ok(())
            }
            Message::Tx {
                addr_from,
                transaction,
            } => {
                log::info!(
                    "Receive tx msg: addr_from={}, txid={}",
                    addr_from,
                    transaction.id
                );
                server.insert_mempool(transaction.clone());
                if server.node_address == server.config.centeral_node {
                    for node in server.get_known_nodes() {
                        if node != server.node_address && node != *addr_from {
                            server.send_message(
                                &node,
                                Message::Inv {
                                    addr_from: server.node_address.clone(),
                                    kind: "tx".to_string(),
                                    items: vec![transaction.hash_val],
                                },
                            )?;
                        }
                    }
                } else if !server.mining_address.is_empty() {
                    let mut mempool = server.get_mempool();
                    log::info!("Current mempool: {:#?}", &mempool);
                    if !mempool.is_empty() {
                        loop {
                            let mut txs = Vec::new();
                            for tx in mempool.values() {
                                if server.verify_tx(tx)? {
                                    txs.push(tx.clone());
                                }
                            }
                            if txs.is_empty() {
                                return Ok(());
                            }

                            let cbtx =
                                Transaction::new_coinbase(&server.mining_address, String::new())?;
                            txs.push(cbtx);

                            for tx in &txs {
                                mempool.remove(&tx.hash_val);
                            }

                            let new_block = server.mine_block(txs)?;
                            server.utxo_reindex()?;

                            for node in server.get_known_nodes() {
                                if node != server.node_address {
                                    server.send_message(
                                        &node,
                                        Message::Inv {
                                            addr_from: server.node_address.clone(),
                                            kind: "block".to_string(),
                                            items: vec![new_block.hash],
                                        },
                                    )?;
                                }
                            }

                            if mempool.is_empty() {
                                break;
                            }
                        }
                        server.clear_mempool();
                    }
                }
                Ok(())
            }
            Message::Version {
                addr_from,
                version,
                best_height,
            } => {
                log::info!(
                    "Receive version msg: addr_from={}, version={}, best_height={}",
                    addr_from,
                    version,
                    best_height
                );
                let my_best_height = server.get_best_height()?;
                if my_best_height < *best_height {
                    server.send_message(
                        addr_from,
                        Message::GetBlocks {
                            addr_from: server.node_address.clone(),
                        },
                    )?;
                } else if my_best_height > *best_height {
                    server.send_message(
                        addr_from,
                        Message::Version {
                            addr_from: server.node_address.clone(),
                            version: server.config.version,
                            best_height: my_best_height,
                        },
                    )?;
                }
                server.send_message(
                    addr_from,
                    Message::Addr {
                        nodes: server.get_known_nodes(),
                    },
                )?;
                if !server.node_is_known(addr_from) {
                    server.add_node(addr_from);
                }
                Ok(())
            }
        }
    }
}

#[derive(Clone)]
pub struct Server {
    node_address: String,
    mining_address: String,
    inner: Arc<RwLock<ServerInner>>,
    config: Config,
}

struct ServerInner {
    known_nodes: HashSet<String>,
    utxo: UTXOSet,
    blocks_in_transit: Vec<HashType>,
    mempool: HashMap<HashType, Transaction>,
}

#[derive(Clone)]
pub struct Config {
    centeral_node: String,
    version: i32,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            centeral_node: CENTERAL_NODE.to_owned(),
            version: 1,
        }
    }
}

const CENTERAL_NODE: &str = "localhost:3000";

#[derive(Default)]
pub struct ServerBuilder {
    port: Option<String>,
    miner_address: Option<String>,
    utxo: Option<UTXOSet>,
    config: Config,
}

impl ServerBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn port(mut self, port: &str) -> Self {
        self.port = Some(port.to_string());
        self
    }

    pub fn miner_address(mut self, address: &str) -> Self {
        self.miner_address = Some(address.to_string());
        self
    }

    pub fn utxo(mut self, utxo: UTXOSet) -> Self {
        self.utxo = Some(utxo);
        self
    }

    pub fn config(mut self, config: Config) -> Self {
        self.config = config;
        self
    }

    pub fn build(self) -> Result<Server> {
        let port = self.port.ok_or_else(|| anyhow!("Missing port"))?;
        let miner_address = self.miner_address.unwrap_or_default();
        let utxo = self.utxo.ok_or_else(|| anyhow!("Missing UTXO set"))?;
        let mut known_nodes = HashSet::new();
        known_nodes.insert(self.config.centeral_node.clone());
        Ok(Server {
            node_address: format!("localhost:{}", port).to_string(),
            mining_address: miner_address,
            inner: Arc::new(RwLock::new(ServerInner {
                known_nodes,
                utxo,
                blocks_in_transit: Vec::new(),
                mempool: HashMap::new(),
            })),
            config: self.config,
        })
    }
}

impl Server {
    pub fn builder() -> ServerBuilder {
        ServerBuilder::new()
    }

    pub fn send_transaction(tx: Transaction, utxo_set: UTXOSet) -> Result<()> {
        let server = Server::builder().port("6969").utxo(utxo_set).build()?;
        server.send_message(
            &server.config.centeral_node,
            Message::Tx {
                addr_from: server.node_address.clone(),
                transaction: tx,
            },
        )?;
        Ok(())
    }

    pub fn start(&self) -> Result<()> {
        let server = self.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(2000));
            match server.get_best_height()? {
                -1 => server.request_blocks(),
                v => server.send_message(
                    &server.config.centeral_node,
                    Message::Version {
                        addr_from: server.node_address.clone(),
                        version: server.config.version,
                        best_height: v,
                    },
                ),
            }
        });

        let listener = TcpListener::bind(&self.node_address)?;
        info!(
            "Server listening on {}, mining_address: {}",
            &self.node_address, &self.mining_address
        );

        for stream in listener.incoming() {
            let stream = stream?;
            let server = self.clone();
            thread::spawn(move || {
                if let Err(e) = server.handle_connection(stream) {
                    error!("Error handling connection: {}", e);
                }
            });
        }

        Ok(())
    }

    fn handle_connection(&self, mut stream: TcpStream) -> Result<()> {
        info!("handle new connection");

        let mut len_buf = [0; 4];
        stream.read_exact(&mut len_buf)?;
        let len = u32::from_be_bytes(len_buf) as usize;
        info!("Received message length: {}", len);

        let mut buf = vec![0; len];
        stream.read_exact(&mut buf)?;
        let msg = bytes_to_msg(&buf)?;
        info!("Deserialized message: {:?}", msg);

        msg.handle(self)
    }

    fn with_read_lock<T, F>(&self, f: F) -> T
    where
        F: FnOnce(&ServerInner) -> T,
    {
        let inner = self.inner.read().unwrap();
        f(&inner)
    }

    fn with_write_lock<T, F>(&self, f: F) -> T
    where
        F: FnOnce(&mut ServerInner) -> T,
    {
        let mut inner = self.inner.write().unwrap();
        f(&mut inner)
    }

    fn verify_tx(&self, tx: &Transaction) -> Result<bool> {
        self.with_read_lock(|inner| inner.utxo.bc.verify_transaction(tx))
    }

    fn utxo_reindex(&self) -> Result<()> {
        info!("utxo reindex");
        self.with_write_lock(|inner| inner.utxo.reindex())
    }

    fn node_is_known(&self, addr: &str) -> bool {
        self.with_read_lock(|inner| inner.known_nodes.contains(addr))
    }

    fn remove_node(&self, addr: &str) {
        self.with_write_lock(|inner| {
            inner.known_nodes.remove(addr);
        });
    }

    fn add_node(&self, addr: &str) {
        self.with_write_lock(|inner| {
            inner.known_nodes.insert(addr.to_string());
        });
    }

    fn get_best_height(&self) -> Result<i32> {
        self.with_read_lock(|inner| inner.utxo.bc.get_best_height())
    }

    fn get_block_hashs(&self) -> Vec<HashType> {
        self.with_read_lock(|inner| inner.utxo.bc.get_block_hashs())
    }

    fn request_blocks(&self) -> Result<()> {
        info!("request_blocks");
        for node in self.get_known_nodes() {
            self.send_message(
                &node,
                Message::GetBlocks {
                    addr_from: self.node_address.clone(),
                },
            )?;
        }
        Ok(())
    }

    fn send_message(&self, addr: &str, message: Message) -> Result<()> {
        log::info!("Sending message:={:?}  to={}", message, addr);
        let data = encode_to_vec(message, standard())?;
        self.send_data(addr, &data)
    }

    fn send_data(&self, addr: &str, data: &[u8]) -> Result<()> {
        if addr == self.node_address {
            info!("skip: send self data");
            return Ok(());
        }

        let mut stream = match TcpStream::connect(addr) {
            Ok(s) => s,
            Err(_) => {
                self.remove_node(addr);
                return Ok(());
            }
        };

        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        let len = data.len() as u32;
        stream.write_all(&len.to_be_bytes())?;
        stream.write_all(data)?;
        log::info!("Data sent successfully to {}", addr);
        Ok(())
    }

    fn get_known_nodes(&self) -> HashSet<String> {
        self.with_read_lock(|inner| inner.known_nodes.clone())
    }

    fn replace_in_transit(&self, hashs: Vec<HashType>) {
        self.with_write_lock(|inner| inner.blocks_in_transit = hashs);
    }

    fn get_in_transit(&self) -> Vec<HashType> {
        self.with_read_lock(|inner| inner.blocks_in_transit.clone())
    }

    fn get_mempool_tx(&self, addr: &HashType) -> Option<Transaction> {
        self.with_read_lock(|inner| inner.mempool.get(addr).cloned())
    }

    fn get_mempool(&self) -> HashMap<HashType, Transaction> {
        self.with_read_lock(|inner| inner.mempool.clone())
    }

    fn insert_mempool(&self, tx: Transaction) {
        self.with_write_lock(|inner| inner.mempool.insert(tx.hash_val, tx));
    }

    fn clear_mempool(&self) {
        self.with_write_lock(|inner| inner.mempool.clear());
    }

    fn get_block(&self, block_hash: &HashType) -> Result<Block> {
        self.with_read_lock(|inner| inner.utxo.bc.get_block(block_hash))
    }

    fn add_block(&self, block: &Block) -> Result<()> {
        self.with_write_lock(|inner| inner.utxo.bc.add_block(block))
    }

    fn mine_block(&self, txs: Vec<Transaction>) -> Result<Block> {
        self.with_write_lock(|inner| inner.utxo.bc.mine_block(txs))
    }
}

fn bytes_to_msg(bytes: &[u8]) -> Result<Message> {
    let (message, _) = decode_from_slice(bytes, standard())?;
    Ok(message)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::blockchain::*;
    use crate::wallet::*;

    #[test]
    fn test_cmd() {
        let mut ws = Wallets::new().unwrap();
        let wa1 = ws.create_wallet();
        let bc = Blockchain::create(&wa1).unwrap();
        let utxo_set = UTXOSet::new(bc);
        let server = Server::builder()
            .port("7878")
            .miner_address("localhost:3001")
            .utxo(utxo_set)
            .build()
            .unwrap();

        let vmsg = Message::Version {
            addr_from: "localhost:7879".to_string(),
            version: 1,
            best_height: 0,
        };

        let data = encode_to_vec(&vmsg, standard()).unwrap();
        match bytes_to_msg(&data).unwrap() {
            Message::Version {
                addr_from,
                version,
                best_height,
            } => {
                assert_eq!(addr_from, vmsg.addr_from());
                assert_eq!(version, server.config.version);
                assert_eq!(best_height, server.get_best_height().unwrap());
            }
            _ => panic!("Expected Version message"),
        }
    }
}
