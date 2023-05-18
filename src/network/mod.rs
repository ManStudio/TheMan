use std::collections::{HashMap, HashSet, VecDeque};

use libp2p::{
    core::upgrade::ReadyUpgrade,
    swarm::{ConnectionHandler, NetworkBehaviour, SubstreamProtocol, THandlerInEvent, ToSwarm},
    PeerId,
};

use self::handler::Connection;

pub mod event;
pub mod handler;
pub mod packet;

pub struct TheManBehaviour {
    peer_id: PeerId,
    events: VecDeque<ToSwarm<event::BehaviourEvent, handler::InputEvent>>,
    mesh: HashMap<String, HashMap<PeerId, Stage>>,
    peers: HashSet<PeerId>,
    connected: HashSet<String>,
    auto_accept: bool,
}

#[derive(Debug)]
pub enum Stage {
    Requested,
    Accepted,
}

impl TheManBehaviour {
    pub fn new(peer_id: PeerId) -> Self {
        Self {
            peer_id,
            events: VecDeque::new(),
            mesh: HashMap::new(),
            connected: HashSet::new(),
            auto_accept: true,
            peers: HashSet::new(),
        }
    }

    pub fn connect(&mut self, channel: String) {
        for peer in self.peers.iter() {
            self.events.push_back(ToSwarm::NotifyHandler {
                peer_id: *peer,
                handler: libp2p::swarm::NotifyHandler::Any,
                event: handler::InputEvent::Connect(channel.clone()),
            });
        }
        self.connected.insert(channel);
    }

    pub fn disconnect(&mut self, channel: String) {
        for peer in self.peers.iter() {
            self.events.push_back(ToSwarm::NotifyHandler {
                peer_id: *peer,
                handler: libp2p::swarm::NotifyHandler::Any,
                event: handler::InputEvent::Disconnect(channel.clone()),
            });
        }
        self.connected.remove(&channel);
    }

    pub fn audio_packet(&mut self, codec: String, data: Vec<u8>) {
        for (channel, stages) in self.mesh.iter() {
            let peers = stages
                .iter()
                .flat_map(|(p, s)| {
                    if let Stage::Accepted = s {
                        Some(*p)
                    } else {
                        None
                    }
                })
                .collect::<Vec<PeerId>>();

            for peer in peers {
                self.events.push_back(ToSwarm::NotifyHandler {
                    peer_id: peer,
                    handler: libp2p::swarm::NotifyHandler::Any,
                    event: handler::InputEvent::VoicePacket {
                        codec: codec.clone(),
                        data: data.clone(),
                        channel: channel.clone(),
                    },
                })
            }
        }
    }
    pub fn accept(&mut self, channel: String, peer_id: PeerId) {
        if let Some(mesh) = self.mesh.get_mut(&channel) {
            mesh.insert(peer_id, Stage::Accepted);
        } else {
            let mut hash = HashMap::new();
            hash.insert(peer_id, Stage::Accepted);
            self.mesh.insert(channel, hash);
        }
    }
    pub fn refuze(&mut self, channel: String, peer_id: PeerId) {
        if let Some(mesh) = self.mesh.get_mut(&channel) {
            mesh.insert(peer_id, Stage::Requested);
        } else {
            let mut hash = HashMap::new();
            hash.insert(peer_id, Stage::Requested);
            self.mesh.insert(channel, hash);
        }
    }
}

impl NetworkBehaviour for TheManBehaviour {
    type ConnectionHandler = Connection;

    type OutEvent = event::BehaviourEvent;

    fn on_swarm_event(&mut self, event: libp2p::swarm::FromSwarm<Self::ConnectionHandler>) {
        match event {
            libp2p::swarm::FromSwarm::ConnectionClosed(event) => {
                self.peers.remove(&event.peer_id);
            }
            _ => {}
        }
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: libp2p::PeerId,
        connection_id: libp2p::swarm::ConnectionId,
        event: handler::OutputEvent,
    ) {
        // println!("SWEV: PeerId: {peer_id}, event: {event:?}");
        match event {
            handler::OutputEvent::VoicePacket {
                codec,
                data,
                channel,
            } => {
                if self.connected.contains(&channel) {
                    if let Some(connection) = self.mesh.get(&channel) {
                        if let Some(Stage::Accepted) = connection.get(&peer_id) {
                            self.events.push_back(ToSwarm::GenerateEvent(
                                event::BehaviourEvent::VoicePacket {
                                    from: peer_id,
                                    codec,
                                    data,
                                    channel,
                                },
                            ));
                        }
                    }
                }
            }
            handler::OutputEvent::Connected(channel) => {
                let stage = if self.auto_accept {
                    Stage::Accepted
                } else {
                    self.events
                        .push_back(ToSwarm::GenerateEvent(event::BehaviourEvent::Request {
                            channel: channel.clone(),
                            from: peer_id,
                        }));
                    Stage::Requested
                };
                if let Some(mesh) = self.mesh.get_mut(&channel) {
                    mesh.insert(peer_id, stage);
                } else {
                    let mut hash = HashMap::new();
                    hash.insert(peer_id, stage);
                    self.mesh.insert(channel, hash);
                }
            }
            handler::OutputEvent::Disconnected(channel) => {
                self.events.push_back(ToSwarm::GenerateEvent(
                    event::BehaviourEvent::Disconnected {
                        channel: channel.clone(),
                        from: peer_id,
                    },
                ));
                if let Some(mesh) = self.mesh.get_mut(&channel) {
                    mesh.remove(&peer_id);
                }
            }
            handler::OutputEvent::SuccesfulyConnect => {
                self.peers.insert(peer_id);
            }
        }
    }

    fn poll(
        &mut self,
        cx: &mut std::task::Context<'_>,
        params: &mut impl libp2p::swarm::PollParameters,
    ) -> std::task::Poll<ToSwarm<Self::OutEvent, THandlerInEvent<Self>>> {
        if let Some(event) = self.events.pop_front() {
            return std::task::Poll::Ready(event);
        }
        std::task::Poll::Pending
    }

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: libp2p::swarm::ConnectionId,
        peer: libp2p::PeerId,
        local_addr: &libp2p::Multiaddr,
        remote_addr: &libp2p::Multiaddr,
    ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
        Connection::new(self.peer_id, peer, self.connected.clone())
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: libp2p::swarm::ConnectionId,
        peer: libp2p::PeerId,
        addr: &libp2p::Multiaddr,
        role_override: libp2p::core::Endpoint,
    ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
        Connection::new(self.peer_id, peer, self.connected.clone())
    }
}

#[derive(Debug)]
pub enum Failure {
    Other {
        error: Box<dyn std::error::Error + Send + 'static>,
    },
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Failure::Other { error } => write!(f, "TheMan error: {error}"),
        }
    }
}

impl std::error::Error for Failure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Failure::Other { error } => Some(&**error),
        }
    }
}
