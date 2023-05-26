use eframe::egui;
use libp2p::{multihash::Multihash, PeerId};

use super::Tab;

#[derive(Default)]
pub struct TabFriends {
    id: usize,
    name: String,
    peer_id: String,
}

impl Tab for TabFriends {
    fn name(&self) -> &str {
        "Friends"
    }

    fn update(
        &mut self,
        ui: &mut eframe::egui::Ui,
        state: &mut crate::gui::TheManGuiState,
    ) -> Option<String> {
        let mut message = None;
        if ui.button("Refresh").clicked() {
            let _ = state.sender.try_send(crate::logic::message::Message::Gui(
                crate::logic::message::GuiMessage::RefreshFriends,
            ));
        }

        egui::panel::TopBottomPanel::bottom("Add frient").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Name: ");
                ui.text_edit_singleline(&mut self.name);
                ui.label("PeerId: ");
                ui.text_edit_singleline(&mut self.peer_id);

                if ui.button("Add").clicked() {
                    if let Ok(peer_id) = self.peer_id.parse::<PeerId>() {
                        self.peer_id.clear();
                        let name = self.name.clone();
                        self.name.clear();
                        state
                            .friends
                            .push(crate::save_state::Friend { peer_id, name });
                        let _ = state.sender.try_send(crate::logic::message::Message::Gui(
                            crate::logic::message::GuiMessage::Friends(state.friends.clone()),
                        ));
                    }
                }
            })
        });

        egui::ScrollArea::both().show(ui, |ui| {
            for friend in state.friends.iter() {
                if ui
                    .selectable_label(
                        false,
                        format!(
                            "PeerId: {}, Online: {}, Name: {}",
                            friend.peer_id,
                            state.peers.contains_key(&friend.peer_id),
                            friend.name
                        ),
                    )
                    .clicked()
                {
                    message = Some(format!("o14,{}", friend.peer_id))
                }
            }
        });

        message
    }

    fn recive(&mut self, message: String) {}

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
