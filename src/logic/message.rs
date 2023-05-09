use libp2p::{kad::kbucket::NodeStatus, multiaddr::Protocol, Multiaddr, PeerId};

use crate::{save_state::TheManSaveState, state::PeerStatus};

use super::TheManLogic;

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
    Peer(PeerId),
    ShutDown,
}

unsafe impl Send for Message {}
unsafe impl Sync for Message {}

impl TheManLogic {
    pub async fn on_message(&mut self, message: Message) {
        match message {
            Message::Save => {
                let save_state = {
                    let mut nodes = Vec::new();
                    for connection in self.state.swarm.behaviour_mut().kademlia.kbuckets() {
                        for peer in connection.iter() {
                            for adress in peer.node.value.iter() {
                                if let Some(Protocol::P2p(_)) = adress.iter().last() {
                                    nodes.push(adress.clone());
                                } else {
                                    let mut adress = adress.clone();
                                    adress.push(Protocol::P2p((*peer.node.key.preimage()).into()));
                                }
                            }
                        }
                    }

                    let private = self.state.keypair.to_protobuf_encoding().unwrap();

                    TheManSaveState { private, nodes }
                };
                let _ = self.sender.try_send(Message::SaveResponse(save_state));
            }
            Message::GetBootNodes => {
                let mut peers = Vec::new();
                for kbucket in self.state.swarm.behaviour_mut().kademlia.kbuckets() {
                    for peer in kbucket.iter() {
                        peers.push((
                            *peer.node.key.preimage(),
                            peer.status,
                            peer.node.value.iter().cloned().collect::<Vec<Multiaddr>>(),
                        ));
                    }
                }
                let _ = self.sender.try_send(Message::BootNodes(peers));
            }
            Message::GetPeers => {
                let _ = self.sender.try_send(Message::Peers(
                    self.state
                        .peers
                        .iter()
                        .map(|(d, e)| (*d, e.clone()))
                        .collect::<Vec<_>>(),
                ));
            }
            Message::Bootstrap => {
                let _query_id = self
                    .state
                    .swarm
                    .behaviour_mut()
                    .kademlia
                    .bootstrap()
                    .unwrap();
            }
            _ => {}
        }
    }
}
