use std::{convert::Infallible, sync::Arc};

use base64::prelude::*;
use iced::{
    advanced::graphics::futures::MaybeSend, widget as W, Element, Length, Renderer, Subscription,
    Task, Theme,
};
use iroh::{protocol::Router, NodeId};
use protocol::{RawConversation, Ticket};
use serde::{de::DeserializeOwned, Serialize};
use tracing::error;

mod screen;

pub mod protocol;

#[derive(Debug, Clone)]
pub enum Screen {
    Login(screen::login::Login),
    Dashboard(screen::dashboard::Dashboard),
}

pub struct TheMan {
    screen: Screen,
    theme: Theme,
    popups: Vec<Popup<Message>>,
}

impl TheMan {
    pub fn new() -> Self {
        Self {
            screen: Screen::Login(screen::login::Login {
                name: String::default(),
                secret: String::default(),
                error: None,
                accounts: Vec::default(),
                loggingin: false,
            }),
            theme: Theme::Dark,
            popups: vec![],
        }
    }
}

#[derive(Clone)]
pub enum Message {
    CreatePopup(Popup<Message>),
    ClosePopup,
    ChangeScreen(Screen),
    Login(screen::login::Message),
    Dashboard(screen::dashboard::Message),
}

impl From<Message> for screen::dashboard::Message {
    fn from(value: Message) -> Self {
        if let Message::Dashboard(value) = value {
            value
        } else {
            panic!()
        }
    }
}

impl std::fmt::Debug for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Message").finish()
    }
}

