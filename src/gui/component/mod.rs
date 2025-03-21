use crate::Account;

use eframe::egui;
use the_man::base64_serialize;

use super::{Context, ToHash, popup::PopupSetName};

pub fn with_name(
    hash: &impl ToHash,
    ui: &mut egui::Ui,
    context: &mut Context,
    account: &mut Account,
    context_menu: impl FnOnce(&mut egui::Ui),
) {
    let hash = hash.hash();

    if let Some(name) = account.known_as.get(&hash).cloned() {
        ui.label(&name).context_menu(|ui| {
            if ui.small_button("Remove Name").clicked() {
                account.known_as.remove(&hash);
                ui.close_menu();
            }

            if ui.small_button("Rename").clicked() {
                context.add_popup(PopupSetName::new(hash, name));
                ui.close_menu();
            }

            context_menu(ui);
        });
    } else {
        ui.label(base64_serialize(&hash).unwrap().to_string())
            .context_menu(|ui| {
                if ui.small_button("Set Name").clicked() {
                    context.add_popup(PopupSetName::new(hash, String::default()));
                    ui.close_menu();
                }
                context_menu(ui);
            });
    }
}
