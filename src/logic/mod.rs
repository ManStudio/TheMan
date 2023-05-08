use std::sync::{Arc, RwLock};

use crate::{save_state::TheManSaveState, state::TheManState};
use libp2p::{futures::StreamExt, multiaddr::Protocol, multihash::Multihash, Multiaddr};
use tokio::sync::mpsc::{Receiver, Sender};

use self::message::Message;

pub mod message;

pub async fn run(
    mut state: TheManState,
    mut sender: Sender<Message>,
    mut reciver: Receiver<Message>,
) {
    sender.try_send(Message::KademliaStatus(state.kademlia.network_info()));

    let mut bootstrap = state.kademlia.behaviour_mut().bootstrap().unwrap();

    loop {
        tokio::select! {
            Some(message) = reciver.recv() => match message{
                Message::ShutDown => {break},
                Message::Save => {
                    let save_state = {
                        let mut nodes = Vec::new();
                        for connection in state.kademlia.behaviour_mut().kbuckets() {
                            for peer in connection.iter() {
                                for adress in peer.node.value.iter() {
                                    if let Some(Protocol::P2p(_)) = adress.iter().last() {
                                        nodes.push(adress.clone());
                                    } else {
                                        let mut adress = adress.clone();
                                        adress.push(Protocol::P2p(peer.node.key.preimage().clone().into()));
                                    }
                                }
                            }
                        }

                        let private = state.keypair.to_protobuf_encoding().unwrap();

                        TheManSaveState { private, nodes }
                    };
                    sender.try_send(Message::SaveResponse(save_state));
                },
                Message::GetPeers => {
                    let mut peers = Vec::new();
                    for kbucket in state.kademlia.behaviour_mut().kbuckets() {
                        for peer in kbucket.iter() {
                            peers.push((
                                peer.node.key.preimage().clone(),
                                peer.status,
                                peer.node
                                    .value
                                    .iter()
                                    .map(|adress| adress.clone())
                                    .collect::<Vec<Multiaddr>>(),
                            ));
                        }
                    }
                    sender.try_send(Message::Peers(peers));
                }
                Message::Bootstrap => {
                    let query_id = state.kademlia.behaviour_mut().bootstrap().unwrap();
                }
                _=>{}
            },
            event = state.kademlia.select_next_some() => {
                // println!("Event: {event:?}");
                match event{
                    libp2p::swarm::SwarmEvent::Behaviour(event) => {
                        match event{
                            libp2p::kad::KademliaEvent::InboundRequest { request } => {
                                println!("Request: {request:?}");
                            },
                            libp2p::kad::KademliaEvent::OutboundQueryProgressed { id, result, stats, step } => {
                                if id == bootstrap && step.last {
                                    bootstrap = state.kademlia.behaviour_mut().bootstrap().unwrap();
                                }
                            },
                            libp2p::kad::KademliaEvent::RoutingUpdated { peer, is_new_peer, addresses, bucket_range, old_peer } => {},
                            libp2p::kad::KademliaEvent::UnroutablePeer { peer } => {},
                            libp2p::kad::KademliaEvent::RoutablePeer { peer, address } => {},
                            libp2p::kad::KademliaEvent::PendingRoutablePeer { peer, address } => {},
                        }
                    },
                    libp2p::swarm::SwarmEvent::ConnectionEstablished { peer_id, endpoint, num_established, concurrent_dial_errors, established_in } => {
                        sender.try_send(Message::KademliaStatus(state.kademlia.network_info()));
                    },
                    libp2p::swarm::SwarmEvent::ConnectionClosed { peer_id, endpoint, num_established, cause } => {
                        sender.try_send(Message::KademliaStatus(state.kademlia.network_info()));
                    },
                    libp2p::swarm::SwarmEvent::IncomingConnection { local_addr, send_back_addr } => {
                        sender.try_send(Message::KademliaStatus(state.kademlia.network_info()));
                    },
                    libp2p::swarm::SwarmEvent::IncomingConnectionError { local_addr, send_back_addr, error } => {
                        sender.try_send(Message::KademliaStatus(state.kademlia.network_info()));
                    },
                    libp2p::swarm::SwarmEvent::OutgoingConnectionError { peer_id, error } => {
                        sender.try_send(Message::KademliaStatus(state.kademlia.network_info()));
                    },
                    libp2p::swarm::SwarmEvent::BannedPeer { peer_id, endpoint } => {
                        sender.try_send(Message::KademliaStatus(state.kademlia.network_info()));
                    },
                    libp2p::swarm::SwarmEvent::NewListenAddr { listener_id, address } => {
                        sender.try_send(Message::KademliaStatus(state.kademlia.network_info()));
                    },
                    libp2p::swarm::SwarmEvent::ExpiredListenAddr { listener_id, address } => {
                        sender.try_send(Message::KademliaStatus(state.kademlia.network_info()));
                    },
                    libp2p::swarm::SwarmEvent::ListenerClosed { listener_id, addresses, reason } => {
                        sender.try_send(Message::KademliaStatus(state.kademlia.network_info()));
                    },
                    libp2p::swarm::SwarmEvent::ListenerError { listener_id, error } => {
                        sender.try_send(Message::KademliaStatus(state.kademlia.network_info()));
                    },
                    libp2p::swarm::SwarmEvent::Dialing(_) => {},
                };
            }
        }
    }
    println!("Worker thread exited!");
}