impl TheMan {
    fn update(&mut self, mut message: Message) -> Task<Message> {
        if let Message::ClosePopup = &message {
            let Some(popup) = self.popups.pop() else {
                error!("No popup to close!");
                return Task::none();
            };
            let Some(n_message) = popup.0.finish() else {
                return Task::none();
            };
            message = n_message;
        }

        if let Some(popup) = self.popups.last_mut() {
            let task = popup.update(message);
            if let Some(m) = popup.finish() {
                self.popups.pop();
                message = m;
            } else {
                return task;
            }
        }

        match message {
            Message::CreatePopup(popup) => {
                self.popups.push(popup);
            }
            Message::ClosePopup => unreachable!(),
            Message::ChangeScreen(screen) => {
                self.screen = screen;
            }
            Message::Login(message) => {
                if let Screen::Login(login) = &mut self.screen {
                    return login.update(message);
                }
            }
            Message::Dashboard(message) => {
                if let Screen::Dashboard(dashboard) = &mut self.screen {
                    return dashboard.update(message);
                }
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<Message, Theme, Renderer> {
        match &self.screen {
            Screen::Login(login) => login.view().map(Message::Login),
            Screen::Dashboard(dashboard) => {
                let body = dashboard.view().map(Message::Dashboard);
                if let Some(popup) = self.popups.last() {
                    let popup = W::center(W::column![
                        W::container(W::row![
                            W::horizontal_space().width(iced::Length::Fill),
                            W::button("X").on_press(Message::ClosePopup)
                        ])
                        .style(|_| W::container::background(iced::color!(0x212121))),
                        W::container(popup.view()).height(Length::Fill)
                    ])
                    .padding(20)
                    .style(W::container::rounded_box);
                    popup.into()
                } else {
                    body
                }
            }
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        match &self.screen {
            Screen::Login(_) => Subscription::none(),
            Screen::Dashboard(dashboard) => dashboard.subscription(),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("the_man=trace".parse().unwrap()),
        )
        .init();

    let app = iced::application("TheMan", TheMan::update, TheMan::view)
        .subscription(TheMan::subscription);
    app.run_with(|| (TheMan::new(), Task::none())).unwrap();

    Ok(())

    // let endpoint = iroh::Endpoint::builder()
    //     .discovery_n0()
    //     .discovery_dht()
    //     .bind()
    //     .await?;

    // let gossip = iroh_gossip::net::Gossip::builder()
    //     .spawn(endpoint.clone())
    //     .await?;

    // let local_pool = iroh_blobs::util::local_pool::LocalPool::default();

    // let blobs = iroh_blobs::net_protocol::Blobs::memory().build(local_pool.handle(), &endpoint);

    // let the_man = protocol::TheMan::spawn(
    //     gossip.clone(),
    //     blobs.clone(),
    //     endpoint.clone(),
    //     local_pool.handle(),
    // )
    // .await;

    // info!("Create node");
    // let node = iroh::protocol::Router::builder(endpoint)
    //     .accept(iroh_gossip::ALPN, gossip.clone())
    //     .accept(iroh_blobs::ALPN, blobs.clone())
    //     .accept(protocol::ALPN, the_man.clone())
    //     .spawn()
    //     .await?;

    // println!(
    //     "NodeId: {}",
    //     base64_serialize(&node.endpoint().node_id()).unwrap()
    // );

    // let mut buffer = [0; 1024];
    // let mut str = String::default();

    // let mut stdin = tokio::io::stdin();
    // loop {
    //     select! {
    //         Ok(len) = stdin.read(&mut buffer) => {
    //             if handle_cli(len, &mut str, &mut buffer, &node, &the_man).await {
    //                 break;
    //             }
    //         }
    //     }
    // }

    // node.shutdown().await?;

    // Ok(())
}

async fn handle_cli(
    len: usize,
    str: &mut String,
    buffer: &mut [u8; 1024],
    node: &Router,
    the_man: &protocol::TheMan<iroh_blobs::store::mem::Store>,
) -> bool {
    str.push_str(&String::from_utf8_lossy(&buffer[0..len]));
    if let Some(i) = str.find('\n') {
        let data = str.drain(0..=i).collect::<Box<str>>();
        let data = data.trim_end();
        let (command, next) = if let Some((command, next)) = data.split_once(' ') {
            (command, Some(next))
        } else {
            (data, None)
        };

        match command {
            "stop" => return true,
            "connect" => {
                let Some(next) = next else {
                    println!("`connect` needs the node hash!");
                    return false;
                };

                let Ok(node_id) = base64_deserialize::<NodeId>(next) else {
                    println!("Cannot parse `node_id`");
                    return false;
                };

                println!("Connecting to: {next}");

                the_man.connect(node_id).await;
            }
            "info" => {
                for info in node.endpoint().remote_info_iter() {
                    println!("{info:#?}");
                }
            }
            "create" => {
                let Some(next) = next else {
                    println!("`create` needs the nodes hashs!");
                    return false;
                };

                let mut nodes = Vec::default();
                for (i, res) in next
                    .split(' ')
                    .map(base64_deserialize::<NodeId>)
                    .enumerate()
                {
                    if let Ok(node_id) = res {
                        nodes.push(node_id);
                    } else {
                        println!("Cannot parse node_id at position: {i}");
                    }
                }

                nodes.push(node.endpoint().node_id());

                let Some(handle) = the_man.create(RawConversation::new(nodes)).await else {
                    eprintln!("Cannot create conversation");
                    return false;
                };

                println!(
                    "Conversation ID: {}",
                    base64_serialize(handle.hash()).unwrap()
                );
            }
            "list" => {
                let Some(next) = next else {
                    println!("Conversations:");
                    for conversation in the_man.conversations().await.unwrap() {
                        let conversation = conversation.get().await;
                        println!(
                            "Ticket: {}",
                            base64_serialize(&conversation.ticket).unwrap()
                        );
                        println!(
                            "\tId: {}",
                            base64_serialize(&conversation.ticket.hash()).unwrap()
                        );
                        println!(
                            "\tNodes: {}",
                            conversation
                                .raw
                                .nodes
                                .iter()
                                .fold(String::default(), |mut acc, peer| {
                                    acc.push_str(&base64_serialize(peer).unwrap());
                                    acc.push(' ');
                                    acc
                                })
                                .trim_end()
                        );
                    }
                    return false;
                };

                let Ok(conversation_id) = base64_deserialize::<iroh_blobs::Hash>(next) else {
                    println!("Cannot parse conversation id");
                    return false;
                };

                let Some(conversation) = the_man.get_conversation(conversation_id).await else {
                    println!("Cannot find conversation");
                    return false;
                };

                println!("Messages:");
                for message in conversation.messages().await {
                    println!("Ticket: {}", base64_serialize(&message.ticket).unwrap());
                    println!(
                        "\tFrom: {}",
                        base64_serialize(&message.ticket.owner_id).unwrap()
                    );
                    println!(
                        "\tTime: {} : {}",
                        message.raw.time.format("%d/%m/%Y %H:%M"),
                        message.raw.data
                    )
                }
            }
            "send" => {
                let Some(next) = next else {
                    println!("`send` needs the conversation and the message");
                    return false;
                };

                let Some((conversation_id, next)) = next.split_once(' ') else {
                    println!("`send` needs the conversation and the message");
                    return false;
                };

                let Ok(conversation_id) = base64_deserialize::<iroh_blobs::Hash>(&conversation_id)
                else {
                    println!("Cannot parse conversation id");
                    return false;
                };

                let Some(conversation) = the_man.get_conversation(conversation_id).await else {
                    println!("Cannot get conversation");
                    return false;
                };

                conversation.send(next).await;
            }
            "recover" => {
                let Some(next) = next else {
                    println!("`reciver` needs a message ticket");
                    return false;
                };

                let Ok(ticket) = base64_deserialize::<Ticket>(next) else {
                    println!("Cannot parse conversation id");
                    return false;
                };

                the_man.recover(ticket).await;
            }
            _ => println!("Invalid command: {command}"),
        }
    }

    false
}

pub fn base64_serialize<T: Serialize>(value: &T) -> bincode::Result<String> {
    let value = bincode::serialize(value)?;
    Ok(BASE64_STANDARD_NO_PAD.encode(value))
}

#[derive(Debug)]
pub enum Base64DecodeError {
    Bincode(bincode::Error),
    Base64(base64::DecodeError),
}

impl From<base64::DecodeError> for Base64DecodeError {
    fn from(value: base64::DecodeError) -> Self {
        Self::Base64(value)
    }
}

impl From<bincode::Error> for Base64DecodeError {
    fn from(value: bincode::Error) -> Self {
        Self::Bincode(value)
    }
}

pub fn base64_deserialize<T: DeserializeOwned>(
    value: impl AsRef<[u8]>,
) -> Result<T, Base64DecodeError> {
    let bytes = BASE64_STANDARD_NO_PAD.decode(value)?;
    Ok(bincode::deserialize::<T>(&bytes)?)
}

pub trait DynClone {
    fn clone_box(&self) -> *mut Infallible;
}

impl<T: Clone> DynClone for T {
    fn clone_box(&self) -> *mut Infallible {
        Box::into_raw(Box::new(self.clone())) as *mut _
    }
}

pub trait TPopup<Message>: Send + Sync + DynClone {
    fn update(&mut self, message: Message) -> Task<Message>;
    fn view(&self) -> Element<Message, Theme, Renderer>;
    fn finish(&self) -> Option<Message>;
}

impl<Message> TPopup<Message> for Infallible {
    fn update(&mut self, _message: Message) -> Task<Message> {
        unreachable!()
    }

    fn view(&self) -> Element<Message, Theme, Renderer> {
        unreachable!()
    }

    fn finish(&self) -> Option<Message> {
        unreachable!()
    }
}

impl<M> Clone for Box<dyn TPopup<M>> {
    fn clone(&self) -> Self {
        unsafe { Box::from_raw(self.clone_box() as *mut _) }
    }
}

#[derive(Clone)]
pub struct Popup<M>(Box<dyn TPopup<M> + 'static>);

impl<M: Clone + MaybeSend + Sync + 'static> Popup<M> {
    pub fn new(popup: impl TPopup<M> + 'static) -> Self {
        Self(Box::new(popup))
    }

    pub fn map<T: Clone + MaybeSend + Sync + 'static>(
        self,
        from_to: impl MapFunction<M, T> + 'static,
        to_from: impl MapFunction<T, M> + 'static,
    ) -> Popup<T> {
        Popup(Box::new(MapPopup {
            popup: self,
            map_from_to: Arc::new(from_to),
            map_to_from: Arc::new(to_from),
        }))
    }
}

impl<M: Clone> TPopup<M> for Popup<M> {
    fn update(&mut self, message: M) -> Task<M> {
        self.0.update(message)
    }

    fn view(&self) -> Element<M, Theme, Renderer> {
        self.0.view()
    }

    fn finish(&self) -> Option<M> {
        self.0.finish()
    }
}

pub trait MapFunction<FROM: MaybeSend + Sync, TO: MaybeSend + Sync>:
    Fn(FROM) -> TO + MaybeSend + Sync
{
}

impl<FROM, TO, T: Fn(FROM) -> TO> MapFunction<FROM, TO> for T
where
    FROM: MaybeSend + Sync + 'static,
    TO: MaybeSend + Sync + 'static,
    T: MaybeSend + Sync + 'static,
{
}

#[derive(Clone)]
pub struct MapPopup<
    TO: MaybeSend + Sync + Clone + 'static,
    FROM: MaybeSend + Sync + Clone + 'static,
> {
    popup: Popup<FROM>,
    map_from_to: Arc<dyn MapFunction<FROM, TO>>,
    map_to_from: Arc<dyn MapFunction<TO, FROM>>,
}

impl<TO: MaybeSend + Sync + Clone + 'static, FROM: MaybeSend + Sync + Clone + 'static> TPopup<TO>
    for MapPopup<TO, FROM>
{
    fn update(&mut self, message: TO) -> Task<TO> {
        let f = (self.map_to_from)(message);
        let map_from_to = self.map_from_to.clone();
        self.popup.update(f).map(move |m| (map_from_to)(m))
    }

    fn view(&self) -> Element<TO, Theme, Renderer> {
        self.popup.view().map(|m| (self.map_from_to)(m))
    }

    fn finish(&self) -> Option<TO> {
        self.popup.0.finish().map(|m| (self.map_from_to)(m))
    }
}
