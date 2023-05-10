use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use chrono::Utc;
use libp2p::{
    identify::Info,
    identity::Keypair,
    kad::{store::MemoryStore, Kademlia, KademliaConfig, KademliaEvent},
    multiaddr::Protocol,
    swarm::{NetworkBehaviour, SwarmBuilder},
    Multiaddr, PeerId, Swarm,
};

use crate::save_state::{Account, Friend};

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
                        Success::Ping { rtt } => Some(Result::Ok(Success::Ping { rtt: *rtt })),
                    },
                    Err(err) => match err {
                        Failure::Timeout => Some(Result::Err(Failure::Timeout)),
                        Failure::Unsupported => Some(Result::Err(Failure::Unsupported)),
                        Failure::Other { .. } => Some(Result::Err(Failure::Unsupported)),
                    },
                },
                None => None,
            },
        }
    }
}

pub struct ActiveAccount {
    pub index: usize,
    pub name: String,
    pub peer_id: PeerId,
    pub keypair: Keypair,
    pub swarm: Swarm<TheManBehaviour>,
    pub friends: Vec<Friend>,
    pub expires: Instant,
}

pub struct TheManState {
    pub accounts: Vec<Account>,
    pub account: Option<ActiveAccount>,
    pub peers: HashMap<PeerId, PeerStatus>,
    pub bootnodes: Vec<Multiaddr>,
}

impl TheManState {
    pub fn set_account(&mut self, account_index: usize) {
        let Some(account) = self.accounts.get(account_index) else{return};

        let keypair = Keypair::from_protobuf_encoding(&account.private).unwrap();
        let peer_id = PeerId::from(keypair.public());

        let kademlia = {
            let mut cfg = KademliaConfig::default();
            cfg.set_query_timeout(Duration::from_secs(5 * 60));
            cfg.disjoint_query_paths(true);
            let store = MemoryStore::with_config(
                peer_id,
                libp2p::kad::store::MemoryStoreConfig {
                    max_records: 1024,
                    max_value_bytes: 65 * 1024,
                    max_providers_per_key: 16384,
                    max_provided_keys: 1024,
                },
            );
            let mut behaviour = Kademlia::with_config(peer_id, store, cfg);

            self.bootnodes.append(&mut vec![
        		"/dnsaddr/bootstrap.libp2p.io/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN".parse().unwrap(),
        		"/dnsaddr/bootstrap.libp2p.io/p2p/QmQCU2EcMqAqQPR2i9bChDtGNJchTbq5TbXJJ16u19uLTa".parse().unwrap(),
        		"/dnsaddr/bootstrap.libp2p.io/p2p/QmbLHAnMoJPWSCR5Zhtx6BHJX9KiKNN6tpvbUcqanj75Nb".parse().unwrap(),
        		"/dnsaddr/bootstrap.libp2p.io/p2p/QmcZf59bWwK5XFi76CZX8cbJ4BhTzzA3gU1ZjYZcYW3dwt".parse().unwrap(),
        		"/ip4/104.131.131.82/tcp/4001/p2p/QmaCpDMGvV2BGHeYERUEnRQAwe3N8SzbUtfsmvsqQLuvuJ".parse().unwrap(),
        		"/ip4/104.131.131.82/udp/4001/quic/p2p/QmaCpDMGvV2BGHeYERUEnRQAwe3N8SzbUtfsmvsqQLuvuJ".parse().unwrap()]);

            for node in self.bootnodes.iter() {
                let Some(protocol) = node.iter().last() else {continue};
                let Protocol::P2p(id) = protocol else {continue};
                let Ok(peer_id) = PeerId::from_multihash(id)else{continue};
                log::debug!("Adding BOOTNODE to kademlia: {node}/p2p/{protocol}");
                behaviour.add_address(&peer_id, node.clone());
            }
            behaviour
        };

        let identify = {
            let config = libp2p::identify::Config::new("theman/1.0.0".into(), keypair.public());
            libp2p::identify::Behaviour::new(config)
        };

        let mdns = {
            libp2p::mdns::tokio::Behaviour::new(
                libp2p::mdns::Config {
                    ttl: Duration::from_secs(60),
                    query_interval: Duration::from_secs(1),
                    enable_ipv6: false,
                },
                peer_id,
            )
            .unwrap()
        };

        let gossipsub = {
            let config = libp2p::gossipsub::ConfigBuilder::default()
                .flood_publish(true)
                .build()
                .unwrap();
            libp2p::gossipsub::Behaviour::new(
                libp2p::gossipsub::MessageAuthenticity::Signed(keypair.clone()),
                config,
            )
            .unwrap()
        };

        let autonat = {
            let config = libp2p::autonat::Config::default();
            libp2p::autonat::Behaviour::new(peer_id, config)
        };

        let relay = {
            let config = libp2p::relay::Config::default();
            libp2p::relay::Behaviour::new(peer_id, config)
        };

        let ping = { libp2p::ping::Behaviour::new(libp2p::ping::Config::new()) };

        let transport = libp2p::tokio_development_transport(keypair.clone()).unwrap();
        let swarm = SwarmBuilder::with_tokio_executor(
            transport,
            crate::state::TheManBehaviour {
                kademlia,
                identify,
                mdns,
                gossipsub,
                autonat,
                relay,
                ping,
            },
            peer_id,
        )
        .build();

        let instant = Instant::now()
            + account
                .expires
                .signed_duration_since(Utc::now())
                .to_std()
                .unwrap_or(Duration::ZERO);

        let account = ActiveAccount {
            name: account.name.clone(),
            peer_id,
            keypair,
            swarm,
            expires: instant,
            friends: vec![],
            index: account_index,
        };

        self.account = Some(account)
    }
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
