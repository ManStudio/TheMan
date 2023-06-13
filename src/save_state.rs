use std::collections::HashMap;

use chrono::{DateTime, Utc};
use libp2p::{Multiaddr, PeerId};

use crate::state::TheManState;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Friend {
    pub peer_id: PeerId,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ChannelType {
    #[default]
    Message,
    Voice,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Account {
    pub name: String,
    pub private: Vec<u8>,
    #[serde(default)]
    pub friends: Vec<Friend>,
    #[serde(default = "default_expires")]
    pub expires: DateTime<Utc>,
    #[serde(default)]
    pub channels: Vec<(String, ChannelType)>,
    #[serde(default)]
    pub renew: bool,
}

fn default_expires() -> DateTime<Utc> {
    Utc::now()
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct TheManSaveState {
    pub accounts: Vec<Account>,
    pub bootnodes: Vec<Multiaddr>,
}

impl From<TheManSaveState> for TheManState {
    fn from(value: TheManSaveState) -> Self {
        Self {
            peers: HashMap::new(),
            accounts: value.accounts,
            account: None,
            bootnodes: value.bootnodes,
        }
    }
}
