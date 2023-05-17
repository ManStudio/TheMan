use libp2p::{
    core::{muxing::SubstreamBox, upgrade::ReadyUpgrade, Negotiated},
    futures::{future::BoxFuture, AsyncReadExt, AsyncWriteExt, FutureExt},
    swarm::{ConnectionHandler, SubstreamProtocol},
};

use super::{Failure, TheManBehaviour};

pub struct Connection {
    init: bool,
    inbound: Option<BoxFuture<'static, Negotiated<SubstreamBox>>>,
    outbound: Option<BoxFuture<'static, Negotiated<SubstreamBox>>>,
}

impl Connection {
    pub fn new() -> Result<libp2p::swarm::THandler<TheManBehaviour>, libp2p::swarm::ConnectionDenied>
    {
        Ok(Self {
            init: false,
            inbound: None,
            outbound: None,
        })
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
            if let Some(inbound) = &mut self.inbound {
                match inbound.poll_unpin(cx) {
                    std::task::Poll::Ready(_) => {
                        println!("Recv!");
                    }
                    std::task::Poll::Pending => {}
                }
            }

            if let Some(outbount) = &mut self.outbound {
                match outbount.poll_unpin(cx) {
                    std::task::Poll::Ready(_) => {
                        println!("Sent!");
                    }
                    std::task::Poll::Pending => {}
                }
            }
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
                self.inbound = Some(send(event.protocol).boxed());
            }
            libp2p::swarm::handler::ConnectionEvent::FullyNegotiatedOutbound(event) => {
                println!("Outbound: {:?}", event.protocol);
                self.outbound = Some(recv(event.protocol).boxed())
            }
            _ => {}
        }
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
