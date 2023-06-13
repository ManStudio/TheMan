use libp2p::{identity::Keypair, PeerId};

use crate::logic::message::Message;

use super::Tab;

#[derive(Default, Clone)]
pub struct TabAccount {
    id: usize,
    account_id: usize,
    name: String,
    peer_id: String,
}

impl Tab for TabAccount {
    fn name(&self) -> &str {
        "Account"
    }

    fn update(
        &mut self,
        ui: &mut eframe::egui::Ui,
        state: &mut crate::gui::TheManGuiState,
    ) -> Option<String> {
        if self.name.is_empty() && self.peer_id.is_empty() {
            if let Some(account) = state.accounts.get(self.account_id) {
                let Ok(private) = Keypair::from_protobuf_encoding(&account.private) else{return None};
                self.peer_id = PeerId::from(private.public()).to_string();
                self.name = account.name.clone();
            }
        }

        #[allow(clippy::blocks_in_if_conditions)]
        if ui
            .selectable_label(false, format!("Id: {}", self.peer_id))
            .on_hover_ui(|ui| {
                ui.label("This is you public key!");
            })
            .clicked()
        {
            ui.output_mut(|out| out.copied_text = self.peer_id.clone());
        }

        ui.horizontal(|ui| {
            ui.label("Account Name:");
            ui.text_edit_singleline(&mut self.name);
        });

        let mut expires = chrono::Utc::now();
        if let Some(account) = state.accounts.get_mut(self.account_id) {
            expires = account.expires;
            ui.checkbox(&mut account.renew, "Auto Renew");
        }

        ui.label(format!("Expires on: {}", expires.format("%d/%m/%Y %H:%M"))).on_hover_ui(|ui| {ui.label("That means that you should be connected to your accont at that time or some one else could get your name!");});

        if ui.button("Save").clicked() {
            if let Some(account) = state.accounts.get_mut(self.account_id) {
                if account.name == self.name {
                    account.expires = chrono::Utc::now();
                }
                account.name = self.name.clone();
                state.send(Message::UpdateAccounts(state.accounts.clone()));
            }
        }

        if ui.button("Load").clicked() {
            if let Some(account) = state.accounts.get(self.account_id) {
                let Ok(private) = Keypair::from_protobuf_encoding(&account.private) else{return None};
                self.peer_id = PeerId::from(private.public()).to_string();
                self.name = account.name.clone();
            }
        }

        ui.separator();

        if ui.button("Delete").clicked() && state.accounts.get(self.account_id).is_some() {
            state.accounts.remove(self.account_id);
            self.name = String::new();
            self.peer_id = String::new();
            state.send(Message::UpdateAccounts(state.accounts.clone()));
        }

        None
    }

    fn hidden(&self) -> bool {
        true
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

    fn recive(&mut self, message: String) {
        let Ok(num) = message.parse() else {return};
        self.account_id = num;
    }
}
