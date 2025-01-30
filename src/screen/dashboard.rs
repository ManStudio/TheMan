use std::sync::Arc;

use iced::advanced::graphics::futures::MaybeSend;
use iced::{widget as W, Element, Renderer, Theme};
use iced::{Subscription, Task};
use iroh::{protocol::Router, NodeId};
use iroh_blobs::Hash;
use serde::de::DeserializeOwned;
use serde::Serialize;
use the_man::{base64_deserialize, base64_serialize, TheMan};
use tracing::error;

use crate::protocol::{RawMessage, Ticket};
use crate::{protocol, Popup, ViewSensor};
use crate::{Message as TMessage, TPopup};

#[derive(Clone)]
pub struct PopupGetList<
    T: std::fmt::Debug + Clone + DeserializeOwned + Serialize + MaybeSend + Sync,
> {
    placeholder: String,
    submit: String,
    list: Vec<T>,
    input: String,
    finished: Option<Vec<T>>,
}

#[derive(Debug, Clone)]
pub enum GetListMessage<
    T: std::fmt::Debug + Clone + DeserializeOwned + Serialize + MaybeSend + Sync,
> {
    Add,
    SetInput(String),
    Finished(Vec<T>),
    Finish,
}

impl<T: std::fmt::Debug + Clone + DeserializeOwned + Serialize + MaybeSend + Sync>
    TPopup<GetListMessage<T>> for PopupGetList<T>
{
    fn update(&mut self, message: GetListMessage<T>) -> Task<GetListMessage<T>> {
        match message {
            GetListMessage::Add => {
                let input = std::mem::take(&mut self.input);
                if let Ok(item) = base64_deserialize::<T>(input) {
                    self.list.push(item);
                } else {
                    error!("Cannot deserialize");
                }
            }
            GetListMessage::SetInput(input) => self.input = input,
            GetListMessage::Finish => {
                self.finished = Some(self.list.clone());
            }
            GetListMessage::Finished(_) => unreachable!(),
        }
        Task::none()
    }

    fn view(&self) -> Element<GetListMessage<T>, Theme, Renderer> {
        W::column![
            W::column(
                self.list
                    .iter()
                    .map(|node_id| Element::from(W::button(W::text(
                        base64_serialize(node_id).unwrap()
                    ))))
            ),
            W::vertical_space(),
            W::row![
                W::text_input(&self.placeholder, &self.input)
                    .on_input(GetListMessage::SetInput)
                    .on_submit(GetListMessage::Add),
                W::button("Add").on_press(GetListMessage::Add)
            ],
            W::button(self.submit.as_str())
                .on_press_maybe((!self.list.is_empty()).then_some(GetListMessage::Finish))
        ]
        .into()
    }

    fn finish(&self) -> Option<GetListMessage<T>> {
        self.finished
            .as_ref()
            .map(|finished| GetListMessage::Finished(finished.clone()))
    }
}

pub enum Msg {
    Waiting(protocol::Ticket),
    Some(protocol::Message),
}

impl Msg {
    pub fn hash(&self) -> Hash {
        match self {
            Msg::Waiting(ticket) => ticket.hash(),
            Msg::Some(message) => message.ticket.hash(),
        }
    }
}

pub struct Tail {
    messages: Vec<Msg>,
}

pub struct Conversation {
    raw: protocol::Conversation,
    tails: Vec<Tail>,
    input: String,
    selected: Option<Ticket>,
}

impl Clone for Conversation {
    fn clone(&self) -> Self {
        todo!()
    }
}

impl std::fmt::Debug for Conversation {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    None,
    CopyNodeId(NodeId),
    CopyTicket(protocol::Ticket),
    PopupCreateConversation(GetListMessage<NodeId>),
    PopupRecoverConversation(GetListMessage<Ticket>),
    CreateConversation,
    RecoverConversation,
    GetConversations,
    SetConversations(Vec<Hash>),
    SetConversation(Hash),
    Set(protocol::Message),
    RecvConversation(Conversation),
    SetInput(String),
    Send,
    Add(protocol::Message),
    Select(Ticket),
    AddDefaultStream,
    AddedDefaultStream,
    LoadTail(usize),
}

