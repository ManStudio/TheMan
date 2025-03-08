use std::sync::Arc;

use iroh_blobs::Hash;
use the_man::{
    base64_deserialize, base64_serialize,
    protocol::{Message, RawMessage, TreeEntry},
};
use tokio::sync::oneshot;
use tracing::{error, info};

use crate::gui::ToHash;

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

#[derive(Default)]
pub struct PaneConversation {
    name: Option<String>,
    conversation: Option<Hash>,
    message: String,
    selected: Option<Hash>,
    conversation_refreshes: usize,

    tails: Vec<Vec<Entry>>,

    o_receiver_tails: Option<oneshot::Receiver<Vec<Arc<TreeEntry>>>>,
    receivers_messages: Vec<oneshot::Receiver<Message>>,
}

impl Pane for PaneConversation {
    fn name(&self) -> String {
        let Some(conversation) = &self.conversation else {
            return String::from("C: NOT SET");
        };

        format!(
            "C: {}",
            self.name
                .clone()
                .unwrap_or_else(|| base64_serialize(conversation).unwrap())
        )
    }

    fn ui(
        &mut self,
        ui: &mut eframe::egui::Ui,
        context: &mut crate::gui::Context,
        the_man: &mut the_man::TheMan,
        account: &mut crate::Account,
    ) {
        let Some(conversation) = &self.conversation else {
            return;
        };

        self.name = Some(
            account
                .known_as
                .get(conversation)
                .cloned()
                .unwrap_or_else(|| base64_serialize(conversation).unwrap()),
        );

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
            let hash = *conversation;
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

        let size = ui.available_size_before_wrap();
        ui.horizontal(|ui| {
            egui::Resize::default().max_size(size).show(ui, |ui| {
                ui.add_sized(
                    ui.available_size_before_wrap(),
                    egui::TextEdit::multiline(&mut self.message).hint_text("Message"),
                );
            });
            if ui.button("Send").clicked() {
                let the_man = the_man.clone();
                let hash_conversation = *conversation;
                let last = self.selected;
                let data = std::mem::take(&mut self.message);
                context.add_task(Box::pin(async move {
                    let conversation = the_man.get_conversation(hash_conversation).await.unwrap();
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

                                            for msg in tail.iter() {
                                                let mut selected = self
                                                    .selected
                                                    .map(|hash| hash == msg.get_hash())
                                                    .unwrap_or(false);
                                                let last_selected = selected;

                                                ui.group(|ui| {
                                                    ui.checkbox(&mut selected, "");
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
                                                                    }
                                                                },
                                                            );
                                                        }
                                                        Entry::Node {
                                                            message,
                                                            tree_entry,
                                                        } => {
                                                            let node_hash = ToHash::hash(
                                                                &message.ticket.owner_id,
                                                            );

                                                            ui.colored_label(
                                                                egui::Color32::YELLOW,
                                                                format!(
                                                                    "From: {}",
                                                                    account
                                                                        .known_as
                                                                        .get(&node_hash)
                                                                        .cloned()
                                                                        .unwrap_or_else(|| {
                                                                            base64_serialize(
                                                                                &node_hash,
                                                                            )
                                                                            .unwrap()
                                                                            .to_string()
                                                                        })
                                                                ),
                                                            );
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
                                                                    }
                                                                });
                                                        }
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
            self.conversation = Some(conversation);
            return;
        }
        error!("Cannot parse data");
    }

    fn get_data(&self) -> String {
        let Some(conversation) = &self.conversation else {
            return String::default();
        };

        base64_serialize(conversation).unwrap()
    }
}

async fn task_get_message(the_man: the_man::TheMan, hash: Hash, sender: oneshot::Sender<Message>) {
    let msg = the_man
        .get_message(hash)
        .await
        .expect("Cannot get the message");
    _ = sender.send(msg);
}

fn sensor(ui: &mut egui::Ui) -> bool {
    let rect = ui
        .allocate_response(egui::Vec2 { x: 0., y: 0. }, egui::Sense::empty())
        .rect;
    ui.is_rect_visible(rect)
}
