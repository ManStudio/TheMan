use iroh_blobs::Hash;
use the_man::base64_serialize;

use gui_deps::*;

use eframe::egui;

use super::Popup;

pub struct PopupSetName {
    hash: Hash,
    name: String,
}

impl PopupSetName {
    pub fn new(hash: Hash, with_name: String) -> Self {
        Self {
            hash,
            name: with_name,
        }
    }
}

impl Popup for PopupSetName {
    fn show(
        &mut self,
        ui: &mut eframe::egui::Ui,
        context: &mut crate::gui::Context,
        account: &mut crate::Account,
    ) -> bool {
        egui::TextEdit::singleline(&mut self.name)
            .hint_text("Name")
            .show(ui);
        if ui.button("Set Name").clicked() {
            account.known_as.insert(self.hash, self.name.clone());
            context.should_save = true;

            return true;
        }

        false
    }

    fn name(&self) -> String {
        format!("Set Name for {}", base64_serialize(&self.hash).unwrap())
    }
}
