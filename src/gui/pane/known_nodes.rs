use crate::{gui::ToHash, Account};

use super::Pane;

use eframe::egui;
use iroh::NodeId;
use iroh_blobs::Hash;
use the_man::{base64_deserialize, base64_serialize};

#[derive(Default)]
pub struct PaneKnownNodes {
    add_node: bool,

    input_node: String,
    input_name: String,

    with_name: bool,

    set_name_for: Option<Hash>,
}

impl Pane for PaneKnownNodes {
    fn name(&self, _account: &Account) -> String {
        String::from("Known Nodes")
    }

    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        context: &mut crate::gui::Context,
        the_man: &mut the_man::TheMan,
        account: &mut crate::Account,
    ) {
        if self.add_node {
            let mut open = true;
            egui::Window::new("Add Node")
                .open(&mut open)
                .show(ui.ctx(), |ui| {
                    egui::TextEdit::singleline(&mut self.input_node)
                        .hint_text("Node")
                        .show(ui);
                    let node = base64_deserialize::<NodeId>(&self.input_node);
                    if let Err(err) = &node {
                        ui.colored_label(egui::Color32::RED, format!("Node Id: {err:?}"));
                    }
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.with_name, "With Name?");
                        if self.with_name {
                            egui::TextEdit::singleline(&mut self.input_name)
                                .hint_text("Name")
                                .show(ui);
                        }
                    });

                    if let Ok(node) = node {
                        if ui.button("Add").clicked() {
                            account.known_nodes.insert(node);
                            if self.with_name {
                                let hash = node.hash();
                                account.known_as.insert(hash, self.input_name.clone());
                            }
                            context.should_save = true;
                            self.add_node = false;
                        }
                    }
                });

            if !open {
                self.add_node = false;
            }
        }

        if let Some(hash) = self.set_name_for.take() {
            let mut close = false;
            let mut open = true;

            egui::Window::new(format!("Set Name for {}", base64_serialize(&hash).unwrap()))
                .open(&mut open)
                .show(ui.ctx(), |ui| {
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
                });

            if close {
                open = false;
            }

            if open {
                self.set_name_for = Some(hash);
            }
        }

        if ui.button("Add Node").clicked() {
            self.input_node.clear();
            self.input_name.clear();
            self.add_node = true;
        }

        ui.separator();

        egui::ScrollArea::vertical().show(ui, |ui| {
            let mut to_remove = Vec::new();

            for known in account.known_nodes.iter() {
                let hash = ToHash::hash(known);
                if let Some(name) = account.known_as.get(&hash).cloned() {
                    if let Some(info) = the_man.remote_info(*known) {
                        ui.horizontal(|ui| {
                            if info
                                .latency
                                .map(|l| l > std::time::Duration::from_secs_f32(1.))
                                .unwrap_or(true)
                            {
                                let res = ui
                                    .horizontal(|ui| {
                                        ui.colored_label(egui::Color32::GRAY, "D");
                                        ui.button(&name)
                                    })
                                    .inner;
                                if res.clicked() {
                                    let the_man = the_man.clone();
                                    let node_id = *known;
                                    context.add_task(Box::pin(async move {
                                        the_man.connect(node_id).await;
                                    }));
                                }

                                res
                            } else {
                                ui.colored_label(egui::Color32::GREEN, "C");
                                ui.label(&name)
                            }
                        })
                        .inner
                    } else {
                        let res = ui
                            .horizontal(|ui| {
                                ui.colored_label(egui::Color32::GRAY, "D");
                                ui.button(&name)
                            })
                            .inner;
                        if res.clicked() {
                            let the_man = the_man.clone();
                            let node_id = *known;
                            context.add_task(Box::pin(async move {
                                the_man.connect(node_id).await;
                            }));
                        }

                        res
                    }
                    .context_menu(|ui| {
                        if ui.small_button("Remove").clicked() {
                            to_remove.push(*known);
                            account.known_as.remove(&hash);
                        }
                        if ui.small_button("Remove Name").clicked() {
                            account.known_as.remove(&hash);
                        }
                        if ui.small_button("Rename").clicked() {
                            self.set_name_for = Some(hash);
                            self.input_name = name;
                        }
                    });
                } else {
                    ui.label(base64_serialize(known).unwrap().to_string())
                        .context_menu(|ui| {
                            if ui.small_button("Remove").clicked() {
                                to_remove.push(*known);
                            }
                            if ui.small_button("Set Name").clicked() {
                                self.set_name_for = Some(hash);
                                self.input_name.clear();
                            }
                        });
                }
            }

            for to_remove in to_remove {
                account.known_nodes.remove(&to_remove);
                context.should_save = true;
            }
        });
    }
}
