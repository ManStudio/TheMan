use gui_deps::*;

use the_man::base64_serialize;

use eframe::egui;

use crate::Account;

use super::Pane;

#[derive(Default)]
pub struct PaneStatus {
    confirm_copy_secret: bool,
}

impl Pane for PaneStatus {
    fn name(&self, _account: &Account) -> String {
        String::from("Status")
    }

    fn ui(
        &mut self,
        ui: &mut eframe::egui::Ui,
        _context: &mut crate::gui::Context,
        _the_man: &mut the_man::TheMan,
        account: &mut crate::Account,
    ) {
        if self.confirm_copy_secret {
            egui::Modal::new(ui.id().with("Confirm copy secret key"))
                .show(ui.ctx(), |ui| {
                    ui.heading("Secret Copy");
                    ui.colored_label(egui::Color32::YELLOW, "You are sure you want to copy the secret key, anyone can pretend to be you using that!");
                    ui.colored_label(egui::Color32::RED, "You account will be compromised, if you share that with someone!");
                    ui.separator();
                    ui.horizontal(|ui|{
                        if ui.button("No").clicked(){
                            self.confirm_copy_secret = false;
                        }
                        if ui.button("Yes").clicked(){
                            ui.ctx()
                                .copy_text(base64_serialize(&account.secret).unwrap());
                            self.confirm_copy_secret = false;
                        }
                    });
                });
        }

        ui.group(|ui| {
            ui.horizontal(|ui| {
                if ui
                    .button(&account.name)
                    .on_hover_text(base64_serialize(&account.secret.public()).unwrap())
                    .clicked()
                {
                    ui.ctx()
                        .copy_text(base64_serialize(&account.secret.public()).unwrap());
                }
                if ui.button("Copy Secret").clicked() {
                    self.confirm_copy_secret = true;
                }
            });
        });
    }
}
