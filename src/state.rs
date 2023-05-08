use std::collections::HashMap;

use libp2p::{
    core::PeerId,
    identify::Info,
    identity::Keypair,
    kad::{store::MemoryStore, Kademlia, KademliaEvent},
    swarm::{behaviour, NetworkBehaviour},
    Swarm,
};
use std::sync::{Arc, RwLock};

#[derive(Default, Debug)]
pub struct PeerStatus {
    pub info: Option<Info>,
    pub ping: Option<libp2p::ping::Result>,
}

impl Clone for PeerStatus {
    fn clone(&self) -> Self {
        use libp2p::ping::*;
        Self {
            info: self.info.clone(),
            ping: match &self.ping {
                Some(res) => match res {
                    Ok(ok) => match ok {
                        Success::Pong => Some(Result::Ok(Success::Pong)),
                        Success::Ping { rtt } => {
                            Some(Result::Ok(Success::Ping { rtt: rtt.clone() }))
                        }
                    },
                    Err(err) => match err {
                        Failure::Timeout => Some(Result::Err(Failure::Timeout)),
                        Failure::Unsupported => Some(Result::Err(Failure::Unsupported)),
                        Failure::Other { error } => Some(Result::Err(Failure::Unsupported)),
                    },
                },
                None => None,
            },
        }
    }
}

pub struct TheManState {
    pub peer_id: PeerId,
    pub keypair: Keypair,
    pub swarm: Swarm<TheManBehaviour>,
    pub peers: HashMap<PeerId, PeerStatus>,
}

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "BehaviourEvent")]
pub struct TheManBehaviour {
    pub kademlia: Kademlia<MemoryStore>,
    pub identify: libp2p::identify::Behaviour,
    pub mdns: libp2p::mdns::tokio::Behaviour,
    pub gossipsub: libp2p::gossipsub::Behaviour,
    pub autonat: libp2p::autonat::Behaviour,
    pub relay: libp2p::relay::Behaviour,
    pub ping: libp2p::ping::Behaviour,
}

#[derive(Debug)]
pub enum BehaviourEvent {
    Kademlia(KademliaEvent),
    Identify(libp2p::identify::Event),
    MDNS(libp2p::mdns::Event),
    GossIpSub(libp2p::gossipsub::Event),
    AutoNat(libp2p::autonat::Event),
    Relay(libp2p::relay::Event),
    Ping(libp2p::ping::Event),
}

impl From<KademliaEvent> for BehaviourEvent {
    fn from(value: KademliaEvent) -> Self {
        Self::Kademlia(value)
    }
}

impl From<libp2p::identify::Event> for BehaviourEvent {
    fn from(value: libp2p::identify::Event) -> Self {
        Self::Identify(value)
    }
}

impl From<libp2p::mdns::Event> for BehaviourEvent {
    fn from(value: libp2p::mdns::Event) -> Self {
        Self::MDNS(value)
    }
}

impl From<libp2p::gossipsub::Event> for BehaviourEvent {
    fn from(value: libp2p::gossipsub::Event) -> Self {
        Self::GossIpSub(value)
    }
}

impl From<libp2p::autonat::Event> for BehaviourEvent {
    fn from(value: libp2p::autonat::Event) -> Self {
        Self::AutoNat(value)
    }
}

impl From<libp2p::relay::Event> for BehaviourEvent {
    fn from(value: libp2p::relay::Event) -> Self {
        Self::Relay(value)
    }
}

impl From<libp2p::ping::Event> for BehaviourEvent {
    fn from(value: libp2p::ping::Event) -> Self {
        Self::Ping(value)
    }
}
