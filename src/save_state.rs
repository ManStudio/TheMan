use std::{collections::HashMap, time::Duration};

use libp2p::{
    identity::Keypair,
    kad::{store::MemoryStore, Kademlia, KademliaConfig},
    multiaddr::Protocol,
    swarm::SwarmBuilder,
    Multiaddr, PeerId,
};

use crate::state::TheManState;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Account {
    pub name: String,
    pub private: Vec<u8>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct TheManSaveState {
    pub accounts: Vec<Account>,
    pub bootnodes: Vec<Multiaddr>,
}

impl From<TheManSaveState> for TheManState {
    fn from(mut value: TheManSaveState) -> Self {
        Self {
            peers: HashMap::new(),
            accounts: value.accounts,
            account: None,
            bootnodes: value.bootnodes,
        }
    }
}
