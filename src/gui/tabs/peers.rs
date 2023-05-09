use eframe::egui;

use crate::logic::message::Message;

use super::Tab;

#[derive(Default, Clone)]
pub struct TabPeers {}

impl Tab for TabPeers {
    fn name(&self) -> &str {
        "Peers"
    }

    fn update(&mut self, ui: &mut eframe::egui::Ui, state: &mut crate::gui::TheManGuiState) {
        ui.horizontal(|ui| {
            if ui.button("Refresh").clicked() {
                let _ = state.sender.try_send(Message::GetBootNodes);
            }
            ui.label(format!("Peers: {}", state.peers.len()));
        });
        let row_height = ui.text_style_height(&egui::TextStyle::Body);
        egui::ScrollArea::both().show_rows(ui, row_height, state.peers.len(), |ui, range| {
            let peers = &state.peers;
            for i in range {
                if let Some(peer) = peers.get(i) {
                    ui.horizontal(|ui| {
                        ui.label(format!("PeerId: {}", peer.0));
                        ui.label(format!("Ping: {:?}", peer.1.ping));
                        ui.label(format!("Info: {:?}", peer.1.info));
                    });
                }
            }
        });
    }

    fn clone_box(&self) -> Box<dyn Tab> {
        Box::new(self.clone())
    }
}
