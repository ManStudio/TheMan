use std::{collections::BTreeMap, pin::Pin, sync::Arc};

use eframe::egui;
use iroh::NodeId;
use serde::Serialize;
use the_man::{base64_deserialize, base64_serialize, protocol};
use tracing::info;

use crate::{Account, Data};

pub struct StateLogin {
    loading: bool,
    name: String,
    secret: String,
}

impl StateLogin {
    fn run(&mut self, ctx: &egui::Context, data: &mut Data, context: &mut Context) {
        egui::TopBottomPanel::top("login_top_panel").show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                ui.heading("Accounts:");
            });
        });
        egui::TopBottomPanel::bottom("login_bottom_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.name);
                egui::TextEdit::singleline(&mut self.secret)
                    .password(true)
                    .show(ui);
                if ui.button("Add").clicked() {
                    let secret = if self.secret.is_empty() {
                        iroh::SecretKey::generate(rand::rngs::OsRng)
                    } else if let Ok(secret) = base64_deserialize::<iroh::SecretKey>(&self.secret) {
                        secret
                    } else {
                        ui.colored_label(egui::Color32::from_rgb(255, 0, 0), "Cannot parse secret");
                        return;
                    };

                    data.accounts.push(Account {
                        name: std::mem::take(&mut self.name),
                        secret,
                        known_as: BTreeMap::default(),
                    });
                    self.secret.clear();
                    data.save();
                }
            })
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.loading {
                ui.spinner();
                return;
            }

            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| {
                    for (i, account) in data.accounts.iter().enumerate() {
                        if ui.button(&account.name).clicked() {
                            let name = account.name.clone();
                            let secret = account.secret.clone();
                            context.add_task(async move {
                                let the_man = the_man::TheMan::new(name.clone(), secret).await;

                                let subscription_messages = the_man.subscribe_messages();

                                Event::SetState(State::Dashboard(StateDashboard {
                                    account_id: i,
                                    the_man,
                                    subscription_messages,
                                    conversations: vec![],
                                    popups: Vec::default(),
                                    new_node: String::default(),
                                    conversation: None,
                                }))
                            });
                            self.loading = true;
                        }
                    }
                });
        });
    }
}

struct DPopupCreateConversation {
    nodes: Vec<NodeId>,
}

impl DPopupCreateConversation {
    fn run(&mut self, ui: &mut egui::Ui, data: &mut Data) -> Option<EventDashboard> {
        let mut res = None;
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.heading("Known");
                egui::ScrollArea::vertical().id_salt("left").show(ui, |ui| {
                    for node in data.knows_node_id.iter() {
                        if ui
                            .button(base64_serialize(node).unwrap().to_string())
                            .clicked()
                        {
                            self.nodes.push(*node);
                        }
                    }
                });
            });
            ui.separator();
            ui.vertical(|ui| {
                ui.heading("In Conversation");
                egui::ScrollArea::vertical()
                    .id_salt("right")
                    .show(ui, |ui| {
                        for node in self.nodes.iter() {
                            ui.label(base64_serialize(node).unwrap().to_string());
                        }
                    });
            });
        });
        ui.separator();
        if ui.button("Create").clicked() {
            res = Some(EventDashboard::CreateConversation(self.nodes.clone()))
        }
        res
    }

    fn name(&self) -> String {
        "Create Conversation".into()
    }
}

struct DPopupRecover {
    ticket_text: String,
}

impl DPopupRecover {
    fn run(&mut self, ui: &mut egui::Ui, data: &mut Data) -> Option<EventDashboard> {
        let mut res = None;

        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut self.ticket_text);
            if ui.button("Recover").clicked() {
                if let Ok(ticket) = base64_deserialize(&mut self.ticket_text) {
                    res = Some(EventDashboard::Recover(ticket))
                }
            }
        });

        res
    }

    fn name(&self) -> String {
        "Recover".into()
    }
}

enum DPopup {
    CreateConversation(DPopupCreateConversation),
    Recover(DPopupRecover),
}

impl DPopup {
    fn run(&mut self, ui: &mut egui::Ui, data: &mut Data) -> Option<EventDashboard> {
        match self {
            DPopup::CreateConversation(v) => v.run(ui, data),
            DPopup::Recover(v) => v.run(ui, data),
        }
    }

    fn name(&self) -> String {
        match self {
            DPopup::CreateConversation(v) => v.name(),
            DPopup::Recover(v) => v.name(),
        }
    }
}

