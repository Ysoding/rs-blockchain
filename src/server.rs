use std::{
    collections::{HashMap, HashSet},
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex, RwLock},
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

use crate::{Block, Transaction, UTXOSet};

#[derive(Debug, PartialEq, Clone, Copy)]
enum Command {
    Addr,
    Block,
    Inv,
    GetBlocks,
    GetData,
    Tx,
    Version,
}

impl Command {
    fn as_str(&self) -> &'static str {
        match self {
            Command::Addr => "addr",
            Command::Block => "block",
            Command::Inv => "inv",
            Command::GetBlocks => "getblocks",
            Command::GetData => "getdata",
            Command::Tx => "tx",
            Command::Version => "version",
        }
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let cmd = String::from_utf8(bytes.to_vec())
            .map_err(|_| anyhow!(String::from_utf8_lossy(bytes).to_string()))?;
        match cmd.trim_end_matches('\0').as_ref() {
            "addr" => Ok(Command::Addr),
            "block" => Ok(Command::Block),
            "inv" => Ok(Command::Inv),
            "getblocks" => Ok(Command::GetBlocks),
            "getdata" => Ok(Command::GetData),
            "tx" => Ok(Command::Tx),
            "version" => Ok(Command::Version),
            _ => Err(anyhow!(cmd)),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Message {
    Addr {
        nodes: Vec<String>,
    },
    Block {
        addr_from: String,
        block: Block,
    },
    Inv {
        addr_from: String,
        kind: String,
        items: Vec<String>,
    },
    GetBlocks {
        addr_from: String,
    },
    GetData {
        addr_from: String,
        kind: String,
        id: String,
    },
    Tx {
        addr_from: String,
        transaction: Transaction,
    },
    Version {
        addr_from: String,
        version: u32,
        best_height: u32,
    },
}

impl Message {
    fn command(&self) -> Command {
        match self {
            Message::Addr { .. } => Command::Addr,
            Message::Block { .. } => Command::Block,
            Message::Inv { .. } => Command::Inv,
            Message::GetBlocks { .. } => Command::GetBlocks,
            Message::GetData { .. } => Command::GetData,
            Message::Tx { .. } => Command::Tx,
            Message::Version { .. } => Command::Version,
        }
    }

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
    blocks_in_transit: Vec<String>,
    mempool: HashMap<String, Transaction>,
    connections: HashMap<String, Arc<Mutex<TcpStream>>>,
}

#[derive(Clone)]
pub struct Config {
    known_node: String,
    version: u32,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            known_node: "localhost:3000".to_string(),
            version: 1,
        }
    }
}

const CENTERAL_NODE: &str = "localhost:3000";

pub struct ServerBuilder {
    port: Option<String>,
    miner_address: Option<String>,
    utxo: Option<UTXOSet>,
    config: Config,
}

impl ServerBuilder {
    pub fn new() -> Self {
        ServerBuilder {
            port: None,
            miner_address: None,
            utxo: None,
            config: Config::default(),
        }
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
        known_nodes.insert(self.config.known_node.clone());
        Ok(Server {
            node_address: format!("localhost:{}", port).to_string(),
            mining_address: miner_address,
            inner: Arc::new(RwLock::new(ServerInner {
                known_nodes,
                utxo,
                blocks_in_transit: Vec::new(),
                mempool: HashMap::new(),
                connections: HashMap::new(),
            })),
            config: self.config,
        })
    }
}

impl Server {
    pub fn builder() -> ServerBuilder {
        ServerBuilder::new()
    }

    pub fn start(&self) -> Result<()> {
        let server = self.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(1000));
            match server.get_best_height().unwrap() {
                Some(v) => server
                    .send_message(
                        &server.config.known_node,
                        Message::Version {
                            addr_from: server.node_address.clone(),
                            version: server.config.version,
                            best_height: v,
                        },
                    )
                    .unwrap_or(()),
                None => server.request_blocks().unwrap(),
            };
        });

        let listener = TcpListener::bind(&self.node_address)?;
        info!("Server listening on {}", &self.node_address);

        for stream in listener.incoming() {
            let stream = stream?;
            info!("New connection: {}", stream.peer_addr()?);
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
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf)?;
        Ok(())
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

    fn get_best_height(&self) -> Result<Option<u32>> {
        self.with_read_lock(|inner| inner.utxo.bc.get_best_height())
    }

    fn request_blocks(&self) -> Result<()> {
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
        let cmd = message.command();
        log::info!("Sending message: cmd={:?}, to={}", cmd, addr);
        let data = encode_to_vec(&(cmd_to_bytes(cmd), message), standard())?;
        self.send_data(addr, &data)
    }

    fn send_data(&self, addr: &str, data: &[u8]) -> Result<()> {
        if addr == self.node_address {
            return Ok(());
        }

        let stream = self.with_write_lock(|inner| match inner.connections.get(addr) {
            Some(stream) => Ok::<_, anyhow::Error>(stream.clone()),
            None => {
                let stream = TcpStream::connect(addr).map_err(|e| {
                    inner.known_nodes.remove(addr);
                    anyhow!(e)
                })?;
                let stream = Arc::new(Mutex::new(stream));
                inner.connections.insert(addr.to_owned(), stream.clone());
                Ok(stream)
            }
        })?;
        stream.lock().unwrap().write_all(data)?;
        log::info!("Data sent successfully to {}", addr);
        Ok(())
    }

    fn get_known_nodes(&self) -> HashSet<String> {
        self.with_read_lock(|inner| inner.known_nodes.clone())
    }
}

fn cmd_to_bytes(cmd: Command) -> [u8; 12] {
    let mut data = [0; 12];
    let cmd_str = cmd.as_str();
    for (i, d) in cmd_str.as_bytes().iter().enumerate() {
        data[i] = *d;
    }
    data
}

fn bytes_to_cmd(bytes: &[u8]) -> Result<Message> {
    let cmd_bytes = &bytes[..12];
    let data = &bytes[12..];
    let cmd = Command::from_bytes(cmd_bytes)?;
    log::info!("cmd: {:?}", cmd);

    let (message, _) = decode_from_slice(data, standard())?;
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

        let data = encode_to_vec(&(cmd_to_bytes(vmsg.command()), &vmsg), standard()).unwrap();
        match bytes_to_cmd(&data).unwrap() {
            Message::Version {
                addr_from,
                version,
                best_height,
            } => {
                assert_eq!(addr_from, vmsg.addr_from());
                assert_eq!(version, server.config.version);
                assert_eq!(best_height, server.get_best_height().unwrap().unwrap());
            }
            _ => panic!("Expected Version message"),
        }
    }
}