#[derive(Debug, Clone)]
pub struct Dashboard {
    pub name: String,
    pub the_man: TheMan,
    pub conversation: Option<Conversation>,
    pub conversations: Vec<Hash>,
    pub message_receiver: tokio::sync::watch::Receiver<Option<protocol::Message>>,
}

impl Dashboard {
    pub fn new(
        name: String,
        the_man: TheMan,
        message_receiver: tokio::sync::watch::Receiver<Option<protocol::Message>>,
    ) -> Self {
        Self {
            name,
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
                Popup::new(PopupGetList::<NodeId> {
                    placeholder: "NodeId".to_owned(),
                    submit: "Create".to_owned(),
                    list: Vec::default(),
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
                Popup::new(PopupGetList::<Ticket> {
                    placeholder: "Message Ticket".to_owned(),
                    submit: "Recover".to_owned(),
                    list: Vec::default(),
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
            Message::PopupCreateConversation(GetListMessage::Finished(nodes_id)) => {
                let the_man = self.the_man.clone();
                Task::perform(
                    async move {
                        let mut nodes_id = nodes_id;
                        nodes_id.push(the_man.node_id());
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

            Message::PopupRecoverConversation(GetListMessage::<Ticket>::Finished(tickets)) => {
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
                                    })
                                    .collect::<Vec<_>>(),
                                raw: conversation,
                                input: String::default(),
                                selected: None,
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
                                })
                                .collect::<Vec<_>>(),
                            raw: conversation,
                            input: String::default(),
                            selected: None,
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
                            Message::Set,
                        ))
                    }
                }

                self.conversation = Some(conversation);

                Task::batch(tasks).map(TMessage::Dashboard)
            }
            Message::Set(raw) => {
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

                        *msg = Msg::Some(raw);

                        return Task::none();
                    }
                }

