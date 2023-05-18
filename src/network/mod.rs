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
    peers: HashSet<PeerId>,
    mesh: HashMap<String, HashMap<PeerId, Stage>>,
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
            peers: HashSet::new(),
            mesh: HashMap::new(),
            connected: HashSet::new(),
            auto_accept: false,
        }
    }
}

impl NetworkBehaviour for TheManBehaviour {
    type ConnectionHandler = Connection;

    type OutEvent = event::BehaviourEvent;

    fn on_swarm_event(&mut self, event: libp2p::swarm::FromSwarm<Self::ConnectionHandler>) {
        match event {
            libp2p::swarm::FromSwarm::ConnectionEstablished(event) => {}
            libp2p::swarm::FromSwarm::ConnectionClosed(event) => {
                self.peers.remove(&event.peer_id);
            }
            libp2p::swarm::FromSwarm::AddressChange(_) => {}
            libp2p::swarm::FromSwarm::DialFailure(_) => {}
            libp2p::swarm::FromSwarm::ListenFailure(_) => {}
            libp2p::swarm::FromSwarm::NewListener(_) => {}
            libp2p::swarm::FromSwarm::NewListenAddr(_) => {}
            libp2p::swarm::FromSwarm::ExpiredListenAddr(_) => {}
            libp2p::swarm::FromSwarm::ListenerError(_) => {}
            libp2p::swarm::FromSwarm::ListenerClosed(_) => {}
            libp2p::swarm::FromSwarm::NewExternalAddr(_) => {}
            libp2p::swarm::FromSwarm::ExpiredExternalAddr(_) => {}
        }
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: libp2p::PeerId,
        connection_id: libp2p::swarm::ConnectionId,
        event: handler::OutputEvent,
    ) {
        println!("SWEV: PeerId: {peer_id}, event: {event:?}");
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
            handler::OutputEvent::Disconnected(channel) => {}
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
        Connection::new(self.peer_id, peer)
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: libp2p::swarm::ConnectionId,
        peer: libp2p::PeerId,
        addr: &libp2p::Multiaddr,
        role_override: libp2p::core::Endpoint,
    ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
        Connection::new(self.peer_id, peer)
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
