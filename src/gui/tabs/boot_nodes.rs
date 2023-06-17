use crate::logic::message::Message;

use super::Tab;

#[derive(Default, Clone)]
pub struct TabBootNodes {
    id: usize,
}

impl Tab for TabBootNodes {
    fn name(&self) -> &str {
        "Boot Nodes"
    }

    fn update(
        &mut self,
        ui: &mut egui::Ui,
        state: &mut crate::gui::TheManGuiState,
    ) -> Option<String> {
        ui.horizontal(|ui| {
            if ui.button("Refresh").clicked() {
                let _ = state.sender.try_send(Message::GetBootNodes);
            }
            ui.label(format!("Nodes: {}", state.bootnodes.len()));
            ui.spinner();
        });
        let row_height = ui.text_style_height(&egui::TextStyle::Body);
        egui::ScrollArea::both().show_rows(ui, row_height, state.bootnodes.len(), |ui, range| {
            for peer in &state.bootnodes[range] {
                ui.horizontal(|ui| {
                    ui.label(format!("Id: {}", peer.0));
                    ui.label(format!("Status: {:?}", peer.1));
                    ui.label(format!("Adresses: {:?}", peer.2));
                });
            }
        });
        None
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
