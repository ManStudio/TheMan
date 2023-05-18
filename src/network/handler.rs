use std::collections::{HashSet, VecDeque};

use bytes_kman::TBytes;
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
        if let Some(event) = self.out_events.pop_front() {
            return std::task::Poll::Ready(event);
        }

        if !self.connected && self.inbound.initial() && self.outbound.initial() {
            println!("Connected!");
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
                    self.inbound = Stage::RunningBase(async { (stream, None, Vec::new()) }.boxed());
                }
                std::task::Poll::Pending => {
                    self.inbound = Stage::RunningInitial(future);
                }
            },
            Stage::RunningBase(mut future) => match future.poll_unpin(cx) {
                std::task::Poll::Ready((mut stream, event, mut buffer)) => {
                    if let Some(event) = event {
                        self.out_events.push_back(event);
                    }
                    self.inbound = Stage::RunningBase(
                        async {
                            let packet = Packet::from_bytes(&mut buffer.drain(..));
                            if let Some(packet) = packet {
                                match packet {
                                    Packet::VoicePacket {
                                        codec,
                                        data,
                                        channel,
                                    } => {
                                        return (
                                            stream,
                                            Some(ConnectionHandlerEvent::Custom(
                                                OutputEvent::VoicePacket {
                                                    codec,
                                                    data,
                                                    channel,
                                                },
                                            )),
                                            buffer,
                                        )
                                    }
                                    Packet::VoiceRequest { codec, channel } => {}
                                    Packet::VoiceAccept { codec, channel } => {}
                                    Packet::VoiceRefuze { codec, channel } => {}
                                    Packet::VoiceDisconnect { channel } => {
                                        return (
                                            stream,
                                            Some(ConnectionHandlerEvent::Custom(
                                                OutputEvent::Disconnected(channel),
                                            )),
                                            buffer,
                                        )
                                    }
                                    Packet::VoiceConnect { channel } => {
                                        println!("Recv channel: {channel}");
                                        return (
                                            stream,
                                            Some(ConnectionHandlerEvent::Custom(
                                                OutputEvent::Connected(channel),
                                            )),
                                            buffer,
                                        );
                                    }
                                }
                            } else {
                                let mut tmp_buffer = [0; 1024 * 16];
                                let len = stream.read(&mut tmp_buffer).await.unwrap();
                                buffer.append(&mut tmp_buffer[..len].to_vec());
                            }
                            (stream, None, buffer)
                        }
                        .boxed(),
                    );
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
                                    .write_all(&Packet::VoiceConnect { channel }.to_bytes())
                                    .await
                                    .unwrap();
                            }
                            (stream, None, Vec::new())
                        }
                        .boxed(),
                    );
                }
                std::task::Poll::Pending => {
                    self.outbound = Stage::RunningInitial(future);
                }
            },
            Stage::RunningBase(mut future) => match future.poll_unpin(cx) {
                std::task::Poll::Ready((mut stream, event, mut buffer)) => {
                    if let Some(event) = self.events.pop_front() {
                        self.outbound = Stage::RunningBase(
                            async {
                                stream
                                    .write_all(
                                        &match event {
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
                                        }
                                        .to_bytes(),
                                    )
                                    .await;
                                (stream, None, buffer)
                            }
                            .boxed(),
                        );
                    } else {
                        self.outbound =
                            Stage::RunningBase(async { (stream, None, Vec::new()) }.boxed());
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
                Vec<u8>,
            ),
        >,
    ),
}

impl Stage {
    pub fn initial(&self) -> bool {
        match self {
            Stage::Initial(_) => true,
            Stage::RunningInitial(_) => true,
            Stage::RunningBase(_) => true,
            _ => false,
        }
    }

    pub fn take(&mut self) -> Stage {
        std::mem::replace(self, Stage::None)
    }
}
