use std::collections::HashMap;

use libp2p::swarm::SwarmEvent;

use crate::state::{BehaviourEvent, PeerStatus, PingError};

use super::{message::Message, TheManLogic};

impl TheManLogic {
    pub async fn on_event<E>(&mut self, event: SwarmEvent<BehaviourEvent, E>) {
        // println!("Event: {event:?}");
        match event {
            libp2p::swarm::SwarmEvent::Behaviour(event) => {
                match event {
                    BehaviourEvent::Kademlia(event) => {
                        match event {
                            libp2p::kad::KademliaEvent::InboundRequest { .. } => {
                                // println!("Request: {request:?}");
                            }
                            libp2p::kad::KademliaEvent::OutboundQueryProgressed {
                                id,
                                step,
                                result,
                                stats,
                            } => {
                                if let Some(account) = &mut self.state.account {
                                    if id == self.bootstrap.unwrap() {
                                        if step.last && self.bootstraping {
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
                                        if let Some(registration_1) =
                                            self.registration_step_1_query.take()
                                        {
                                            if registration_1.0 == id {
                                                const SECS: u64 = 60 * 60 * 24 * 3;
                                                let instant = std::time::Instant::now()
                                                    + std::time::Duration::from_secs(SECS);
                                                self.registration_query = account
                                                    .swarm
                                                    .behaviour_mut()
                                                    .kademlia
                                                    .put_record(
                                                        libp2p::kad::Record {
                                                            key: libp2p::kad::RecordKey::new(
                                                                &libp2p::kad::record::Key::new(
                                                                    &registration_1.1,
                                                                ),
                                                            ),
                                                            value: account.peer_id.to_bytes(),
                                                            publisher: Some(account.peer_id),
                                                            expires: Some(instant),
                                                        },
                                                        libp2p::kad::Quorum::Majority,
                                                    )
                                                    .map_or_else(
                                                        |e| {
                                                            eprintln!(
                                                                "Cannot register itself: {e:?}"
                                                            );
                                                            None
                                                        },
                                                        |q| Some((q, instant)),
                                                    );
                                            } else {
                                                self.registration_step_1_query =
                                                    Some(registration_1)
                                            }

                                            if let Some((query_id, instant)) =
                                                self.registration_query.take()
                                            {
                                                if let libp2p::kad::QueryResult::PutRecord(_) =
                                                    result
                                                {
                                                    if query_id == id {
                                                        account.expires = instant;
                                                        if let Some(acc) = self
                                                            .state
                                                            .accounts
                                                            .get_mut(account.index)
                                                        {
                                                            acc.expires = chrono::Utc::now()
                                                                + chrono::Duration::from_std(
                                                                    account.expires.duration_since(
                                                                        std::time::Instant::now(),
                                                                    ),
                                                                )
                                                                .unwrap_or_else(|_| {
                                                                    chrono::Duration::zero()
                                                                })
                                                        }
                                                        let _ = self.sender.try_send(
                                                            Message::Accounts(
                                                                self.state.accounts.clone(),
                                                            ),
                                                        );
                                                    } else {
                                                        account.expires = std::time::Instant::now()
                                                            + std::time::Duration::from_secs(
                                                                60 * 15,
                                                            );
                                                    }
                                                } else {
                                                    self.registration_query =
                                                        Some((query_id, instant))
                                                }
                                            }
                                        }
                                        let _ = self.sender.try_send(
                                            Message::KademliaQueryProgress(id, result, stats, step),
                                        );
                                    }
                                }
                            }
                            libp2p::kad::KademliaEvent::RoutingUpdated { .. } => {}
                            libp2p::kad::KademliaEvent::UnroutablePeer { .. } => {}
                            libp2p::kad::KademliaEvent::RoutablePeer { .. } => {}
                            libp2p::kad::KademliaEvent::PendingRoutablePeer { .. } => {}
                        }
                    }
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
                        libp2p::mdns::Event::Discovered(_discovered) => {}
                        libp2p::mdns::Event::Expired(_) => {}
                    },
                    BehaviourEvent::GossIpSub(event) => match event {
                        libp2p::gossipsub::Event::Message { message, .. } => {
                            let topic = message.topic.clone();
                            let _ = self.sender.try_send(Message::NewMessage(topic, message));
                        }
                        libp2p::gossipsub::Event::Subscribed { peer_id, topic } => {
                            let _ = self.sender.try_send(Message::NewSubscribed(peer_id, topic));
                        }
                        libp2p::gossipsub::Event::Unsubscribed { peer_id, topic } => {
                            let _ = self
                                .sender
                                .try_send(Message::DestroySubscriber(peer_id, topic));
                        }
                        libp2p::gossipsub::Event::GossipsubNotSupported { .. } => {}
                    },
                    BehaviourEvent::AutoNat(event) => match event {
                        libp2p::autonat::Event::InboundProbe(_event) => {
                            // println!("Inbount: {event:?}")
                        }
                        libp2p::autonat::Event::OutboundProbe(_event) => {
                            // println!("Outbound: {event:?}")
                        }
                        libp2p::autonat::Event::StatusChanged { new, .. } => {
                            // println!("NatStatus: {new:?}");
                            if let Some(account) = &mut self.state.account {
                                println!("NatStatus: {new:?}");
                                println!(
                                    "Adress: {:?}",
                                    account.swarm.behaviour_mut().autonat.public_address()
                                );
                            }
                        }
                    },
                    BehaviourEvent::Relay(_event) => {
                        // println!("Relay: {event:?}");
                    }
                    BehaviourEvent::Ping(event) => {
                        if let Some(peer) = self.state.peers.get_mut(&event.peer) {
                            let ping = match event.result {
                                Ok(ping) => match ping {
                                    libp2p::ping::Success::Pong => Ok(crate::state::PingOk::Pong),
                                    libp2p::ping::Success::Ping { rtt } => {
                                        Ok(crate::state::PingOk::Ping(
                                            std::time::Instant::now() - rtt,
                                            rtt,
                                        ))
                                    }
                                },
                                Err(err) => match err {
                                    libp2p::ping::Failure::Timeout => Err(PingError::Timeout),
                                    libp2p::ping::Failure::Unsupported => {
                                        Err(crate::state::PingError::Unsupported)
                                    }
                                    libp2p::ping::Failure::Other { error } => {
                                        Err(crate::state::PingError::Other(error.to_string()))
                                    }
                                },
                            };
                            peer.ping = Some(ping);
                        }
                    }
                    BehaviourEvent::TheMan(event) => {
                        if let Some(account) = &mut self.state.account {
                            match event {
                                the_man::network::event::BehaviourEvent::VoicePacket {
                                    from,
                                    codec: _,
                                    data,
                                    channel,
                                } => {
                                    let mut id = None;
                                    if let Some(hash) = account.voice_channels.get(&channel) {
                                        if let Some(tmp_id) = hash.get(&from) {
                                            id = Some(*tmp_id);
                                        }
                                    }

                                    let id = if let Some(id) = id {
                                        id
                                    } else {
                                        let id = self.audio_counter;
                                        self.audio_counter += 1;
                                        let _ = self.audio_sender.try_send(Message::Audio(
                                            super::message::AudioMessage::CreateOutputChannel {
                                                id,
                                                codec: "opus".into(),
                                            },
                                        ));
                                        if let Some(channel) =
                                            account.voice_channels.get_mut(&channel)
                                        {
                                            channel.insert(from, id);
                                        } else {
                                            let mut hash = HashMap::new();
                                            hash.insert(from, id);
                                            account.voice_channels.insert(channel.clone(), hash);
                                        }
                                        id
                                    };

                                    let _ = self.audio_sender.try_send(Message::Audio(
                                        super::message::AudioMessage::OutputData { id, data },
                                    ));
                                }
                                the_man::network::event::BehaviourEvent::Request {
                                    channel,
                                    from,
                                } => {
                                    println!("Voice: request: channel: {channel}, from: {from}");
                                    let _ = self.sender.try_send(Message::Voice(
                                        crate::logic::message::VoiceMessage::Request(channel, from),
                                    ));
                                }
                                the_man::network::event::BehaviourEvent::Disconnected {
                                    channel,
                                    from,
                                } => {
                                    println!(
                                        "Voice: Disconnect:  channel: {channel}, from: {from}"
                                    );
                                    if let Some(channel) = account.voice_channels.get_mut(&channel)
                                    {
                                        if let Some(id) = channel.remove(&from) {
                                            let _ = self.audio_sender.try_send(Message::Audio(crate::logic::message::AudioMessage::DestroyOuputChannel { id }));
                                        }
                                    }
                                    let _ = self.sender.try_send(Message::Voice(
                                        crate::logic::message::VoiceMessage::UnRequest(
                                            channel, from,
                                        ),
                                    ));
                                }
                                the_man::network::event::BehaviourEvent::VoiceDisconnected {
                                    from,
                                } => {
                                    for (_, hash) in account.voice_channels.iter_mut() {
                                        if let Some(id) = hash.remove(&from) {
                                            let _ = self.audio_sender.try_send(Message::Audio(crate::logic::message::AudioMessage::DestroyOuputChannel { id }));
                                        }
                                    }
                                    let _ = self.sender.try_send(Message::Voice(
                                        crate::logic::message::VoiceMessage::Disconnected(from),
                                    ));
                                }
                                the_man::network::event::BehaviourEvent::VoiceErrorConnection {
                                    to,
                                    codec,
                                    channel,
                                    error,
                                } => {
                                    println!("VoiceErrorConnection: to: {to}, codec: {codec}, channel: {channel}, error: {error}");
                                }
                            }
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
