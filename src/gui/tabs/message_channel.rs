use eframe::egui;
use libp2p::gossipsub::IdentTopic;

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

        ui.vertical_centered_justified(|ui| {
            if ui
                .selectable_label(
                    false,
                    egui::widget_text::WidgetText::RichText(
                        egui::RichText::new(format!("Channel Name: {}", self.name)).size(21.0),
                    ),
                )
                .clicked()
            {
                ui.output_mut(|out| {
                    out.copied_text = self.name.clone();
                })
            };
        });
        ui.separator();
        let max_height =
            (ui.available_height() - ui.text_style_height(&egui::TextStyle::Heading)) - 12.0; // separator has 6 height by default
        ui.horizontal(|ui| {
            let width = ui.available_width();
            let message_width = width * self.split;
            let peers_width = width - message_width;

            ui.allocate_ui(egui::Vec2::new(width, max_height), |ui| {
                ui.vertical(|ui| {
                    ui.label("Messages: ");
                    let empty = vec![];
                    let messages = if let Some(messages) = state.messages.get(&topic.hash()) {
                        messages
                    } else {
                        &empty
                    };
                    egui::ScrollArea::both()
                        .auto_shrink([false, false])
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
                                        match &message.source {
                                            Some(from) => {
                                                if let Some(peer_id) = &state.peer_id {
                                                    if from == peer_id {
                                                        ui.label("From You");
                                                    } else {
                                                        if let Some(from) =
                                                            state.register_names.get(from)
                                                        {
                                                            ui.label(format!("From: {from}"));
                                                        } else {
                                                            ui.label("From PeerId: ");
                                                            if ui
                                                                .selectable_label(
                                                                    false,
                                                                    &from.to_string(),
                                                                )
                                                                .clicked()
                                                            {
                                                                ui.output_mut(|out| {
                                                                    out.copied_text =
                                                                        from.to_string()
                                                                });
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            None => {
                                                ui.label("From: NoBudy");
                                            }
                                        };
                                    });
                                    let text = match String::from_utf8(message.data.clone()) {
                                        Ok(text) => text,
                                        Err(err) => {
                                            format!("Bytes: {:?}", err.as_bytes())
                                        }
                                    };
                                    ui.horizontal(|ui| {
                                        ui.label("    ");
                                        if ui.selectable_label(false, &text).clicked() {
                                            ui.output_mut(|out| out.copied_text = text.clone());
                                        }
                                    });
                                    ui.separator();
                                }
                            },
                        );
                });
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
                    .auto_shrink([false, false])
                    .max_width(peers_width)
                    .max_height(max_height)
                    .show_rows(
                        ui,
                        ui.text_style_height(&egui::TextStyle::Body),
                        peers.len(),
                        |ui, range| {
                            for peer in &peers[range] {
                                if let Some(name) = state.register_names.get(&peer) {
                                    ui.label(name);
                                } else {
                                    if ui
                                        .selectable_label(false, format!("PeerId: {}", peer))
                                        .clicked()
                                    {
                                        ui.output_mut(|out| out.copied_text = format!("{}", peer));
                                    }
                                }
                                ui.separator();
                            }
                        },
                    );
            })
        });

        ui.separator();

        ui.horizontal(|ui| {
            ui.label("Message: ");
            let width = ui.available_width() - 60.0;
            ui.add(egui::TextEdit::singleline(&mut self.message).desired_width(width));
            if ui.button("Send").clicked() {
                state.send(crate::logic::message::Message::SendMessage(
                    topic.hash(),
                    self.message.clone().into_bytes(),
                ));
                let message = libp2p::gossipsub::Message {
                    source: state.peer_id,
                    data: self.message.as_bytes().to_vec(),
                    sequence_number: None,
                    topic: topic.hash(),
                };
                if let Some(messages) = state.messages.get_mut(&topic.hash()) {
                    messages.push(message);
                } else {
                    state.messages.insert(topic.hash(), vec![message]);
                }
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
                let _ = sender.try_send(crate::logic::message::Message::UnsubscibeTopic(topic));
            }
        }
    }
}
