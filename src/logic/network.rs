use libp2p::swarm::SwarmEvent;

use crate::state::{BehaviourEvent, PeerStatus};

use super::{message::Message, TheManLogic};

impl TheManLogic {
    pub async fn on_event<E>(&mut self, event: SwarmEvent<BehaviourEvent, E>) {
        // println!("Event: {event:?}");
        match event {
            libp2p::swarm::SwarmEvent::Behaviour(event) => {
                match event {
                    BehaviourEvent::Kademlia(event) => match event {
                        libp2p::kad::KademliaEvent::InboundRequest { request } => {
                            println!("Request: {request:?}");
                        }
                        libp2p::kad::KademliaEvent::OutboundQueryProgressed {
                            id,
                            step,
                            result,
                            stats,
                        } => {
                            if let Some(account) = &mut self.state.account {
                                if id == self.bootstrap.unwrap() {
                                    if step.last {
                                        self.bootstrap = Some(
                                            account
                                                .swarm
                                                .behaviour_mut()
                                                .kademlia
                                                .bootstrap()
                                                .unwrap(),
                                        );
                                    }
                                } else {
                                    let _ = self.sender.try_send(Message::KademliaQueryProgress(
                                        id, result, stats, step,
                                    ));
                                }
                            }
                        }
                        libp2p::kad::KademliaEvent::RoutingUpdated { .. } => {}
                        libp2p::kad::KademliaEvent::UnroutablePeer { .. } => {}
                        libp2p::kad::KademliaEvent::RoutablePeer { .. } => {}
                        libp2p::kad::KademliaEvent::PendingRoutablePeer { .. } => {}
                    },
                    BehaviourEvent::Identify(event) => {
                        match event {
                            libp2p::identify::Event::Received { peer_id, info } => {
                                if let Some(peer) = self.state.peers.get_mut(&peer_id) {
                                    peer.info = Some(info);
                                }
                                // println!("Info: {info:?}");
                            }
                            libp2p::identify::Event::Sent { .. } => {}
                            libp2p::identify::Event::Pushed { .. } => {}
                            libp2p::identify::Event::Error { .. } => {}
                        }
                    }
                    BehaviourEvent::MDNS(event) => match event {
                        libp2p::mdns::Event::Discovered(discovered) => {
                            println!("Discovered: {discovered:?}")
                        }
                        libp2p::mdns::Event::Expired(_) => todo!(),
                    },
                    BehaviourEvent::GossIpSub(event) => match event {
                        libp2p::gossipsub::Event::Message {
                            propagation_source,
                            message_id,
                            message,
                        } => {}
                        libp2p::gossipsub::Event::Subscribed { peer_id, topic } => {}
                        libp2p::gossipsub::Event::Unsubscribed { peer_id, topic } => {}
                        libp2p::gossipsub::Event::GossipsubNotSupported { peer_id } => {}
                    },
                    BehaviourEvent::AutoNat(event) => match event {
                        libp2p::autonat::Event::InboundProbe(event) => {
                            println!("Inbount: {event:?}")
                        }
                        libp2p::autonat::Event::OutboundProbe(event) => {
                            println!("Outbound: {event:?}")
                        }
                        libp2p::autonat::Event::StatusChanged { new, .. } => {
                            println!("NatStatus: {new:?}");
                            if let Some(account) = &mut self.state.account {
                                println!(
                                    "Adress: {:?}",
                                    account.swarm.behaviour_mut().autonat.public_address()
                                );
                            }
                        }
                    },
                    BehaviourEvent::Relay(event) => {
                        println!("Relay: {event:?}");
                    }
                    BehaviourEvent::Ping(event) => {
                        if let Some(peer) = self.state.peers.get_mut(&event.peer) {
                            peer.ping = Some(event.result);
                        }
                    }
                }
            }
            libp2p::swarm::SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                self.state.peers.insert(peer_id, PeerStatus::default());
                self.update_swarm_status();
            }
            libp2p::swarm::SwarmEvent::ConnectionClosed { peer_id, .. } => {
                self.state.peers.remove(&peer_id);
                self.update_swarm_status()
            }
            libp2p::swarm::SwarmEvent::IncomingConnection { .. } => self.update_swarm_status(),
            libp2p::swarm::SwarmEvent::IncomingConnectionError { .. } => self.update_swarm_status(),
            libp2p::swarm::SwarmEvent::OutgoingConnectionError { .. } => self.update_swarm_status(),
            libp2p::swarm::SwarmEvent::NewListenAddr { .. } => self.update_swarm_status(),
            libp2p::swarm::SwarmEvent::ExpiredListenAddr { .. } => self.update_swarm_status(),
            libp2p::swarm::SwarmEvent::ListenerClosed { .. } => self.update_swarm_status(),
            libp2p::swarm::SwarmEvent::ListenerError { .. } => self.update_swarm_status(),
            libp2p::swarm::SwarmEvent::Dialing(_) => {}
            _ => {}
        };
    }

    pub fn update_swarm_status(&mut self) {
        if let Some(account) = &mut self.state.account {
            let _ = self
                .sender
                .try_send(Message::SwarmStatus(account.swarm.network_info()));
        }
    }
}
