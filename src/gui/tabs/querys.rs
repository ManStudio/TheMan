use super::Tab;

#[derive(Default)]
pub struct TabQuerys {
    id: usize,
}

impl Tab for TabQuerys {
    fn name(&self) -> &str {
        "Querys"
    }

    fn update(
        &mut self,
        ui: &mut egui::Ui,
        state: &mut crate::gui::TheManGuiState,
    ) -> Option<String> {
        let mut message = None;

        let len = state.kademlia_query_progress.len();
        let vec = state.kademlia_query_progress.iter().collect::<Vec<_>>();
        ui.spinner();
        egui::ScrollArea::both().show_rows(
            ui,
            ui.text_style_height(&egui::TextStyle::Body),
            len,
            |ui, range| {
                for i in range {
                    if let Some((query_id, query)) = vec.get(i) {
                        ui.horizontal(|ui| {
                            if ui
                                .selectable_label(
                                    false,
                                    format!(
                                        "{i} {query_id:?} {}",
                                        if query.2.last { "Finished" } else { "Waiting" }
                                    ),
                                )
                                .clicked()
                            {
                                message = Some(format!("o9,{i}"));
                            }
                        });
                    }
                }
            },
        );

        message
    }

    fn hidden(&self) -> bool {
        false
    }

    fn recive(&mut self, _message: String) {}

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