struct Tail {
    messages: Vec<(Arc<protocol::TreeEntry>, Option<protocol::Message>)>,
}

struct Conversation {
    tails: Vec<Tail>,
    handle: protocol::ConversationHandle<iroh_blobs::store::fs::Store>,
    message: String,
    selected: Option<iroh_blobs::Hash>,
    recording: bool,
}

struct StateDashboard {
    account_id: usize,
    the_man: the_man::TheMan,
    subscription_messages: tokio::sync::watch::Receiver<Option<protocol::Message>>,

    conversations: Vec<iroh_blobs::Hash>,
    popups: Vec<DPopup>,
    new_node: String,
    conversation: Option<Conversation>,
}

async fn dashboard_get_message(the_man: the_man::TheMan, hash: iroh_blobs::Hash) -> Event {
    let message = the_man.get_message(hash).await.unwrap();
    Event::Dashboard(EventDashboard::SetMessage(message))
}

async fn dashboard_recv_message(
    mut subscription_messages: tokio::sync::watch::Receiver<Option<protocol::Message>>,
) -> Event {
    let mut i = 0;
    let message = subscription_messages
        .wait_for(|o| {
            let res = o.is_some() && i != 0;
            i += 1;
            res
        })
        .await
        .unwrap()
        .clone()
        .unwrap();
    Event::Dashboard(EventDashboard::ReceivedMessage(
        subscription_messages,
        message,
    ))
}

const MAX_MESSAGES: usize = 100;

