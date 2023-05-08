use std::{num::NonZeroUsize, time::Duration};

use libp2p::{
    identity::Keypair,
    kad::{store::MemoryStore, Kademlia, KademliaConfig},
    multiaddr::Protocol,
    swarm::SwarmBuilder,
    Multiaddr, PeerId,
};

use crate::state::TheManState;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct TheManSaveState {
    pub private: Vec<u8>,
    pub nodes: Vec<Multiaddr>,
}

impl From<TheManSaveState> for TheManState {
    fn from(mut value: TheManSaveState) -> Self {
        let keypair = Keypair::from_protobuf_encoding(&value.private).unwrap();
        let peer_id = PeerId::from(keypair.public());

        let kademlia = {
            let transport = libp2p::tokio_development_transport(keypair.clone()).unwrap();
            let mut cfg = KademliaConfig::default();
            cfg.set_query_timeout(Duration::from_secs(5 * 60));
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

            value.nodes.append(&mut vec![
        		"/dnsaddr/bootstrap.libp2p.io/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN".parse().unwrap(),
        		"/dnsaddr/bootstrap.libp2p.io/p2p/QmQCU2EcMqAqQPR2i9bChDtGNJchTbq5TbXJJ16u19uLTa".parse().unwrap(),
        		"/dnsaddr/bootstrap.libp2p.io/p2p/QmbLHAnMoJPWSCR5Zhtx6BHJX9KiKNN6tpvbUcqanj75Nb".parse().unwrap(),
        		"/dnsaddr/bootstrap.libp2p.io/p2p/QmcZf59bWwK5XFi76CZX8cbJ4BhTzzA3gU1ZjYZcYW3dwt".parse().unwrap(),
        		"/ip4/104.131.131.82/tcp/4001/p2p/QmaCpDMGvV2BGHeYERUEnRQAwe3N8SzbUtfsmvsqQLuvuJ".parse().unwrap(),
        		"/ip4/104.131.131.82/udp/4001/quic/p2p/QmaCpDMGvV2BGHeYERUEnRQAwe3N8SzbUtfsmvsqQLuvuJ".parse().unwrap()]);

            for mut node in value.nodes {
                let Some(protocol) = node.iter().last() else {continue};
                let Protocol::P2p(id) = protocol else {continue};
                let Ok(peer_id) = PeerId::from_multihash(id)else{continue};
                log::debug!("Adding BOOTNODE to kademlia: {node}/p2p/{protocol}");
                behaviour.add_address(&peer_id, node);
            }
            SwarmBuilder::with_tokio_executor(transport, behaviour, peer_id).build()
        };

        Self {
            peer_id,
            keypair,
            kademlia,
        }
    }
}
