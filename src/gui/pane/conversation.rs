use std::sync::Arc;

use iroh_blobs::Hash;
use the_man::{
    base64_deserialize, base64_serialize,
    protocol::{Message, RawMessage, TreeEntry},
};
use tokio::sync::oneshot;
use tracing::{error, info};

use crate::gui::{component::with_name, popup};

use super::Pane;

use eframe::egui;

pub enum Entry {
    Loading(Arc<TreeEntry>),
    Node {
        message: Message,
        tree_entry: Arc<TreeEntry>,
    },
}

const MAX_PER_TAIL: usize = 100;

impl Entry {
    pub fn get_hash(&self) -> Hash {
        match self {
            Entry::Loading(tree_entry) => tree_entry.hash(),
            Entry::Node { tree_entry, .. } => tree_entry.hash(),
        }
    }
    pub fn tree_entry(&self) -> Arc<TreeEntry> {
        match self {
            Entry::Loading(tree_entry) => tree_entry.clone(),
            Entry::Node { tree_entry, .. } => tree_entry.clone(),
        }
    }
}

pub struct PaneConversation {
    conversation_id: Option<Hash>,
    message: String,
    selected: Option<Hash>,
    conversation_refreshes: usize,

    tails: Vec<Vec<Entry>>,

    conversation: Option<the_man::protocol::Conversation>,
    receive_conversation: Option<oneshot::Receiver<the_man::protocol::Conversation>>,

    o_receiver_tails: Option<oneshot::Receiver<Vec<Arc<TreeEntry>>>>,
    receivers_messages: Vec<oneshot::Receiver<Option<Message>>>,

    ttl: u16,
}

impl Default for PaneConversation {
    fn default() -> Self {
        Self {
            conversation_id: Default::default(),
            message: Default::default(),
            selected: Default::default(),
            conversation_refreshes: Default::default(),
            tails: Default::default(),
            conversation: Default::default(),
            receive_conversation: Default::default(),
            o_receiver_tails: Default::default(),
            receivers_messages: Default::default(),
            ttl: 10,
        }
    }
}

impl Pane for PaneConversation {
    fn name(&self, account: &crate::Account) -> String {
        let Some(conversation) = &self.conversation_id else {
            return String::from("C: NOT SET");
        };

        format!(
            "C: {}",
            account
                .known_as
                .get(conversation)
                .cloned()
                .unwrap_or_else(|| base64_serialize(conversation).unwrap()),
        )
    }

    fn closable(&self) -> bool {
        true
    }

