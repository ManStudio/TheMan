use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use libp2p::{kad::kbucket::NodeStatus, Multiaddr, PeerId};

use crate::{save_state::TheManSaveState, state::PeerStatus};

#[derive(Debug)]
pub enum Message {
    SwarmStatus(libp2p::swarm::NetworkInfo),
    Save,
    SaveResponse(TheManSaveState),
    Bootstrap,
    GetBootNodes,
    BootNodes(Vec<(PeerId, NodeStatus, Vec<Multiaddr>)>),
    GetPeers,
    Peers(Vec<(PeerId, PeerStatus)>),
    ShutDown,
}

unsafe impl Send for Message {}
unsafe impl Sync for Message {}
