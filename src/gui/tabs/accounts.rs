use libp2p::identity::Keypair;

use crate::save_state::Account;

use super::Tab;

#[derive(Default, Clone)]
pub struct TabAccounts {
    id: usize,
    add_user_name: String,
}

impl Tab for TabAccounts {
    fn name(&self) -> &str {
        "Accounts"
    }

    fn update(
        &mut self,
        ui: &mut eframe::egui::Ui,
        state: &mut crate::gui::TheManGuiState,
    ) -> Option<String> {
        let mut message = None;
        if ui.button("Refresh").clicked() {
            state.send(crate::logic::message::Message::GetAccounts);
        }

        let mut to_send = vec![];

        ui.separator();

        ui.label("Accounts:");
        for (i, account) in state.accounts.iter().enumerate() {
            let button = ui.button(account.name.to_string());
            if button.clicked() {
                to_send.push(crate::logic::message::Message::SetAccount(i));
            }
            if button.secondary_clicked() {
                message = Some(format!("o6,{i}"));
            }
        }

        ui.separator();

        ui.label("Add account:");
        ui.horizontal(|ui| {
            ui.label("Account Name: ");
            ui.text_edit_singleline(&mut self.add_user_name);
            if ui.button("Add").clicked() {
                let allready_taken = state
                    .accounts
                    .iter()
                    .filter(|account| account.name == self.add_user_name)
                    .count()
                    > 0;
                if allready_taken {
                    eprintln!("The Account name is allready taken!");
                } else {
                    state.accounts.push(Account {
                        name: self.add_user_name.clone(),
                        private: Keypair::generate_ed25519().to_protobuf_encoding().unwrap(),
                    });
                    to_send.push(crate::logic::message::Message::UpdateAccounts(
                        state.accounts.clone(),
                    ));
                }
            }
        });

        for message in to_send {
            state.send(message)
        }

        message
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