impl StateDashboard {
    fn run(&mut self, ctx: &egui::Context, data: &mut Data, context: &mut Context) {
        {
            let mut events = Vec::default();
            self.popups.retain_mut(|popup| {
                let mut res = true;
                let mut open = true;
                egui::Window::new(format!("{} Popup", popup.name()))
                    .open(&mut open)
                    .show(ctx, |ui| {
                        if let Some(event) = popup.run(ui, data) {
                            events.push(event);
                            res = false;
                        }
                    });

                if !open {
                    return false;
                }

                res
            });

            for event in events {
                self.update(context, event);
            }
        }
        egui::SidePanel::left("known-peers-panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.new_node);
                if ui.button("Add").clicked() {
                    if let Ok(node_id) = base64_deserialize::<NodeId>(&self.new_node) {
                        *data = Data::load();
                        data.knows_node_id.push(node_id);
                        data.save();
                        self.new_node.clear();
                    }
                }
            });
            egui::ScrollArea::both()
                .auto_shrink(false)
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                .show(ui, |ui| {
                    ui.label("Known peers");
                    ui.vertical(|ui| {
                        for known in data.knows_node_id.iter() {
                            ui.label(base64_serialize(known).unwrap().to_string());
                        }
                    })
                });
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Conversations");
            ui.horizontal(|ui| {
                if ui.button("Refresh").clicked() {
                    let the_man = self.the_man.clone();
                    context.add_task(async move {
                        Event::Dashboard(EventDashboard::SetConversations(
                            the_man.raw_conversations().await.unwrap(),
                        ))
                    });
                }

                if ui.button("Create").clicked() {
                    self.popups
                        .push(DPopup::CreateConversation(DPopupCreateConversation {
                            nodes: vec![],
                        }));
                }

                if ui.button("Recover").clicked() {
                    self.popups.push(DPopup::Recover(DPopupRecover {
                        ticket_text: String::default(),
                    }));
                }
            });
            egui::TopBottomPanel::bottom("status-panel").show_inside(ui, |ui| {
                ui.heading("Status");
                if ui.button(&data.accounts[self.account_id].name).clicked() {
                    ui.ctx()
                        .copy_text(base64_serialize(&self.the_man.node_id()).unwrap());
                }
            });
            egui::ScrollArea::both()
                .auto_shrink(false)
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                .show(ui, |ui| {
                    for conversation in self.conversations.iter() {
                        if ui
                            .button(base64_serialize(conversation).unwrap().to_string())
                            .clicked()
                        {
                            let the_man = self.the_man.clone();
                            let hash = *conversation;
                            context.add_task(async move {
                                let conversation = the_man.get_conversation(hash).await.unwrap();
                                Event::Dashboard(EventDashboard::SetConversation(conversation))
                            });
                        }
                    }
                });
        });

        if let Some(conversation) = &mut self.conversation {
            egui::SidePanel::right("conversation-panel").show(ctx, |ui| {
                ui.heading("Conversation");
                egui::ScrollArea::horizontal()
                    .auto_shrink(false)
                    .max_height(ui.available_size().y - 30.)
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                    .show(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            for (tail_id, tail) in conversation.tails.iter_mut().enumerate() {
                                egui::ScrollArea::vertical()
                                    .id_salt(tail_id)
                                    .auto_shrink(egui::Vec2b::new(true, false))
                                    .stick_to_bottom(true)
                                    .show(ui, |ui| {
                                        ui.vertical(|ui| {
                                            if sensor(ui) {
                                                if let Some(first) = tail.messages.first() {
                                                    if let Some(prev) = first.0.blocking_prev() {
                                                        context.add_task(dashboard_get_message(
                                                            self.the_man.clone(),
                                                            prev.hash(),
                                                        ));
                                                        tail.messages.insert(0, (prev, None));
                                                        if tail.messages.len() > MAX_MESSAGES {
                                                            tail.messages.drain(MAX_MESSAGES..);
                                                        }
                                                        ui.label("Fetching up...");
                                                    }
                                                }
                                            }

                                            let mut last: Option<NodeId> = None;
                                            for (entry, message) in tail.messages.iter() {
                                                let mut selected = conversation
                                                    .selected
                                                    .map(|hash| hash == entry.hash())
                                                    .unwrap_or(false);
                                                let last_selected = selected;
                                                ui.horizontal(|ui| {
                                                    ui.checkbox(&mut selected, "");
                                                    ui.group(|ui| {
                                                        if let Some(message) = message {
                                                            ui.vertical(|ui| {
                                                                if last
                                                                    != Some(message.ticket.owner_id)
                                                                {
                                                                    ui.horizontal(|ui| {
                                                                        ui.label("From: ");
                                                                        nameble_info(
                                                                            ui,
                                                                            data,
                                                                            self.account_id,
                                                                            &message
                                                                                .ticket
                                                                                .owner_id,
                                                                        );
                                                                    });
                                                                    last = Some(
                                                                        message.ticket.owner_id,
                                                                    );
                                                                }
                                                                ui.label(&message.raw.data)
                                                                    .context_menu(|ui| {
                                                                        if ui
                                                                            .small_button(
                                                                                "Copy token",
                                                                            )
                                                                            .clicked()
                                                                        {
                                                                            ui.ctx().copy_text(
                                                                                base64_serialize(
                                                                                    &message.ticket,
                                                                                )
                                                                                .unwrap(),
                                                                            );
                                                                        }
                                                                    });
                                                            });
                                                        } else {
                                                            ui.horizontal(|ui| {
                                                                ui.spinner();
                                                                ui.label(
                                                                    base64_serialize(&entry.hash())
                                                                        .unwrap()
                                                                        .to_string(),
                                                                );
                                                            });
                                                        }
                                                    })
                                                    .response
                                                });

                                                if last_selected != selected {
                                                    conversation.selected =
                                                        selected.then_some(entry.hash());
                                                }
                                            }

                                            if sensor(ui) {
                                                if let Some(last) = tail.messages.last() {
                                                    if let Some(next) = last.0.blocking_next() {
                                                        context.add_task(dashboard_get_message(
                                                            self.the_man.clone(),
                                                            next.hash(),
                                                        ));
                                                        tail.messages.push((next, None));
                                                        if tail.messages.len() > MAX_MESSAGES {
                                                            tail.messages.drain(
                                                                ..tail.messages.len()
                                                                    - MAX_MESSAGES,
                                                            );
                                                        }
                                                        ui.label("Fetching down...");
                                                    }
                                                }
                                            }
                                        });
                                    });
                                ui.separator();
                            }
                        });
                    });
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut conversation.message);
                    if ui.button("Send").clicked() {
                        let last = conversation.selected;
                        let the_man = self.the_man.clone();
                        let data = std::mem::take(&mut conversation.message);
                        let conversation = *conversation.handle.hash();
                        context.add_task(async move {
                            let last = if let Some(last) = last {
                                Some(the_man.get_message(last).await.unwrap().ticket)
                            } else {
                                None
                            };
                            let conversation =
                                the_man.get_conversation(conversation).await.unwrap();
                            let hash = the_man
                                .send_message(protocol::RawMessage {
                                    last,
                                    time: chrono::Utc::now(),
                                    conversation: conversation.ticket().await,
                                    data,
                                })
                                .await
                                .unwrap();

                            Event::Dashboard(EventDashboard::SelectMessage(hash))
                        });
                    }

                    if !conversation.recording {
                        if ui.button("Start Stream").clicked() {
                            let the_man = self.the_man.clone();
                            let hash = *conversation.handle.hash();
                            conversation.recording = true;
                            context.add_task(async move {
                                the_man.add_conversation_default_stream(hash).await;
                                Event::None
                            });
                        }
                    } else if ui.button("Stop Stream").clicked() {
                        let the_man = self.the_man.clone();
                        let hash = *conversation.handle.hash();
                        conversation.recording = false;
                        context.add_task(async move {
                            the_man.stop_conversation_default_stream(hash).await;
                            Event::None
                        });
                    }
                });
            });
        }
    }

    fn update(&mut self, context: &mut Context, event: EventDashboard) {
        match event {
            EventDashboard::SetConversations(conversations) => {
                self.conversations = conversations;
            }
            EventDashboard::CreateConversation(mut nodes) => {
                let the_man = self.the_man.clone();
                context.add_task(async move {
                    nodes.insert(0, the_man.node_id());
                    the_man
                        .create(protocol::RawConversation {
                            nodes,
                            time: chrono::Utc::now(),
                        })
                        .await;
                    Event::Dashboard(EventDashboard::CreatedConversation)
                });
            }
            EventDashboard::CreatedConversation => {
                let the_man = self.the_man.clone();
                context.add_task(async move {
                    Event::Dashboard(EventDashboard::SetConversations(
                        the_man.raw_conversations().await.unwrap(),
                    ))
                });
            }
            EventDashboard::Recover(ticket) => {
                let the_man = self.the_man.clone();
                context.add_task(async move {
                    the_man.recover(ticket).await;

                    Event::Dashboard(EventDashboard::SetConversations(
                        the_man.raw_conversations().await.unwrap(),
                    ))
                });
            }
            EventDashboard::ReceivedMessage(receiver, message) => {
                info!("Recv message: {message:?}");
                context.add_task(dashboard_recv_message(receiver));
                if let Some(conversation) = &self.conversation {
                    let handle = conversation.handle.clone();
                    context.add_task(async move {
                        Event::Dashboard(EventDashboard::SetTails(handle.get().await.tails))
                    });
                }
            }
            EventDashboard::SetTails(tails) => {
                let Some(conversation) = &mut self.conversation else {
                    return;
                };
                for new_tail in &tails[conversation.tails.len()..] {
                    let hash = new_tail.hash();
                    {
                        let the_man = self.the_man.clone();
                        context.add_task(async move {
                            let message = the_man.get_message(hash).await.unwrap();

                            Event::Dashboard(EventDashboard::SetMessage(message))
                        });
                    }
                    conversation.tails.push(Tail {
                        messages: vec![(new_tail.clone(), None)],
                    });
                }
            }
            EventDashboard::SetMessage(message) => {
                let Some(conversation) = &mut self.conversation else {
                    return;
                };

                for tail in conversation.tails.iter_mut() {
                    for (entry, message_o) in tail.messages.iter_mut() {
                        if entry.hash() == message.ticket.hash() {
                            _ = message_o.insert(message.clone());
                        }
                    }
                }
            }
            EventDashboard::SetConversation(handle) => {
                {
                    let handle = handle.clone();
                    context.add_task(async move {
                        Event::Dashboard(EventDashboard::SetTails(handle.get().await.tails))
                    });
                }
                let _ = self.conversation.insert(Conversation {
                    tails: Vec::default(),
                    handle,
                    message: String::default(),
                    selected: None,
                    recording: false,
                });
            }
            EventDashboard::SelectMessage(hash) => {
                let Some(conversation) = &mut self.conversation else {
                    return;
                };

                _ = conversation.selected.insert(hash);
            }
        }
    }
}

