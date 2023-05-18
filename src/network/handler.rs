use std::collections::{HashSet, VecDeque};

use libp2p::{
    core::{muxing::SubstreamBox, upgrade::ReadyUpgrade, Negotiated},
    futures::{future::BoxFuture, AsyncReadExt, AsyncWriteExt, FutureExt},
    swarm::{ConnectionHandler, ConnectionHandlerEvent, SubstreamProtocol},
    PeerId,
};

use super::{packet::Packet, Failure, TheManBehaviour};

pub struct Connection {
    init: bool,
    inbound: Stage,
    outbound: Stage,
    peer_id: PeerId,
    local_peer_id: PeerId,
    connected: bool,
    initial_connections: HashSet<String>,
    events: VecDeque<InputEvent>,
    out_events:
        VecDeque<ConnectionHandlerEvent<ReadyUpgrade<&'static str>, String, OutputEvent, Failure>>,
}

impl Connection {
    pub fn new(
        local_peer_id: PeerId,
        peer_id: PeerId,
        initial_connected: HashSet<String>,
    ) -> Result<libp2p::swarm::THandler<TheManBehaviour>, libp2p::swarm::ConnectionDenied> {
        Ok(Self {
            init: false,
            inbound: Stage::None,
            outbound: Stage::None,
            peer_id,
            local_peer_id,
            connected: false,
            initial_connections: initial_connected,
            events: VecDeque::new(),
            out_events: VecDeque::new(),
        })
    }
}

#[derive(Debug)]
pub enum InputEvent {
    VoicePacket {
        codec: String,
        data: Vec<u8>,
        channel: String,
    },
    Connect(String),
    Disconnect(String),
}

#[derive(Debug)]
pub enum OutputEvent {
    VoicePacket {
        codec: String,
        data: Vec<u8>,
        channel: String,
    },
    Connected(String),
    Disconnected(String),
    SuccesfulyConnect,
}

impl ConnectionHandler for Connection {
    type InEvent = InputEvent;

    type OutEvent = OutputEvent;

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
        if let Some(event) = self.out_events.pop_front() {
            return std::task::Poll::Ready(event);
        }
        if !self.init {
            self.init = true;

            return std::task::Poll::Ready(
                libp2p::swarm::ConnectionHandlerEvent::OutboundSubstreamRequest {
                    protocol: SubstreamProtocol::new(
                        ReadyUpgrade::new("/the-man/1.0.0"),
                        "Test".into(),
                    ),
                },
            );
        }

        if !self.connected && self.inbound.initial() && self.outbound.initial() {
            self.connected = true;
            return std::task::Poll::Ready(libp2p::swarm::ConnectionHandlerEvent::Custom(
                OutputEvent::SuccesfulyConnect,
            ));
        }

        match self.inbound.take() {
            Stage::None => {}
            Stage::Initial(stream) => {
                self.inbound = Stage::RunningInitial(async { stream }.boxed());
            }
            Stage::RunningInitial(mut future) => match future.poll_unpin(cx) {
                std::task::Poll::Ready(stream) => {
                    self.inbound = Stage::RunningBase(async { (stream, None) }.boxed());
                }
                std::task::Poll::Pending => {
                    self.inbound = Stage::RunningInitial(future);
                }
            },
            Stage::RunningBase(mut future) => match future.poll_unpin(cx) {
                std::task::Poll::Ready(stream) => {
                    self.inbound = Stage::RunningBase(async { stream }.boxed());
                }
                std::task::Poll::Pending => {
                    self.inbound = Stage::RunningBase(future);
                }
            },
        }

        match self.outbound.take() {
            Stage::None => {}
            Stage::Initial(stream) => {
                self.outbound = Stage::RunningInitial(async { stream }.boxed());
            }
            Stage::RunningInitial(mut future) => match future.poll_unpin(cx) {
                std::task::Poll::Ready(mut stream) => {
                    let channels = self.initial_connections.clone();
                    self.outbound = Stage::RunningBase(
                        async {
                            for channel in channels {
                                stream
                                    .write_all(
                                        &ron::to_string(&Packet::VoiceConnect { channel })
                                            .unwrap()
                                            .as_bytes(),
                                    )
                                    .await;
                            }
                            (stream, None)
                        }
                        .boxed(),
                    );
                }
                std::task::Poll::Pending => {
                    self.outbound = Stage::RunningInitial(future);
                }
            },
            Stage::RunningBase(mut future) => match future.poll_unpin(cx) {
                std::task::Poll::Ready((mut stream, event)) => {
                    if let Some(event) = self.events.pop_front() {
                        self.outbound = Stage::RunningBase(
                            async {
                                stream
                                    .write_all(
                                        &ron::to_string(&match event {
                                            InputEvent::VoicePacket {
                                                codec,
                                                data,
                                                channel,
                                            } => Packet::VoicePacket {
                                                codec,
                                                data,
                                                channel,
                                            },
                                            InputEvent::Connect(channel) => {
                                                Packet::VoiceConnect { channel }
                                            }
                                            InputEvent::Disconnect(channel) => {
                                                Packet::VoiceDisconnect { channel }
                                            }
                                        })
                                        .unwrap()
                                        .as_bytes(),
                                    )
                                    .await;
                                (stream, None)
                            }
                            .boxed(),
                        );
                    } else {
                        self.outbound = Stage::RunningBase(async { (stream, None) }.boxed());
                    }
                }
                std::task::Poll::Pending => {
                    self.outbound = Stage::RunningBase(future);
                }
            },
        }
        std::task::Poll::Pending
    }

    fn on_behaviour_event(&mut self, event: Self::InEvent) {
        println!("Conn Event: {event:?}");
        self.events.push_back(event);
    }

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
                self.inbound = Stage::Initial(event.protocol);
            }
            libp2p::swarm::handler::ConnectionEvent::FullyNegotiatedOutbound(event) => {
                println!("Outbound: {:?}", event.protocol);
                self.outbound = Stage::Initial(event.protocol)
            }
            _ => {}
        }
    }
}

pub enum Stage {
    None,
    Initial(Negotiated<SubstreamBox>),
    RunningInitial(BoxFuture<'static, Negotiated<SubstreamBox>>),
    RunningBase(
        BoxFuture<
            'static,
            (
                Negotiated<SubstreamBox>,
                Option<
                    ConnectionHandlerEvent<
                        ReadyUpgrade<&'static str>,
                        String,
                        OutputEvent,
                        Failure,
                    >,
                >,
            ),
        >,
    ),
}

impl Stage {
    pub fn initial(&self) -> bool {
        match self {
            Stage::Initial(_) => true,
            _ => false,
        }
    }

    pub fn take(&mut self) -> Stage {
        std::mem::replace(self, Stage::None)
    }
}

async fn send(mut stream: Negotiated<SubstreamBox>) -> Negotiated<SubstreamBox> {
    let _ = stream.write(b"Hello There!").await.unwrap();
    stream
}

async fn recv(mut stream: Negotiated<SubstreamBox>) -> Negotiated<SubstreamBox> {
    let mut buffer = [0; 1024];
    let len = stream.read(&mut buffer).await.unwrap();
    let text = String::from_utf8(buffer[0..len].to_vec()).unwrap();
    println!("Recv: {text}");
    stream
}
