use libp2p::{
    core::upgrade::ReadyUpgrade,
    swarm::{ConnectionHandler, NetworkBehaviour, SubstreamProtocol},
};

pub mod event;
pub mod packet;

pub struct TheManBehaviour {}

impl TheManBehaviour {
    pub fn new() -> Self {
        Self {}
    }
}

impl NetworkBehaviour for TheManBehaviour {
    type ConnectionHandler = Connection;

    type OutEvent = event::BehaviourEvent;

    fn on_swarm_event(&mut self, event: libp2p::swarm::FromSwarm<Self::ConnectionHandler>) {
        match event {
            libp2p::swarm::FromSwarm::ConnectionEstablished(event) => {}
            libp2p::swarm::FromSwarm::ConnectionClosed(_) => {}
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
        event: libp2p::swarm::THandlerOutEvent<Self>,
    ) {
        println!("SWEV: PeerId: {peer_id}, event: {event:?}");
    }

    fn poll(
        &mut self,
        cx: &mut std::task::Context<'_>,
        params: &mut impl libp2p::swarm::PollParameters,
    ) -> std::task::Poll<libp2p::swarm::ToSwarm<Self::OutEvent, libp2p::swarm::THandlerInEvent<Self>>>
    {
        std::task::Poll::Pending
        // println!(
        //     "sup: {:?}",
        //     params
        //         .supported_protocols()
        //         .flat_map(|d| String::from_utf8(d))
        //         .collect::<Vec<String>>()
        // );
    }

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: libp2p::swarm::ConnectionId,
        peer: libp2p::PeerId,
        local_addr: &libp2p::Multiaddr,
        remote_addr: &libp2p::Multiaddr,
    ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
        // println!("ConnectionId: {:?}", _connection_id);
        // println!("Peer: {}", peer);
        // println!("Local_addr: {:?}", local_addr);
        // println!("Remote addr: {:?}", remote_addr);
        Connection::new()
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: libp2p::swarm::ConnectionId,
        peer: libp2p::PeerId,
        addr: &libp2p::Multiaddr,
        role_override: libp2p::core::Endpoint,
    ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
        // println!("ConnectionId: {:?}", _connection_id);
        // println!("Peer: {}", peer);
        // println!("addr: {:?}", addr);
        // println!("Role override: {:?}", role_override);
        Connection::new()
    }
}

pub struct Connection {
    init: bool,
}

impl Connection {
    pub fn new() -> Result<libp2p::swarm::THandler<TheManBehaviour>, libp2p::swarm::ConnectionDenied>
    {
        Ok(Self { init: false })
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

impl ConnectionHandler for Connection {
    type InEvent = ();

    type OutEvent = ();

    type Error = Failure;

    type InboundProtocol = ReadyUpgrade<&'static str>;

    type OutboundProtocol = ReadyUpgrade<&'static str>;

    type InboundOpenInfo = String;

    type OutboundOpenInfo = String;

    fn listen_protocol(
        &self,
    ) -> libp2p::swarm::SubstreamProtocol<Self::InboundProtocol, Self::InboundOpenInfo> {
        SubstreamProtocol::new(ReadyUpgrade::new("/the-man/1.0.0"), "Test".into())
    }

    fn connection_keep_alive(&self) -> libp2p::swarm::KeepAlive {
        libp2p::swarm::KeepAlive::Yes
    }

    fn poll(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<
        libp2p::swarm::ConnectionHandlerEvent<
            Self::OutboundProtocol,
            Self::OutboundOpenInfo,
            Self::OutEvent,
            Self::Error,
        >,
    > {
        if !self.init {
            self.init = true;

            std::task::Poll::Ready(
                libp2p::swarm::ConnectionHandlerEvent::OutboundSubstreamRequest {
                    protocol: SubstreamProtocol::new(
                        ReadyUpgrade::new("/the-man/1.0.0"),
                        "Test".into(),
                    ),
                },
            )
        } else {
            std::task::Poll::Pending
        }
    }

    fn on_behaviour_event(&mut self, _event: Self::InEvent) {}

    fn on_connection_event(
        &mut self,
        event: libp2p::swarm::handler::ConnectionEvent<
            Self::InboundProtocol,
            Self::OutboundProtocol,
            Self::InboundOpenInfo,
            Self::OutboundOpenInfo,
        >,
    ) {
        match event {
            libp2p::swarm::handler::ConnectionEvent::FullyNegotiatedInbound(event) => {
                println!("Inbound: {:?}", event.protocol);
            }
            libp2p::swarm::handler::ConnectionEvent::FullyNegotiatedOutbound(event) => {
                println!("Outbound: {:?}", event.protocol);
            }
            _ => {}
        }
    }
}
