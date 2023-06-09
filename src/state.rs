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
    swarm::{derive_prelude::ListenerId, NetworkBehaviour, SwarmBuilder},
    Multiaddr, PeerId, Swarm, Transport,
};

use crate::save_state::{Account, Friend};

#[derive(Default, Debug, Clone)]
pub struct PeerStatus {
    pub info: Option<Info>,
    pub ping: Option<Result<PingOk, PingError>>,
}

#[derive(Clone, Debug)]
pub enum PingOk {
    Pong,
    Ping(std::time::Instant, std::time::Duration),
}

#[derive(Clone, Debug)]
pub enum PingError {
    Timeout,
    Unsupported,
    Other(String),
}

pub struct ActiveAccount {
    pub index: usize,
    pub name: String,
    pub peer_id: PeerId,
    pub keypair: Keypair,
    pub swarm: Swarm<TheManBehaviour>,
    pub friends: Vec<Friend>,
    pub expires: Instant,
    pub auto_renew: bool,
    pub voice_channels: HashMap<String, HashMap<PeerId, usize>>,
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
            cfg.set_connection_idle_timeout(Duration::from_secs(60 * 5));
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
                let Protocol::P2p(peer_id) = protocol else {continue};
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
            let mut config = libp2p::autonat::Config::default();
            config.only_global_ips = false;
            libp2p::autonat::Behaviour::new(peer_id, config)
        };

        let relay = {
            let config = libp2p::relay::Config::default();
            libp2p::relay::Behaviour::new(peer_id, config)
        };

        let ping = { libp2p::ping::Behaviour::new(libp2p::ping::Config::new()) };

        let the_man = { the_man::network::TheManBehaviour::new(peer_id) };

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
                the_man,
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
            friends: account.friends.clone(),
            index: account_index,
            voice_channels: HashMap::new(),
            auto_renew: account.renew,
        };

        self.account = Some(account)
    }
}

#[derive(NetworkBehaviour)]
pub struct TheManBehaviour {
    pub kademlia: Kademlia<MemoryStore>,
    pub identify: libp2p::identify::Behaviour,
    pub mdns: libp2p::mdns::tokio::Behaviour,
    pub gossipsub: libp2p::gossipsub::Behaviour,
    pub autonat: libp2p::autonat::Behaviour,
    pub relay: libp2p::relay::Behaviour,
    pub ping: libp2p::ping::Behaviour,
    pub the_man: the_man::network::TheManBehaviour,
}
