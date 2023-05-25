use eframe::egui;
use libp2p::PeerId;

use crate::state::{PingError, PingOk};

use super::Tab;

#[derive(Default)]
pub struct TabPeer {
    id: usize,
    peer_id: Option<PeerId>,
    name: String,
}

impl Tab for TabPeer {
    fn name(&self) -> &str {
        "Peer"
    }

    fn update(
        &mut self,
        ui: &mut eframe::egui::Ui,
        state: &mut crate::gui::TheManGuiState,
    ) -> Option<String> {
        let Some(peer_id) = &self.peer_id else{ui.label("No peer selected!");return None};
        let mut is_friend = false;
        if let Some(name) = state.register_names.get(peer_id) {
            ui.label("Saved name: {name}");
            is_friend = true;
        }
        if ui
            .selectable_label(false, format!("PeerId: {peer_id}"))
            .clicked()
        {
            ui.output_mut(|out| out.copied_text = peer_id.to_string());
        }
        if let Some(status) = state.peers.get(peer_id) {
            if let Some(info) = &status.info {
                ui.separator();
                ui.label("Info:");
                if ui
                    .selectable_label(false, format!("PublicKey: {:?}", info.public_key))
                    .clicked()
                {
                    ui.output_mut(|out| {
                        out.copied_text =
                            ron::to_string(&info.public_key.encode_protobuf()).unwrap()
                    });
                }
                if ui
                    .selectable_label(false, format!("Protocol: {}", info.protocol_version))
                    .clicked()
                {
                    ui.output_mut(|out| out.copied_text = info.protocol_version.clone());
                }

                if ui
                    .selectable_label(false, format!("Agent: {}", info.agent_version))
                    .clicked()
                {
                    ui.output_mut(|out| out.copied_text = info.agent_version.clone())
                }

                egui::CollapsingHeader::new("Adresses")
                    .show_background(true)
                    .show(ui, |ui| {
                        egui::ScrollArea::both()
                            .id_source("Peer Adresses")
                            .show(ui, |ui| {
                                for adress in info.listen_addrs.iter() {
                                    let string = adress.to_string();
                                    if ui.selectable_label(false, &string).clicked() {
                                        ui.output_mut(|out| out.copied_text = string)
                                    }
                                }
                            });
                    });

                egui::CollapsingHeader::new("Protocols")
                    .show_background(true)
                    .show(ui, |ui| {
                        egui::ScrollArea::both()
                            .id_source("Peer Protocols")
                            .show(ui, |ui| {
                                for protocol in info.protocols.iter() {
                                    if ui.selectable_label(false, protocol).clicked() {
                                        ui.output_mut(|out| out.copied_text = protocol.clone())
                                    }
                                }
                            });
                    });

                if ui
                    .selectable_label(false, format!("Oserved: {}", info.observed_addr))
                    .clicked()
                {
                    ui.output_mut(|out| out.copied_text = info.observed_addr.to_string())
                }

                ui.separator();
            }

            if let Some(ping) = &status.ping {
                match ping {
                    Ok(ping) => match ping {
                        PingOk::Pong => ui.label("Ping: Pong"),
                        PingOk::Ping(ping, rtt) => ui.label(format!(
                            "Ping: {}, Duration since ping: {}",
                            rtt.as_secs_f64(),
                            ping.elapsed().as_secs_f32()
                        )).on_hover_ui(|ui|{ui.label("If durations since is more the 15 seccons the connection probably died!");}),
                    },
                    Err(err) => match err {
                        PingError::Timeout => ui.label("Ping: Timeout"),
                        PingError::Unsupported => ui.label("Ping: Unsupported"),
                        PingError::Other(error) => ui.label(format!("Ping: Error: {error:?}")),
                    },
                };
            } else {
                ui.label("No ping!");
            }

            ui.separator();
            if !is_friend {
                ui.label("Add as friend!");
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.name);
                    if ui.button("Add").clicked() {
                        state.friends.push(crate::save_state::Friend {
                            peer_id: peer_id.clone(),
                            name: self.name.clone(),
                        });
                        let _ = state.sender.try_send(crate::logic::message::Message::Gui(
                            crate::logic::message::GuiMessage::Friends(state.friends.clone()),
                        ));
                    }
                });
            } else {
                if ui.button("Remove friend").clicked() {
                    state.friends.retain(|friend| friend.peer_id != *peer_id);
                    let _ = state.sender.try_send(crate::logic::message::Message::Gui(
                        crate::logic::message::GuiMessage::Friends(state.friends.clone()),
                    ));
                }
            }
        }
        None
    }

    fn recive(&mut self, message: String) {
        if let Ok(peer_id) = message.parse::<PeerId>() {
            self.peer_id = Some(peer_id)
        }
    }

    fn clone_box(&self) -> Box<dyn Tab> {
        Box::<Self>::default()
    }

    fn id(&self) -> usize {
        self.id
    }

    fn set_id(&mut self, id: usize) {
        self.id = id;
    }
}