    fn ui(
        &mut self,
        ui: &mut eframe::egui::Ui,
        context: &mut crate::gui::Context,
        the_man: &mut the_man::TheMan,
        account: &mut crate::Account,
    ) {
        let Some(conversation_id) = &self.conversation_id else {
            return;
        };

        if let Some(mut receiver_conversation) = self.receive_conversation.take() {
            if let Ok(conversation) = receiver_conversation.try_recv() {
                self.conversation = Some(conversation);
            } else {
                self.receive_conversation = Some(receiver_conversation);
                return;
            }
        }

        let Some(conversation) = &self.conversation else {
            let conversation_id = *conversation_id;
            let (sender, receiver) = tokio::sync::oneshot::channel();
            let the_man = the_man.clone();
            context.add_task(Box::pin(async move {
                let handle = the_man
                    .get_conversation(conversation_id)
                    .await
                    .expect("Cannot get conversation");
                _ = sender.send(handle.get().await);
            }));
            self.receive_conversation = Some(receiver);

            return;
        };

        if let Some(mut receiver_tails) = self.o_receiver_tails.take() {
            if let Ok(tails) = receiver_tails.try_recv() {
                for tail in tails.into_iter().skip(self.tails.len()) {
                    self.tails.push(vec![Entry::Loading(tail.clone())]);

                    let (sender, receiver) = oneshot::channel();
                    context.add_task(Box::pin(task_get_message(
                        the_man.clone(),
                        tail.hash(),
                        sender,
                    )));

                    self.receivers_messages.push(receiver);
                }
            } else {
                self.o_receiver_tails = Some(receiver_tails);
            }
        }

        if self.o_receiver_tails.is_none()
            && context.conversation_refreshes != self.conversation_refreshes
        {
            info!("Refresh tails");
            self.conversation_refreshes = context.conversation_refreshes;
            let (sender, receiver) = oneshot::channel();

            let the_man = the_man.clone();
            let hash = *conversation_id;
            context.add_task(Box::pin(async move {
                let tails = the_man
                    .get_conversation(hash)
                    .await
                    .expect("Cannot find conversation")
                    .get()
                    .await
                    .tails;
                _ = sender.send(tails);
            }));

            self.o_receiver_tails = Some(receiver);
        }

        for mut receiver_message in std::mem::take(&mut self.receivers_messages) {
            let Ok(result) = receiver_message.try_recv() else {
                self.receivers_messages.push(receiver_message);
                continue;
            };

            let Some(result) = result else {
                error!("Message lost");
                continue;
            };

            for tail in self.tails.iter_mut() {
                for msg in tail {
                    if msg.get_hash() == result.ticket.hash() {
                        *msg = Entry::Node {
                            message: result.clone(),
                            tree_entry: msg.tree_entry(),
                        }
                    }
                }
            }
        }

        ui.horizontal(|ui| {
            if ui.button("Set Conversation Name").clicked() {
                context.add_popup(popup::PopupSetName::new(
                    *conversation_id,
                    account
                        .known_as
                        .get(conversation_id)
                        .cloned()
                        .unwrap_or_default(),
                ));
            };
        });

        ui.separator();

        egui::Frame::group(ui.style())
            .inner_margin(4)
            .outer_margin(4)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let scroll_width = (ui.available_size_before_wrap().x / 2.) - 6.0;
                    ui.vertical(|ui| {
                        ui.heading("OUT");

                        egui::ScrollArea::horizontal()
                            .max_width(scroll_width)
                            .id_salt("OUT")
                            .auto_shrink(false)
                            .scroll_bar_visibility(
                                egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                            )
                            .show(ui, |ui| {
                                ui.set_max_size(egui::Vec2::INFINITY);
                                ui.horizontal(|ui| {
                                    for node_id in conversation.raw.nodes.iter() {
                                        ui.vertical(|ui| {
                                            with_name(node_id, ui, context, account, |_|{});
                                            egui::ScrollArea::vertical()
                                            .max_width(scroll_width)
                                            .id_salt(format!("OUT {node_id}"))
                                            .auto_shrink([true, false])
                                            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                                            .show(ui, |ui| {
                                                tokio::runtime::Handle::current().block_on(async {
                                                    for idx in the_man.conversation_outputs(*conversation_id, *node_id).await{
                                                        ui.label(format!("{idx}"));
                                                    }
                                                });

                                            });
                                        });
                                        ui.separator();
                                    }
                                })
                            });
                    });
                    ui.separator();
                    ui.vertical(|ui| {
                        ui.heading("IN");

                        ui.add(egui::DragValue::new(&mut self.ttl).prefix("TTL: ").suffix("s").range(0..=u16::MAX).speed(1));

                        if ui
                            .add_enabled(self.selected.is_some(), egui::Button::new("Create Input"))
                            .on_disabled_hover_text(
                                "You need to select the message where that input will reply.",
                            )
                            .clicked()
                        {
                            let the_man = the_man.clone();
                            let conversation = *conversation_id;
                            let msg_hash = self.selected.unwrap();
                            let ttl = self.ttl;
                            context.add_task(Box::pin(async move {
                                the_man
                                    .conversation_create_input(conversation, msg_hash, ttl)
                                    .await;
                            }));
                        }

                        egui::ScrollArea::vertical()
                            .max_width(scroll_width)
                            .id_salt("IN")
                            .auto_shrink(false)
                            .scroll_bar_visibility(
                                egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                            )
                            .show(ui, |ui| {
                                tokio::runtime::Handle::current().block_on(async {
                                    for idx in the_man.conversation_inputs(*conversation_id ).await{
                                        if ui.button(format!("{idx}")).clicked(){
                                            the_man.conversation_stop_input(*conversation_id, idx).await;
                                        }
                                    }
                                });
                            });
                    });
                });
            });
        ui.separator();

        let size = ui.available_size_before_wrap();
        ui.horizontal(|ui| {
            ui.horizontal(|ui| {
                let send = egui::Resize::default().max_size(size).show(ui, |ui| {
                    if ui
                        .add_sized(
                            ui.available_size_before_wrap(),
                            egui::TextEdit::multiline(&mut self.message).hint_text("Message"),
                        )
                        .has_focus()
                        && ui
                            .ctx()
                            .input(|i| (!i.modifiers.shift) && i.key_pressed(egui::Key::Enter))
                    {
                        return true;
                    }
                    false
                });

                if send {
                    let the_man = the_man.clone();
                    let hash_conversation = *conversation_id;
                    let last = self.selected;
                    let data = std::mem::take(&mut self.message).trim().to_string();
                    context.add_task(Box::pin(async move {
                        let conversation =
                            the_man.get_conversation(hash_conversation).await.unwrap();
                        let last = if let Some(last) = last {
                            Some(the_man.get_message(last).await.unwrap().ticket)
                        } else {
                            None
                        };
                        the_man
                            .send_message(RawMessage {
                                last,
                                time: chrono::Utc::now(),
                                conversation: conversation.ticket().await,
                                data,
                            })
                            .await;
                    }));
                }
            });
        });

        egui::Frame::group(ui.style())
            .inner_margin(4)
            .outer_margin(4)
            .show(ui, |ui| {
                egui::ScrollArea::horizontal()
                    .auto_shrink(false)
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                    .show(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            for (i, tail) in self.tails.iter_mut().enumerate() {
                                egui::ScrollArea::vertical()
                                    .id_salt(i)
                                    .auto_shrink([true, false])
                                    .stick_to_bottom(true)
                                    .scroll_bar_visibility(
                                        egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                                    )
                                    .show(ui, |ui| {
                                        ui.set_max_size(egui::Vec2::INFINITY);
                                        ui.vertical(|ui| {
                                            if sensor(ui) {
                                                let first = tail.first().unwrap();
                                                if let Some(prev) =
                                                    first.tree_entry().blocking_prev()
                                                {
                                                    ui.label("Getting the previous!");
                                                    let (sender, receiver) = oneshot::channel();
                                                    context.add_task(Box::pin(task_get_message(
                                                        the_man.clone(),
                                                        prev.hash(),
                                                        sender,
                                                    )));
                                                    tail.insert(0, Entry::Loading(prev));
                                                    if tail.len() > MAX_PER_TAIL {
                                                        tail.drain(MAX_PER_TAIL..);
                                                    }

                                                    self.receivers_messages.push(receiver);
                                                }
                                            }

                                            for msg in tail.iter_mut() {
                                                let mut selected = self
                                                    .selected
                                                    .map(|hash| hash == msg.get_hash())
                                                    .unwrap_or(false);
                                                let last_selected = selected;

                                                ui.group(|ui| {
                                                    ui.checkbox(&mut selected, "");

                                                    let mut to_delete = false;

                                                    match msg {
                                                        Entry::Loading(tree_entry) => {
                                                            ui.label("spinner").context_menu(
                                                                |ui| {
                                                                    if ui
                                                                        .small_button(
                                                                            "Copy message hash",
                                                                        )
                                                                        .clicked()
                                                                    {
                                                                        ui.ctx().copy_text(
                                                                            base64_serialize(
                                                                                &tree_entry.hash(),
                                                                            )
                                                                            .unwrap(),
                                                                        );
                                                                        ui.close_menu();
                                                                    }
                                                                },
                                                            );
                                                        }
                                                        Entry::Node {
                                                            message,
                                                            ..
                                                        } => {
                                                            ui.horizontal(|ui|{
                                                                ui.colored_label(egui::Color32::YELLOW, "From:");
                                                                with_name(&message.ticket.owner_id, ui, context, account, |_| {});
                                                            });

                                                            ui.label(&message.raw.data)
                                                                .context_menu(|ui| {
                                                                    if ui
                                                                        .small_button("Copy Ticket")
                                                                        .clicked()
                                                                    {
                                                                        ui.ctx().copy_text(
                                                                            base64_serialize(
                                                                                &message.ticket,
                                                                            )
                                                                            .unwrap(),
                                                                        );
                                                                        ui.close_menu();
                                                                    }
                                                                    if ui.small_button("Delete").clicked(){
                                                                        let the_man = the_man.clone();
                                                                        let hash = message.ticket.hash();
                                                                        context.add_task(Box::pin(async move {
                                                                            the_man.message_set_ttl(hash, 1).await;
                                                                        }));

                                                                        to_delete = true;

                                                                        ui.close_menu();
                                                                    }
                                                                    if ui.small_button("Remove TTL").clicked(){
                                                                        let the_man = the_man.clone();
                                                                        let hash = message.ticket.hash();
                                                                        context.add_task(Box::pin(async move {
                                                                            the_man.message_set_ttl(hash, 0).await;
                                                                        }));

                                                                        ui.close_menu();
                                                                    }
                                                                });
                                                        }
                                                    }

                                                    if to_delete{
                                                        *msg = Entry::Loading(msg.tree_entry());
                                                    }
                                                });

                                                if selected != last_selected {
                                                    if selected {
                                                        self.selected = Some(msg.get_hash());
                                                    } else if last_selected && !selected {
                                                        self.selected = None;
                                                    }
                                                }
                                            }

                                            if sensor(ui) {
                                                let last = tail.last().unwrap();
                                                if let Some(next) =
                                                    last.tree_entry().blocking_next()
                                                {
                                                    ui.label("Getting next");
                                                    let (sender, receiver) = oneshot::channel();
                                                    context.add_task(Box::pin(task_get_message(
                                                        the_man.clone(),
                                                        next.hash(),
                                                        sender,
                                                    )));
                                                    tail.push(Entry::Loading(next));
                                                    if tail.len() > MAX_PER_TAIL {
                                                        tail.drain(..tail.len() - MAX_PER_TAIL);
                                                    }

                                                    self.receivers_messages.push(receiver);
                                                }
                                            }
                                        });
                                    });
                                ui.separator();
                            }
                        });
                    });
            });
    }

    fn set_data(&mut self, data: String) {
        if let Ok(conversation) = base64_deserialize::<Hash>(data) {
            self.conversation_id = Some(conversation);
            return;
        }
        error!("Cannot parse data");
    }

    fn get_data(&self) -> String {
        let Some(conversation) = &self.conversation_id else {
            return String::default();
        };

        base64_serialize(conversation).unwrap()
    }
}

async fn task_get_message(
    the_man: the_man::TheMan,
    hash: Hash,
    sender: oneshot::Sender<Option<Message>>,
) {
    _ = sender.send(the_man.get_message(hash).await);
}

fn sensor(ui: &mut egui::Ui) -> bool {
    let rect = ui
        .allocate_response(egui::Vec2 { x: 0., y: 0. }, egui::Sense::empty())
        .rect;
    ui.is_rect_visible(rect)
}
