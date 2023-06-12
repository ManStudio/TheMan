use eframe::egui;
use libp2p::PeerId;

use crate::{logic::message::Message, state::PeerStatus};

use super::Tab;

#[derive(Default, Clone)]
pub struct TabPeers {
    id: usize,
    filter: String,
}

impl Tab for TabPeers {
    fn name(&self) -> &str {
        "Peers"
    }

    fn update(
        &mut self,
        ui: &mut eframe::egui::Ui,
        state: &mut crate::gui::TheManGuiState,
    ) -> Option<String> {
        let mut message = None;
        let row_height = ui.text_style_height(&egui::TextStyle::Body);
        let peers = state
            .peers
            .iter()
            .filter(|peer| peer.0.to_string().contains(&self.filter))
            .collect::<Vec<(&PeerId, &PeerStatus)>>();
        ui.horizontal(|ui| {
            ui.label(format!("Peers: {}", peers.len()));
            ui.separator();
            ui.label("Filter");
            ui.text_edit_singleline(&mut self.filter);
            ui.separator();
            if ui.button("Refresh").clicked() {
                let _ = state.sender.try_send(Message::GetBootNodes);
            }
            ui.spinner();
        });

        egui::ScrollArea::both().show_rows(ui, row_height, peers.len(), |ui, range| {
            for i in range {
                if let Some(peer) = peers.get(i) {
                    ui.horizontal(|ui| {
                        let res = ui.selectable_label(false, format!("PeerId: {}", peer.0));
                        if res.clicked() {
                            message = Some(format!("o14,{}", peer.0));
                        }
                        ui.label(format!("Ping: {:?}", peer.1.ping));
                        ui.label(format!("Info: {:?}", peer.1.info));
                    });
                }
            }
        });
        message
    }

    fn hidden(&self) -> bool {
        false
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

    fn recive(&mut self, _message: String) {}
}