enum State {
    Login(StateLogin),
    Dashboard(StateDashboard),
}

impl State {
    fn run(&mut self, ctx: &egui::Context, data: &mut Data, context: &mut Context) {
        match self {
            State::Login(state_login) => state_login.run(ctx, data, context),
            State::Dashboard(state_dashboard) => state_dashboard.run(ctx, data, context),
        }
    }
}

pub struct App {
    data: Data,
    state: State,
    context: Context,
}

enum EventDashboard {
    SetConversations(Vec<iroh_blobs::Hash>),
    CreateConversation(Vec<NodeId>),
    CreatedConversation,
    SetConversation(protocol::ConversationHandle<iroh_blobs::store::fs::Store>),
    Recover(protocol::Ticket),
    ReceivedMessage(
        tokio::sync::watch::Receiver<Option<protocol::Message>>,
        protocol::Message,
    ),
    SetTails(Vec<Arc<protocol::TreeEntry>>),
    SetMessage(protocol::Message),
    SelectMessage(iroh_blobs::Hash),
}

enum Event {
    None,
    SetState(State),
    Dashboard(EventDashboard),
}

struct Context {
    receiver: tokio::sync::mpsc::Receiver<Event>,
    task_sender:
        tokio::sync::mpsc::Sender<Pin<Box<dyn std::future::Future<Output = Event> + Send>>>,
    worker: tokio::task::JoinHandle<()>,
}

