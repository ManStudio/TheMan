use std::sync::Arc;

use iced::{widget as W, Element, Renderer, Theme};
use iced::{Subscription, Task};
use iroh::{protocol::Router, NodeId};
use iroh_blobs::Hash;
use tracing::{error, info};

use crate::protocol::RawMessage;
use crate::{base64_deserialize, base64_serialize, protocol, Popup};
use crate::{Message as TMessage, TPopup};

#[derive(Clone)]
pub struct PopupCreateConversation {
    nodes: Vec<NodeId>,
    input: String,
    finished: Option<Vec<NodeId>>,
}

#[derive(Debug, Clone)]
pub enum CreateConversationMessage {
    Add,
    SetInput(String),
    Finished(Vec<NodeId>),
    Create,
}

impl TPopup<CreateConversationMessage> for PopupCreateConversation {
    fn update(&mut self, message: CreateConversationMessage) -> Task<CreateConversationMessage> {
        match message {
            CreateConversationMessage::Add => {
                let input = std::mem::take(&mut self.input);
                if let Ok(node_id) = base64_deserialize::<NodeId>(input) {
                    self.nodes.push(node_id);
                    self.nodes.sort();
                    self.nodes.dedup();
                } else {
                    error!("Invalid node_id");
                }
            }
            CreateConversationMessage::SetInput(input) => self.input = input,
            CreateConversationMessage::Create => {
                self.finished = Some(self.nodes.clone());
            }
            CreateConversationMessage::Finished(_) => unreachable!(),
        }
        Task::none()
    }

    fn view(&self) -> Element<CreateConversationMessage, Theme, Renderer> {
        W::column![
            W::column(
                self.nodes
                    .iter()
                    .map(|node_id| Element::from(W::button(W::text(
                        base64_serialize(node_id).unwrap()
                    ))))
            ),
            W::vertical_space(),
            W::row![
                W::text_input("NodeId", &self.input)
                    .on_input(CreateConversationMessage::SetInput)
                    .on_submit(CreateConversationMessage::Add),
                W::button("Add").on_press(CreateConversationMessage::Add)
            ],
            W::button("Create").on_press_maybe(
                (!self.nodes.is_empty()).then_some(CreateConversationMessage::Create)
            )
        ]
        .into()
    }

    fn finish(&self) -> Option<CreateConversationMessage> {
        if let Some(finished) = &self.finished {
            Some(CreateConversationMessage::Finished(finished.clone()))
        } else {
            None
        }
    }
}

#[derive(Clone)]
pub struct PopupRecoverConversation {
    tickets: Vec<protocol::Ticket>,
    input: String,
    finished: Option<Vec<protocol::Ticket>>,
}

impl TPopup<RecoverConversationMessage> for PopupRecoverConversation {
    fn update(&mut self, message: RecoverConversationMessage) -> Task<RecoverConversationMessage> {
        match message {
            RecoverConversationMessage::Add => {
                let input = std::mem::take(&mut self.input);
                if let Ok(ticket) = base64_deserialize::<protocol::Ticket>(input) {
                    self.tickets.push(ticket);
                } else {
                    error!("Invalid node_id");
                }
            }
            RecoverConversationMessage::SetInput(input) => self.input = input,
            RecoverConversationMessage::Create => {
                self.finished = Some(self.tickets.clone());
            }
            RecoverConversationMessage::Finished(_) => unreachable!(),
        }
        Task::none()
    }

    fn view(&self) -> Element<RecoverConversationMessage, Theme, Renderer> {
        W::column![
            W::column(
                self.tickets
                    .iter()
                    .map(|node_id| Element::from(W::button(W::text(
                        base64_serialize(node_id).unwrap()
                    ))))
            ),
            W::vertical_space(),
            W::row![
                W::text_input("Message Ticket", &self.input)
                    .on_input(RecoverConversationMessage::SetInput)
                    .on_submit(RecoverConversationMessage::Add),
                W::button("Add").on_press(RecoverConversationMessage::Add)
            ],
            W::button("Recover").on_press_maybe(
                (!self.tickets.is_empty()).then_some(RecoverConversationMessage::Create)
            )
        ]
        .into()
    }

