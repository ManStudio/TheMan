use eframe::egui;
use libp2p::gossipsub::{IdentTopic, TopicHash};

use super::Tab;

#[derive(Default)]
pub struct TabMessageChannel {
    id: usize,
    name: String,
    topic: Option<IdentTopic>,
    initializated: bool,
    split: f32,
    message: String,
    sender: Option<tokio::sync::mpsc::Sender<crate::logic::message::Message>>,
}

impl Tab for TabMessageChannel {
    fn name(&self) -> &str {
        "Message Channel"
    }

    fn update(
        &mut self,
        ui: &mut eframe::egui::Ui,
        state: &mut crate::gui::TheManGuiState,
    ) -> Option<String> {
        let Some(topic) = self.topic.clone() else {
            ui.label("This Message Channel Has Not Topic");
            return None};
        if !self.initializated {
            state.send(crate::logic::message::Message::SubscribeTopic(
                topic.clone(),
            ));
            self.split = 0.7;
            self.sender = Some(state.sender.clone());
            self.initializated = true;
        }

        ui.label(&self.name);
        let max_height =
            (ui.available_height() - ui.text_style_height(&egui::TextStyle::Body)) - 6.0; // separator has 6 height by default
        ui.horizontal(|ui| {
            let width = ui.available_width();
            let message_width = width * self.split;
            let peers_width = width - message_width;

            ui.vertical(|ui| {
                ui.label("Messages: ");
                let empty = vec![];
                let messages = if let Some(messages) = state.messages.get(&topic.hash()) {
                    messages
                } else {
                    &empty
                };
                egui::ScrollArea::both()
                    .max_width(message_width)
                    .max_height(max_height)
                    .id_source("Messages")
                    .show_rows(
                        ui,
                        ui.text_style_height(&egui::TextStyle::Body),
                        messages.len(),
                        |ui, range| {
                            for message in &messages[range] {
                                ui.horizontal(|ui| {
                                    ui.label(format!(
                                        "From: {}",
                                        match &message.source {
                                            Some(s) => s.to_string(),
                                            None => "NoBudy".to_string(),
                                        }
                                    ));
                                    match String::from_utf8(message.data.clone()) {
                                        Ok(text) => {
                                            ui.label(text);
                                        }
                                        Err(err) => {
                                            ui.label(format!("Bytes: {:?}", err.as_bytes()));
                                        }
                                    }
                                });
                            }
                        },
                    );
            });
            ui.separator();
            ui.vertical(|ui| {
                ui.label("Peers: ");

                let empty = vec![];
                let peers = if let Some(peers) = state.subscribers.get(&topic.hash()) {
                    peers
                } else {
                    &empty
                };
                egui::ScrollArea::both()
                    .id_source("Peers")
                    .max_width(peers_width)
                    .max_height(max_height)
                    .show_rows(
                        ui,
                        ui.text_style_height(&egui::TextStyle::Body),
                        peers.len(),
                        |ui, range| {
                            for peer in &peers[range] {
                                if ui.selectable_label(false, format!("{}", peer)).clicked() {
                                    ui.output_mut(|out| out.copied_text = format!("{}", peer));
                                }
                            }
                        },
                    );
            })
        });

        ui.separator();

        ui.horizontal(|ui| {
            ui.label("Message: ");
            ui.text_edit_singleline(&mut self.message);
            if ui.button("Send").clicked() {
                state.send(crate::logic::message::Message::SendMessage(
                    topic.hash(),
                    self.message.clone().into_bytes(),
                ));
            }
        });
        None
    }

    fn recive(&mut self, message: String) {
        self.name = message.clone();
        self.topic = Some(IdentTopic::new(message));
    }

    fn clone_box(&self) -> Box<dyn Tab> {
        Box::<Self>::default()
    }

    fn id(&self) -> usize {
        self.id
    }

    fn set_id(&mut self, id: usize) {
        self.id = id
    }
}

impl Drop for TabMessageChannel {
    fn drop(&mut self) {
        if let Some(sender) = &self.sender {
            if let Some(topic) = self.topic.clone() {
                sender.try_send(crate::logic::message::Message::UnsubscibeTopic(topic));
            }
        }
    }
}