impl Context {
    fn add_task(&self, task: impl std::future::Future<Output = Event> + Send + 'static) {
        self.task_sender.blocking_send(Box::pin(task)).unwrap();
    }
}

fn sensor(ui: &mut egui::Ui) -> bool {
    let rect = ui
        .allocate_response(egui::Vec2 { x: 0., y: 0. }, egui::Sense::empty())
        .rect;
    ui.is_rect_visible(rect)
}

impl App {
    pub fn new(ctx: &egui::Context, data: Data) -> Self {
        let (e_sender, receiver) = tokio::sync::mpsc::channel(8);
        let (task_sender, task_receiver) = tokio::sync::mpsc::channel(8);

        let ctx = ctx.clone();

        Self {
            state: State::Login(StateLogin {
                loading: false,
                name: String::default(),
                secret: String::default(),
            }),
            data,
            context: Context {
                receiver,
                task_sender,
                worker: tokio::spawn(async move {
                    let ctx = ctx;
                    let e_sender = e_sender;
                    let mut task_receiver = task_receiver;
                    let mut tasks = Vec::new();
                    loop {
                        let mut remove_task = None;
                        tokio::select! {
                            Some(task) = task_receiver.recv() => {
                                tasks.push(task)
                            }

                            (event, i, _) = async{if !tasks.is_empty() {
                                        futures_util::future::select_all(tasks.iter_mut()).await
                                    } else {
                                        std::future::pending().await
                                    }} => {
                                    remove_task = Some(i);
                                e_sender.send(event).await.unwrap();
                                ctx.request_repaint();
                            }
                        };

                        if let Some(idx) = remove_task {
                            _ = tasks.swap_remove(idx);
                        }
                    }
                }),
            },
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        while let Ok(event) = self.context.receiver.try_recv() {
            match event {
                Event::None => {}
                Event::SetState(state) => {
                    if let State::Dashboard(dashboard) = &state {
                        self.context.add_task(dashboard_recv_message(
                            dashboard.subscription_messages.clone(),
                        ));
                    };
                    self.state = state;
                }
                Event::Dashboard(event_dashboard) => {
                    if let State::Dashboard(dashboard) = &mut self.state {
                        dashboard.update(&mut self.context, event_dashboard)
                    }
                }
            }
        }

        self.state.run(ctx, &mut self.data, &mut self.context);
    }
}

fn nameble_info<Value: ToHash + Serialize>(
    ui: &mut egui::Ui,
    data: &mut Data,
    account_id: usize,
    info: &Value,
) {
    let hash = iroh_blobs::Hash::from(info.hash().as_bytes());
    if let Some(name) = data.accounts[account_id].known_as.get(&hash) {
        ui.label(name).context_menu(|ui| {
            if ui.small_button("Copy serialized").clicked() {
                ui.ctx()
                    .copy_text(base64_serialize(info).unwrap().to_string());
            }
        });
        return;
    }
    ui.label(base64_serialize(info).unwrap().to_string())
        .context_menu(|ui| {
            ui.small_button("Set name");
        });
}

trait ToHash {
    fn hash(&self) -> blake3::Hash;
}

impl ToHash for iroh::PublicKey {
    fn hash(&self) -> blake3::Hash {
        blake3::hash(self.as_bytes())
    }
}

impl ToHash for iroh_blobs::Hash {
    fn hash(&self) -> blake3::Hash {
        blake3::Hash::from_bytes(*self.as_bytes())
    }
}