    fn finish(&self) -> Option<RecoverConversationMessage> {
        if let Some(finished) = &self.finished {
            Some(RecoverConversationMessage::Finished(finished.clone()))
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub enum RecoverConversationMessage {
    Add,
    SetInput(String),
    Finished(Vec<protocol::Ticket>),
    Create,
}

pub enum Msg {
    Waiting(protocol::Ticket),
    Some(protocol::Message),
}

pub struct Tail {
    messages: Vec<Msg>,
    input: String,
}

pub struct Conversation {
    raw: protocol::Conversation,
    tails: Vec<Tail>,
    input: String,
}

impl Clone for Conversation {
    fn clone(&self) -> Self {
        Self {
            tails: Vec::default(),
            raw: todo!(),
            input: todo!(),
        }
    }
}

impl std::fmt::Debug for Conversation {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    CopyNodeId(NodeId),
    CopyTicket(protocol::Ticket),
    PopupCreateConversation(CreateConversationMessage),
    PopupRecoverConversation(RecoverConversationMessage),
    CreateConversation,
    RecoverConversation,
    GetConversations,
    SetConversations(Vec<Hash>),
    SetConversation(Hash),
    SetMessage(protocol::Message),
    RecvConversation(Conversation),
    SetTailInput(usize, String),
    SetInput(String),
    SendTail(usize),
    Send,
    AddMessage(protocol::Message),
}

#[derive(Debug, Clone)]
pub struct Dashboard {
    pub name: String,
    pub node: Router,
    pub local_pool: Arc<iroh_blobs::util::local_pool::LocalPool>,
    pub the_man: protocol::TheMan<iroh_blobs::store::mem::Store>,
    pub conversation: Option<Conversation>,
    pub conversations: Vec<Hash>,
    pub message_receiver: tokio::sync::watch::Receiver<Option<protocol::Message>>,
}

impl Dashboard {
    pub fn new(
        name: String,
        node: Router,
        local_pool: iroh_blobs::util::local_pool::LocalPool,
        the_man: protocol::TheMan<iroh_blobs::store::mem::Store>,
        message_receiver: tokio::sync::watch::Receiver<Option<protocol::Message>>,
    ) -> Self {
        Self {
            name,
            node,
            local_pool: Arc::new(local_pool),
            the_man,
            conversation: None,
            conversations: Vec::default(),
            message_receiver,
        }
    }

    pub fn update(&mut self, message: Message) -> Task<TMessage> {
        match message {
            Message::CopyNodeId(node_id) => {
                iced::clipboard::write(base64_serialize(&node_id).unwrap())
            }
            Message::CopyTicket(ticket) => {
                iced::clipboard::write(base64_serialize(&ticket).unwrap())
            }
            Message::CreateConversation => Task::done(TMessage::CreatePopup({
                Popup::new(PopupCreateConversation {
                    nodes: Vec::default(),
                    input: String::default(),
                    finished: None,
                })
                .map::<TMessage>(
                    |m| TMessage::Dashboard(Message::PopupCreateConversation(m)),
                    |m| {
                        if let TMessage::Dashboard(Message::PopupCreateConversation(m)) = m {
                            m
                        } else {
                            panic!()
                        }
                    },
                )
            })),
            Message::RecoverConversation => Task::done(TMessage::CreatePopup({
                Popup::new(PopupRecoverConversation {
                    tickets: Vec::default(),
                    input: String::default(),
                    finished: None,
                })
                .map::<TMessage>(
                    |m| TMessage::Dashboard(Message::PopupRecoverConversation(m)),
                    |m| {
                        if let TMessage::Dashboard(Message::PopupRecoverConversation(m)) = m {
                            m
                        } else {
                            panic!()
                        }
                    },
                )
            })),
            Message::GetConversations => {
                let the_man = self.the_man.clone();
                Task::perform(
                    async move { the_man.raw_conversations().await },
                    |conversations| Message::SetConversations(conversations.unwrap()),
                )
                .map(TMessage::Dashboard)
            }
            Message::SetConversations(conversations) => {
                self.conversations = conversations;
                Task::none()
            }
            Message::PopupCreateConversation(CreateConversationMessage::Finished(nodes_id)) => {
                let node_id = self.node.endpoint().node_id();
                let the_man = self.the_man.clone();
                Task::perform(
                    async move {
                        let mut nodes_id = nodes_id;
                        nodes_id.push(node_id);
                        let res = the_man
                            .create(protocol::RawConversation {
                                nodes: nodes_id,
                                time: chrono::Utc::now(),
                            })
                            .await;

                        dbg!(res.is_some());

                        the_man.raw_conversations().await.unwrap()
                    },
                    Message::SetConversations,
                )
                .map(TMessage::Dashboard)
            }

            Message::PopupRecoverConversation(RecoverConversationMessage::Finished(tickets)) => {
                let mut tasks = Vec::default();
                for ticket in tickets {
                    let the_man = self.the_man.clone();
                    tasks.push(Task::perform(
                        async move {
                            the_man.recover(ticket.clone()).await;
                            let message = the_man.get_message(ticket.hash()).await.unwrap();
                            let conversation = the_man
                                .get_conversation(message.raw.conversation.hash())
                                .await
                                .unwrap()
                                .get()
                                .await;
                            Conversation {
                                tails: conversation
                                    .tails
                                    .iter()
                                    .cloned()
                                    .map(|ticket| Tail {
                                        messages: vec![Msg::Waiting(ticket)],
                                        input: String::default(),
                                    })
                                    .collect::<Vec<_>>(),
                                raw: conversation,
                                input: String::default(),
                            }
                        },
                        Message::RecvConversation,
                    ));
                }

                Task::batch(tasks).map(TMessage::Dashboard)
            }
            Message::SetConversation(hash) => {
                let the_man = self.the_man.clone();
                Task::perform(
                    async move {
                        let conversation = the_man
                            .get_conversation(hash)
                            .await
                            .expect("Cannot get conversation!");

                        let conversation = conversation.get().await;
                        Conversation {
                            tails: conversation
                                .tails
                                .iter()
                                .cloned()
                                .map(|ticket| Tail {
                                    messages: vec![Msg::Waiting(ticket)],
                                    input: String::default(),
                                })
                                .collect::<Vec<_>>(),
                            raw: conversation,
                            input: String::default(),
                        }
                    },
                    Message::RecvConversation,
                )
                .map(TMessage::Dashboard)
            }
            Message::RecvConversation(conversation) => {
                let mut tasks = Vec::new();
                for tail in conversation.tails.iter() {
                    for msg in tail.messages.iter() {
                        let Msg::Waiting(ticket) = msg else {
                            continue;
                        };
                        let ticket = ticket.clone();
                        let the_man = self.the_man.clone();
                        tasks.push(Task::perform(
                            async move {
                                the_man
                                    .get_message(ticket.hash())
                                    .await
                                    .expect("Cannot get message!")
                            },
                            Message::SetMessage,
                        ))
                    }
                }

                self.conversation = Some(conversation);

                Task::batch(tasks).map(TMessage::Dashboard)
            }
            Message::SetMessage(raw) => {
                let Some(conversation) = &mut self.conversation else {
                    return Task::none();
                };

                for tail in conversation.tails.iter_mut() {
                    for msg in tail.messages.iter_mut() {
                        let Msg::Waiting(ticket) = &msg else {
                            continue;
                        };

                        if ticket.hash() != raw.ticket.hash() {
                            continue;
                        }

                        let ticket = raw.raw.last.clone();

                        *msg = Msg::Some(raw);

                        return if let Some(ticket) = ticket {
                            tail.messages.insert(0, Msg::Waiting(ticket.clone()));
                            let the_man = self.the_man.clone();
                            Task::perform(
                                async move {
                                    the_man
                                        .get_message(ticket.hash())
                                        .await
                                        .expect("Cannot get message")
                                },
                                Message::SetMessage,
                            )
                            .map(TMessage::Dashboard)
                        } else {
                            Task::none()
                        };
                    }
                }

                Task::none()
            }
            Message::SetTailInput(index, input) => {
                let Some(conversation) = &mut self.conversation else {
                    return Task::none();
                };

                conversation.tails[index].input = input;
                Task::none()
            }
            Message::SetInput(input) => {
                let Some(conversation) = &mut self.conversation else {
                    return Task::none();
                };
                conversation.input = input;
                Task::none()
            }
            Message::SendTail(index) => {
                let Some(conversation) = &mut self.conversation else {
                    return Task::none();
                };
                let Msg::Some(last) = conversation.tails[index]
                    .messages
                    .last()
                    .expect("No Messages")
                else {
                    error!("Cannot respond to a message we don't have.");
                    return Task::none();
                };

                let the_man = self.the_man.clone();
                let ticket = last.ticket.clone();
                let text = std::mem::take(&mut conversation.tails[index].input);
                let conversation_ticket = conversation.raw.ticket.clone();

                Task::perform(
                    async move {
                        let hash = the_man
                            .send_message(RawMessage {
                                last: Some(ticket),
                                time: chrono::Utc::now(),
                                conversation: conversation_ticket,
                                data: text,
                            })
                            .await
                            .expect("Cannot send message!");

                        the_man
                            .get_message(hash)
                            .await
                            .expect("Cannot get message!")
                    },
                    Message::AddMessage,
                )
                .map(TMessage::Dashboard)
            }
            Message::Send => {
                let Some(conversation) = &mut self.conversation else {
                    return Task::none();
                };

                let the_man = self.the_man.clone();
                let text = std::mem::take(&mut conversation.input);
                let conversation_ticket = conversation.raw.ticket.clone();

                Task::perform(
                    async move {
                        let hash = the_man
                            .send_message(RawMessage {
                                last: None,
                                time: chrono::Utc::now(),
                                conversation: conversation_ticket,
                                data: text,
                            })
                            .await
                            .expect("Cannot send message!");

                        the_man
                            .get_message(hash)
                            .await
                            .expect("Cannot get message!")
                    },
                    Message::AddMessage,
                )
                .map(TMessage::Dashboard)
            }
            Message::AddMessage(message) => {
                let Some(conversation) = &mut self.conversation else {
                    return Task::none();
                };

                if let Some(last) = message.raw.last.clone() {
                    for tail in conversation.tails.iter_mut() {
                        for (i, msg) in tail.messages.iter_mut().enumerate() {
                            let Msg::Some(msg) = &msg else {
                                continue;
                            };
                            if msg.ticket.hash() != last.hash() {
                                continue;
                            }
                            tail.messages.insert(i + 1, Msg::Some(message));
                            return Task::none();
                        }
                    }
                } else {
                    conversation.tails.push(Tail {
                        messages: vec![Msg::Some(message)],
                        input: String::default(),
                    });
                }
                Task::none()
            }
            _ => todo!(),
        }
    }

    pub fn view(&self) -> Element<Message, Theme, Renderer> {
        let conversations = W::container(W::column![
            W::container(W::scrollable(W::column(self.conversations.iter().map(
                |conversation| {
                    Element::from(
                        W::button(W::text(base64_serialize(conversation).unwrap()))
                            .on_press(Message::SetConversation(*conversation)),
                    )
                }
            ))))
            .style(W::container::bordered_box),
            W::vertical_space(),
            W::button("Refresh").on_press(Message::GetConversations),
            W::button("Create").on_press(Message::CreateConversation),
            W::button("Recover").on_press(Message::RecoverConversation),
            W::container(
                W::button(W::text(&self.name))
                    .on_press(Message::CopyNodeId(self.node.endpoint().node_id()))
            )
            .style(W::container::bordered_box)
        ])
        .style(W::container::bordered_box);
        let chat = W::container('d: {
            let Some(conversation) = &self.conversation else {
                break 'd Element::from(W::row![]);
            };

            Element::from(W::column([
                Element::from(W::row(conversation.tails.iter().enumerate().map(
                    |(i, tail)| {
                        let body = Element::from(W::column(tail.messages.iter().map(|msg| {
                            match msg {
                                Msg::Waiting(ticket) => Element::from(W::text(format!(
                                    "Waiting: {}",
                                    base64_serialize(ticket).unwrap()
                                ))),
                                Msg::Some(message) => Element::from(
                                    W::button(W::text(&message.raw.data))
                                        .on_press(Message::CopyTicket(message.ticket.clone())),
                                ),
                            }
                        })));

                        Element::from(W::column![
                            body,
                            W::vertical_space(),
                            W::text_input("Reply Message", &tail.input)
                                .on_input(move |input| Message::SetTailInput(i, input))
                                .on_submit(Message::SendTail(i))
                        ])
                    },
                ))),
                Element::from(
                    W::text_input("New Message", &conversation.input)
                        .on_input(Message::SetInput)
                        .on_submit(Message::Send),
                ),
            ]))
        })
        .style(W::container::bordered_box);
        let body = W::row![conversations, chat];
        Element::from(body)
    }

    pub fn subscription(&self) -> Subscription<TMessage> {
        #[derive(Hash)]
        pub struct RecvMessages;

        iced::advanced::subscription::from_recipe(ListenForMessages {
            id: RecvMessages,
            receiver: self.message_receiver.clone(),
        })
        .map(Message::AddMessage)
        .map(TMessage::Dashboard)
    }
}

pub struct ListenForMessages<ID: std::hash::Hash> {
    id: ID,
    receiver: tokio::sync::watch::Receiver<Option<protocol::Message>>,
}

impl<ID: std::hash::Hash> iced::advanced::subscription::Recipe for ListenForMessages<ID> {
    type Output = protocol::Message;

    fn hash(&self, state: &mut iced::advanced::subscription::Hasher) {
        self.id.hash(state)
    }

    fn stream(
        self: Box<Self>,
        _: iced::advanced::subscription::EventStream,
    ) -> iced::advanced::graphics::futures::BoxStream<Self::Output> {
        Box::pin(futures_lite::stream::unfold(
            self.receiver,
            |receiver| async move {
                let mut receiver = receiver;
                let mut i = 0;
                let msg = receiver
                    .wait_for(|t| {
                        if i != 0 {
                            return t.is_some();
                        }

                        i += 1;
                        false
                    })
                    .await
                    .expect("Cannot recv message!");
                let message = msg.as_ref().unwrap().clone();
                drop(msg);
                Some((message, receiver))
            },
        ))
    }
}
