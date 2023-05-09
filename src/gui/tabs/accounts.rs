use super::Tab;

#[derive(Default, Clone)]
pub struct TabAccounts {
    id: usize,
}

impl Tab for TabAccounts {
    fn name(&self) -> &str {
        "Accounts"
    }

    fn update(&mut self, ui: &mut eframe::egui::Ui, state: &mut crate::gui::TheManGuiState) {
        if ui.button("Refresh").clicked() {
            state.send(crate::logic::message::Message::GetAccounts);
        }

        let mut to_send = vec![];

        for (i, account) in state.accounts.iter().enumerate() {
            if ui.button(format!("Account: {}", account.name)).clicked() {
                to_send.push(crate::logic::message::Message::SetAccount(i));
            }
        }

        for message in to_send {
            state.send(message)
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
