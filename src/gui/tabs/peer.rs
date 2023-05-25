use libp2p::PeerId;

use super::Tab;

#[derive(Default)]
pub struct TabPeer {
    id: usize,
    peer_id: Option<PeerId>,
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
        let Some(peer_id) = &self.peer_id else{return None};
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