                Task::none()
            }
            Message::SetInput(input) => {
                let Some(conversation) = &mut self.conversation else {
                    return Task::none();
                };
                conversation.input = input;
                Task::none()
            }
            Message::Select(hash) => {
                let Some(conversation) = &mut self.conversation else {
                    return Task::none();
                };
                if conversation
                    .selected
                    .as_ref()
                    .is_some_and(|h| h.hash() == hash.hash())
                {
                    conversation.selected = None;
                } else {
                    conversation.selected = Some(hash);
                }

                Task::none()
            }
            Message::Send => {
                let Some(conversation) = &mut self.conversation else {
                    return Task::none();
                };

                let the_man = self.the_man.clone();
                let text = std::mem::take(&mut conversation.input);
                let conversation_ticket = conversation.raw.ticket.clone();

                if let Some(ticket) = conversation.selected.clone() {
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
                                .expect("Cannot get message!");
                        },
                        |_| Message::None,
                    )
                } else {
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
                                .expect("Cannot get message!");
                        },
                        |_| Message::None,
                    )
                }
                .map(TMessage::Dashboard)
            }
            Message::Add(message) => {
                let Some(conversation) = &mut self.conversation else {
                    return Task::none();
                };

                if message.raw.conversation.hash() != conversation.raw.ticket.hash() {
                    return Task::none();
                }

                let mut add = None;

                if let Some(last) = message.raw.last.clone() {
                    for (i, tail) in conversation.tails.iter().enumerate() {
                        let tail_last = tail.messages.last().unwrap();
                        let Msg::Some(tail_last) = &tail_last else {
                            continue;
                        };
                        if tail_last.ticket.hash() != last.hash() {
                            continue;
                        }

                        add = Some(i);
                    }
                }

                if let Some(i) = add {
                    let tail = &mut conversation.tails[i];

                    if tail.messages.len() > 100 {
                        tail.messages.drain(..tail.messages.len() - 100);
                    }

                    tail.messages.push(Msg::Some(message));
                    W::scrollable::snap_to(
                        W::scrollable::Id::new(format!("scroll{i}")),
                        W::scrollable::RelativeOffset::END,
                    )
                } else {
                    conversation.tails.push(Tail {
                        messages: vec![Msg::Some(message)],
                    });
                    Task::none()
                }
            }
            Message::AddDefaultStream => {
                let Some(conversation) = &self.conversation else {
                    return Task::none();
                };

                let conversation_id = conversation.raw.ticket.hash();

                let the_man = self.the_man.clone();
                Task::perform(
                    async move {
                        the_man
                            .add_conversation_default_stream(conversation_id)
                            .await;
                    },
                    |_| Message::AddedDefaultStream,
                )
                .map(TMessage::Dashboard)
            }
            Message::AddedDefaultStream => Task::none(),
            Message::None => Task::none(),
            Message::LoadTail(tail_idx) => {
                let Some(conversation) = &mut self.conversation else {
                    return Task::none();
                };
                let Some(tail) = conversation.tails.get_mut(tail_idx) else {
                    return Task::none();
                };
                let first = tail.messages.first().unwrap();
                let Msg::Some(msg) = &first else {
                    return Task::none();
                };
                let Some(last) = &msg.raw.last else {
                    return Task::none();
                };
                let last = last.clone();

                tail.messages.insert(0, Msg::Waiting(last.clone()));

                let the_man = self.the_man.clone();
                Task::perform(
                    async move {
                        the_man
                            .get_message(last.hash())
                            .await
                            .expect("Cannot get message")
                    },
                    Message::Set,
                )
                .map(TMessage::Dashboard)
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
                    .on_press(Message::CopyNodeId(self.the_man.node_id()))
            )
            .style(W::container::bordered_box)
        ])
        .style(W::container::bordered_box);
        let chat = W::container('d: {
            let Some(conversation) = &self.conversation else {
                break 'd Element::from(W::row![]);
            };

            Element::from(W::column([
                Element::from(
                    W::scrollable(W::row(conversation.tails.iter().enumerate().map(
                        |(tail_idx, tail)| {
                            let body = Element::from(
                                W::scrollable(W::column(
                                    std::iter::once(Element::new(ViewSensor {
                                        on_in_view: Message::LoadTail(tail_idx),
                                    }))
                                    .chain(
                                        tail.messages.iter().map(|msg| match msg {
                                            Msg::Waiting(ticket) => {
                                                Element::from(W::text(format!(
                                                    "Waiting: {}",
                                                    base64_serialize(ticket).unwrap()
                                                )))
                                            }
                                            Msg::Some(message) => Element::from(W::row![
                                                W::checkbox(
                                                    "",
                                                    conversation.selected.as_ref().is_some_and(
                                                        |t| t.hash() == message.ticket.hash()
                                                    )
                                                )
                                                .on_toggle(|_| Message::Select(
                                                    message.ticket.clone()
                                                )),
                                                W::button(W::text(&message.raw.data)).on_press(
                                                    Message::CopyTicket(message.ticket.clone())
                                                ),
                                            ]),
                                        }),
                                    ),
                                ))
                                .id(W::scrollable::Id::new(format!("scroll{tail_idx}"))),
                            );

                            Element::from(body)
                        },
                    )))
                    .direction(W::scrollable::Direction::Horizontal(
                        W::scrollable::Scrollbar::new(),
                    ))
                    .height(iced::Length::Fill),
                ),
                Element::from(
                    W::text_input("New Message", &conversation.input)
                        .on_input(Message::SetInput)
                        .on_submit(Message::Send),
                ),
                Element::from(
                    W::button("Add Default Input Stream").on_press(Message::AddDefaultStream),
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
        .map(Message::Add)
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
