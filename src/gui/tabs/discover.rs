use libp2p::PeerId;

use crate::logic::message::Message;

use super::Tab;

#[derive(Default, Clone)]
pub struct TabDiscover {
    id: usize,
    peer_id: String,
}

impl Tab for TabDiscover {
    fn name(&self) -> &str {
        "Discover"
    }

    fn update(&mut self, ui: &mut eframe::egui::Ui, state: &mut crate::gui::TheManGuiState) {
        ui.horizontal(|ui| {
            ui.label("PeerId: ");
            ui.text_edit_singleline(&mut self.peer_id);
            ui.separator();
            if ui.button("Search").clicked() {
                if let Ok(peer_id) = self.peer_id.parse::<PeerId>() {
                    state.send(Message::SearchPeerId(peer_id));
                } else {
                    eprintln!("Cannot Parse PeerId");
                }
            }
        });
    }

    fn clone_box(&self) -> Box<dyn Tab> {
        Box::new(self.clone())
    }

    fn id(&self) -> usize {
        self.id
    }

    fn set_id(&mut self, id: usize) {
        self.id = id;
    }
}
