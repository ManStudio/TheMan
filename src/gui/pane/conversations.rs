use std::collections::{BTreeSet, HashSet};

use iroh::PublicKey;
use iroh_blobs::Hash;
use the_man::{
    base64_deserialize, base64_serialize,
    protocol::{RawConversation, Ticket},
};
use tokio::sync::oneshot;

use eframe::egui;
use tracing::info;

use crate::{gui::ToHash, Account};

use super::{Pane, PaneConversation};

#[derive(Default)]
pub struct PopupCreate {
    nodes: BTreeSet<PublicKey>,
}

#[derive(Default)]
pub struct PopupRecover {
    input: String,
    tickets: BTreeSet<Ticket>,
}

#[derive(Default)]
pub struct PaneConversations {
    conversations: Vec<Hash>,
    conversations_receiver: Option<oneshot::Receiver<Vec<Hash>>>,

    conversation_refreshes: usize,

    create: Option<PopupCreate>,
    recover: Option<PopupRecover>,

    set_name_for: Option<Hash>,
    input_name: String,
}

impl Pane for PaneConversations {
    fn name(&self, _account: &Account) -> String {
        String::from("Conversations")
    }

    fn ui(
        &mut self,
        ui: &mut eframe::egui::Ui,
        context: &mut crate::gui::Context,
        the_man: &mut the_man::TheMan,
        account: &mut crate::Account,
    ) {
        if let Some(mut receiver) = self.conversations_receiver.take() {
            if let Ok(conversations) = receiver.try_recv() {
                self.conversations = conversations;
            } else {
                self.conversations_receiver = Some(receiver);
            }
        }

        if self.conversations_receiver.is_none()
            && context.conversation_refreshes != self.conversation_refreshes
        {
            self.conversation_refreshes = context.conversation_refreshes;
            let (sender, receiver) = oneshot::channel();
            context.add_task(Box::pin(task_get_conversations(the_man.clone(), sender)));
            self.conversations_receiver = Some(receiver);
        }

        if let Some(mut create) = self.create.take() {
            let mut close = false;
            let mut open = true;
            egui::Window::new("Create Conversation")
                .open(&mut open)
                .show(ui.ctx(), |ui| {
                    if ui.button("Create").clicked() {
                        let raw = RawConversation {
                            nodes: create
                                .nodes
                                .iter()
                                .copied()
                                .chain([the_man.node_id()])
                                .collect::<Vec<_>>(),
                            time: chrono::Utc::now(),
                        };
                        let the_man = the_man.clone();
                        context.add_task(Box::pin(async move {
                            the_man.create(raw).await;
                        }));
                        self.conversation_refreshes = usize::MAX;
                        close = true;
                    }

                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for known in account.known_nodes.iter() {
                            let selected = create.nodes.contains(known);
                            if ui
                                .selectable_label(
                                    selected,
                                    account
                                        .known_as
                                        .get(&ToHash::hash(known))
                                        .cloned()
                                        .unwrap_or_else(|| base64_serialize(known).unwrap()),
                                )
                                .clicked()
                            {
                                if selected {
                                    create.nodes.remove(known);
                                } else {
                                    create.nodes.insert(*known);
                                }
                            }
                        }
                    });
                });

            if close {
                open = false;
            }

            if open {
                self.create = Some(create);
            }
        }

        if let Some(mut recover) = self.recover.take() {
            let mut close = false;
            let mut open = true;
            egui::Window::new("Recover Messages")
                .open(&mut open)
                .show(ui.ctx(), |ui| {
                    ui.horizontal(|ui| {
                        egui::TextEdit::singleline(&mut recover.input)
                            .hint_text("Ticket Message")
                            .show(ui);
                        match base64_deserialize::<Ticket>(&recover.input) {
                            Ok(ticket) => {
                                if ui.button("Add").clicked() {
                                    recover.input.clear();
                                    recover.tickets.insert(ticket);
                                }
                            }
                            Err(err) => {
                                ui.colored_label(
                                    egui::Color32::RED,
                                    format!("Invalid Ticket: {err:?}"),
                                );
                            }
                        }
                    });

                    if ui.button("Recover").clicked() {
                        let the_man = the_man.clone();
                        let tickets = recover.tickets.iter().cloned().collect::<Vec<_>>();

                        context.add_task(Box::pin(async move {
                            for ticket in tickets {
                                the_man.recover(ticket).await;
                            }
                        }));

                        close = true;
                    }
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        let mut to_remove = Vec::new();
                        for ticket in recover.tickets.iter() {
                            if ui
                                .button(base64_serialize(ticket).unwrap().to_string())
                                .clicked()
                            {
                                to_remove.push(ticket.clone());
                            }
                        }
                        for to_remove in to_remove {
                            recover.tickets.remove(&to_remove);
                        }
                    });
                });

            if close {
                open = false;
            }

            if open {
                self.recover = Some(recover);
            }
        }

        if let Some(hash) = self.set_name_for.take() {
            let mut close = false;
            let mut open = true;
            egui::Window::new(format!("Set Name for {}", base64_serialize(&hash).unwrap())).show(
                ui.ctx(),
                |ui| {
                    egui::TextEdit::singleline(&mut self.input_name)
                        .hint_text("Name")
                        .show(ui);
                    if ui.button("Set Name").clicked() {
                        account
                            .known_as
                            .insert(hash, std::mem::take(&mut self.input_name));
                        context.should_save = true;
                        close = true;
                    }
                },
            );

            if close {
                open = false;
            }

            if open {
                self.set_name_for = Some(hash);
            }
        }

        ui.horizontal(|ui| {
            if ui.button("Create").clicked() {
                self.create = Some(PopupCreate::default());
            }
            if ui.button("Recover").clicked() {
                self.recover = Some(PopupRecover::default());
            }
        });

        egui::ScrollArea::vertical().show(ui, |ui| {
            for conversation in self.conversations.iter() {
                let res = ui.button(if let Some(name) = account.known_as.get(conversation) {
                    name.clone()
                } else {
                    base64_serialize(conversation).unwrap()
                });

                res.context_menu(|ui| {
                    if let Some(name) = account.known_as.get(conversation) {
                        if ui.small_button("Rename").clicked() {
                            self.set_name_for = Some(*conversation);
                            self.input_name = name.clone();
                        }
                        if ui.small_button("Remove Name").clicked() {
                            account.known_as.remove(conversation);
                        }
                    } else {
                        if ui.small_button("Set Name").clicked() {
                            self.set_name_for = Some(*conversation);
                        }
                    }
                });

                if res.clicked() {
                    let mut tab = PaneConversation::default();
                    tab.set_data(base64_serialize(conversation).unwrap());
                    context.add_tab(tab);
                }
            }
        });
    }
}

async fn task_get_conversations(the_man: the_man::TheMan, sender: oneshot::Sender<Vec<Hash>>) {
    _ = sender.send(the_man.raw_conversations().await.unwrap());
}
