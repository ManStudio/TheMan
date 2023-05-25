use eframe::egui;

use crate::logic::message::{Message, VoiceMessage};

use super::Tab;

#[derive(Default)]
pub struct TabVoiceChannel {
    id: usize,
    name: String,
    sender: Option<tokio::sync::mpsc::Sender<crate::logic::message::Message>>,
    init: bool,
}

impl Tab for TabVoiceChannel {
    fn name(&self) -> &str {
        "Voice Channel"
    }

    fn update(
        &mut self,
        ui: &mut eframe::egui::Ui,
        state: &mut crate::gui::TheManGuiState,
    ) -> Option<String> {
        let mut message = None;
        if self.name.is_empty() {
            ui.label("VoiceChannel has not Topic");
            return None;
        }

        if !self.init {
            self.init = true;
            let _ = state
                .sender
                .try_send(Message::Voice(VoiceMessage::Connect(self.name.clone())));
        }

        if self.sender.is_none() {
            self.sender = Some(state.sender.clone());
        }

        ui.vertical_centered(|ui| {
            if ui
                .selectable_label(
                    false,
                    egui::widget_text::WidgetText::RichText(
                        egui::RichText::new(format!("Channel Name: {}", self.name)).size(21.0),
                    ),
                )
                .clicked()
            {
                ui.output_mut(|out| out.copied_text = self.name.clone());
            }
        });

        ui.separator();

        ui.allocate_ui_with_layout(
            ui.available_size(),
            egui::Layout::left_to_right(egui::Align::LEFT),
            |ui| {
                if let Some(peers) = state.voice_connected.get_mut(&self.name) {
                    let height = ui.available_height();
                    let width = ui.available_width();
                    let connected_width = width * 0.5;
                    let requests_width = width * 0.5;

                    ui.allocate_ui_with_layout(
                        egui::Vec2::new(connected_width, height),
                        egui::Layout::top_down(egui::Align::TOP),
                        |ui| {
                            ui.vertical_centered(|ui| ui.label("Connected:"));
                            ui.separator();
                            egui::ScrollArea::both()
                                .id_source("Connected: ")
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    for (peer, connected) in
                                        peers.iter_mut().filter(|(_, state)| **state)
                                    {
                                        let name =
                                            if let Some(name) = state.register_names.get(peer) {
                                                name.clone()
                                            } else {
                                                format!("PeerId: {peer}")
                                            };
                                        let res = ui.selectable_label(false, name);
                                        if res.clicked() {
                                            let _ = state.sender.try_send(Message::Voice(
                                                VoiceMessage::Refuse(self.name.clone(), *peer),
                                            ));
                                            *connected = false;
                                        }
                                        if res.secondary_clicked() {
                                            message = Some(format!("o14,{peer}"))
                                        }
                                    }
                                });
                        },
                    );

                    ui.separator();

                    ui.allocate_ui_with_layout(
                        egui::Vec2::new(requests_width, height),
                        egui::Layout::top_down(egui::Align::TOP),
                        |ui| {
                            ui.vertical_centered(|ui| ui.label("Requests:"));
                            ui.separator();
                            egui::ScrollArea::both()
                                .id_source("Requests: ")
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    for (peer, connected) in
                                        peers.iter_mut().filter(|(_, state)| !**state)
                                    {
                                        let name =
                                            if let Some(name) = state.register_names.get(peer) {
                                                name.clone()
                                            } else {
                                                format!("PeerId: {peer}")
                                            };

                                        let res = ui.selectable_label(false, name);
                                        if res.clicked() {
                                            let _ = state.sender.try_send(Message::Voice(
                                                VoiceMessage::Accept(self.name.clone(), *peer),
                                            ));
                                            *connected = true;
                                        }

                                        if res.secondary_clicked() {
                                            message = Some(format!("o14,{peer}"))
                                        }
                                    }
                                });
                        },
                    );
                }
            },
        );
        message
    }

    fn recive(&mut self, message: String) {
        self.name = message;
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

impl Drop for TabVoiceChannel {
    fn drop(&mut self) {
        if let Some(sender) = &mut self.sender {
            let _ = sender.try_send(Message::Voice(VoiceMessage::Disconnect(self.name.clone())));
        }
    }
}
