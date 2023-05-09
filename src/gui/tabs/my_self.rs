use crate::logic::message::Message;

use super::Tab;

#[derive(Default, Clone)]
pub struct TabMySelf {
    id: usize,
}

impl Tab for TabMySelf {
    fn name(&self) -> &str {
        "My Self"
    }

    fn update(&mut self, ui: &mut eframe::egui::Ui, state: &mut crate::gui::TheManGuiState) {
        if ui.button("Refresh").clicked() {
            state.send(Message::GetAdresses);
        }
        if let Some(peer_id) = &state.peer_id {
            if ui
                .selectable_label(false, format!("PeerId {}", peer_id))
                .clicked()
            {
                ui.output_mut(|o| o.copied_text = format!("{}", peer_id));
            }
        }
        ui.label("Adresses:");
        for adress in state.adresses.iter() {
            if ui
                .selectable_label(false, format!("{}", adress.addr))
                .clicked()
            {
                ui.output_mut(|out| out.copied_text = format!("{}", adress.addr));
            }
        }
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
