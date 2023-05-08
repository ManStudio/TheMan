use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use crate::{
    save_state::TheManSaveState,
    state::{BehaviourEvent, PeerStatus, TheManState},
};
use libp2p::{futures::StreamExt, multiaddr::Protocol, multihash::Multihash, Multiaddr, PeerId};
use tokio::sync::mpsc::{Receiver, Sender};

use self::message::Message;

pub mod message;

pub async fn run(
    mut state: TheManState,
    mut sender: Sender<Message>,
    mut reciver: Receiver<Message>,
) {
    sender.try_send(Message::SwarmStatus(state.swarm.network_info()));

    state.swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse().unwrap());
    let mut bootstrap = state.swarm.behaviour_mut().kademlia.bootstrap().unwrap();

    loop {
        tokio::select! {
            Some(message) = reciver.recv() => match message{
                Message::ShutDown => {break},
                Message::Save => {
                    let save_state = {
                        let mut nodes = Vec::new();
                        for connection in state.swarm.behaviour_mut().kademlia.kbuckets() {
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
                Message::GetBootNodes => {
                    let mut peers = Vec::new();
                    for kbucket in state.swarm.behaviour_mut().kademlia.kbuckets() {
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
                    sender.try_send(Message::BootNodes(peers));
                }
                Message::GetPeers =>{
                    sender.try_send(Message::Peers(state.peers.iter().map(|(d, e)|(d.clone(), e.clone())).collect::<Vec<_>>()));
                }
                Message::Bootstrap => {
                    let query_id = state.swarm.behaviour_mut().kademlia.bootstrap().unwrap();
                }
                _=>{}
            },
            event = state.swarm.select_next_some() => {
                // println!("Event: {event:?}");
                match event{
                    libp2p::swarm::SwarmEvent::Behaviour(event) => {
                        match event{BehaviourEvent::Kademlia(event)=>match event {
                                libp2p::kad::KademliaEvent::InboundRequest { request } => {
                                    println!("Request: {request:?}");
                                }
                                libp2p::kad::KademliaEvent::OutboundQueryProgressed {
                                    id,
                                    result,
                                    stats,
                                    step,
                                } => {
                                    if id == bootstrap && step.last {
                                        bootstrap = state.swarm.behaviour_mut().kademlia.bootstrap().unwrap();
                                    }
                                }
                                libp2p::kad::KademliaEvent::RoutingUpdated {
                                    peer,
                                    is_new_peer,
                                    addresses,
                                    bucket_range,
                                    old_peer,
                                } => {}
                                libp2p::kad::KademliaEvent::UnroutablePeer { peer } => {}
                                libp2p::kad::KademliaEvent::RoutablePeer { peer, address } => {}
                                libp2p::kad::KademliaEvent::PendingRoutablePeer { peer, address } => {}
                            }
                            BehaviourEvent::Identify(event) => {
                                match event{
                                    libp2p::identify::Event::Received { peer_id, info } => {
                                        if let Some(peer) = state.peers.get_mut(&peer_id){
                                            peer.info = Some(info);
                                        }
                                        // println!("Info: {info:?}");
                                    },
                                    libp2p::identify::Event::Sent { peer_id } => {},
                                    libp2p::identify::Event::Pushed { peer_id } => {},
                                    libp2p::identify::Event::Error { peer_id, error } => {},
                                }
                            },
                            BehaviourEvent::MDNS(event) => {
                                match event{
                                    libp2p::mdns::Event::Discovered(discovered) => println!("Discovered: {discovered:?}"),
                                    libp2p::mdns::Event::Expired(_) => todo!(),
                                }
                            },
                            BehaviourEvent::GossIpSub(event) => {
                            },
                            BehaviourEvent::AutoNat(event) => {
                                match event{
                                    libp2p::autonat::Event::InboundProbe(event) => { println!("Inbount: {event:?}")},
                                    libp2p::autonat::Event::OutboundProbe(event) => { println!("Outbound: {event:?}")},
                                    libp2p::autonat::Event::StatusChanged { old, new } => {
                                        println!("NatStatus: {new:?}");
                                        println!("Adress: {:?}", state.swarm.behaviour_mut().autonat.public_address());
                                    },
                                }
                            },
                            BehaviourEvent::Relay(event) => {
                                println!("Relay: {event:?}");
                            },
                            BehaviourEvent::Ping(event) => {
                                if let Some(peer) = state.peers.get_mut(&event.peer){
                                    peer.ping = Some(event.result);
                                }
                            }
                        }
                    },
                    libp2p::swarm::SwarmEvent::ConnectionEstablished { peer_id, endpoint, num_established, concurrent_dial_errors, established_in } => {
                        state.peers.insert(peer_id, PeerStatus::default());
                        sender.try_send(Message::SwarmStatus(state.swarm.network_info()));
                    },
                    libp2p::swarm::SwarmEvent::ConnectionClosed { peer_id, endpoint, num_established, cause } => {
                        state.peers.remove(&peer_id);
                        sender.try_send(Message::SwarmStatus(state.swarm.network_info()));
                    },
                    libp2p::swarm::SwarmEvent::IncomingConnection { local_addr, send_back_addr } => {
                        sender.try_send(Message::SwarmStatus(state.swarm.network_info()));
                    },
                    libp2p::swarm::SwarmEvent::IncomingConnectionError { local_addr, send_back_addr, error } => {
                        sender.try_send(Message::SwarmStatus(state.swarm.network_info()));
                    },
                    libp2p::swarm::SwarmEvent::OutgoingConnectionError { peer_id, error } => {
                        sender.try_send(Message::SwarmStatus(state.swarm.network_info()));
                    },
                    libp2p::swarm::SwarmEvent::BannedPeer { peer_id, endpoint } => {
                        sender.try_send(Message::SwarmStatus(state.swarm.network_info()));
                    },
                    libp2p::swarm::SwarmEvent::NewListenAddr { listener_id, address } => {
                        sender.try_send(Message::SwarmStatus(state.swarm.network_info()));
                    },
                    libp2p::swarm::SwarmEvent::ExpiredListenAddr { listener_id, address } => {
                        sender.try_send(Message::SwarmStatus(state.swarm.network_info()));
                    },
                    libp2p::swarm::SwarmEvent::ListenerClosed { listener_id, addresses, reason } => {
                        sender.try_send(Message::SwarmStatus(state.swarm.network_info()));
                    },
                    libp2p::swarm::SwarmEvent::ListenerError { listener_id, error } => {
                        sender.try_send(Message::SwarmStatus(state.swarm.network_info()));
                    },
                    libp2p::swarm::SwarmEvent::Dialing(_) => {},
                };
            }
        }
    }
    println!("Worker thread exited!");
}
