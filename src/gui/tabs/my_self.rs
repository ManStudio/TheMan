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
        if let Some(peer_id) = &state.peer_id {
            if ui
                .selectable_label(false, format!("PeerId {}", peer_id))
                .clicked()
            {
                ui.output_mut(|o| o.copied_text = format!("{}", peer_id));
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
