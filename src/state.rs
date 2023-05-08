use libp2p::{
    core::PeerId,
    identity::Keypair,
    kad::{store::MemoryStore, Kademlia},
    Swarm,
};

pub struct TheManState {
    pub peer_id: PeerId,
    pub keypair: Keypair,
    pub kademlia: Swarm<Kademlia<MemoryStore>>,
}
