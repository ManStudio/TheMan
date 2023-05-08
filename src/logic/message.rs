use libp2p::{kad::kbucket::NodeStatus, Multiaddr, PeerId};

use crate::save_state::TheManSaveState;

#[derive(Debug)]
pub enum Message {
    KademliaStatus(libp2p::swarm::NetworkInfo),
    Save,
    SaveResponse(TheManSaveState),
    Bootstrap,
    GetPeers,
    Peers(Vec<(PeerId, NodeStatus, Vec<Multiaddr>)>),
    ShutDown,
}
