use eframe::egui;

use crate::Account;

mod active;
mod conversation;
mod conversations;
mod known_nodes;
mod status;

pub use active::PaneActive;
pub use conversation::PaneConversation;
pub use conversations::PaneConversations;
pub use known_nodes::PaneKnownNodes;
pub use status::PaneStatus;

pub trait Pane {
    fn name(&self, account: &Account) -> String;
    fn closable(&self) -> bool {
        false
    }
    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        context: &mut super::Context,
        the_man: &mut the_man::TheMan,
        account: &mut Account,
    );

    fn set_data(&mut self, _data: String) {}
    fn get_data(&self) -> String {
        String::default()
    }
}

impl Pane for String {
    fn name(&self, _account: &Account) -> String {
        self.clone()
    }

    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        _context: &mut super::Context,
        _the_man: &mut the_man::TheMan,
        _account: &mut Account,
    ) {
        ui.label(self.clone());
    }
}
