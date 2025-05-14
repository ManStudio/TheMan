use gui_deps::*;

use crate::gui::ToHash;

use super::Popup;

use eframe::egui::{self};
use iroh::NodeId;
use the_man::base64_deserialize;

#[derive(Default)]
pub struct PopupAddNode {
    input_node: String,
    input_name: Option<String>,
}

impl Popup for PopupAddNode {
    fn show(
        &mut self,
        ui: &mut eframe::egui::Ui,
        context: &mut crate::gui::Context,
        account: &mut crate::Account,
    ) -> bool {
        egui::TextEdit::singleline(&mut self.input_node)
            .hint_text("Node")
            .show(ui);

        let node = base64_deserialize::<NodeId>(&self.input_node);
        if let Err(err) = &node {
            if !self.input_node.is_empty() {
                ui.colored_label(egui::Color32::RED, format!("Node Id: {err:?}"));
            }
        }

        ui.horizontal(|ui| {
            let mut with_name = self.input_name.is_some();
            ui.checkbox(&mut with_name, "With Name?");

            if with_name {
                if self.input_name.is_none() {
                    self.input_name = Some(String::default());
                }
            } else {
                self.input_name = None;
            }

            if let Some(name) = &mut self.input_name {
                egui::TextEdit::singleline(name).hint_text("Name").show(ui);
            }
        });

        if let Ok(node) = node {
            if ui.button("Add").clicked() {
                account.known_nodes.insert(node);
                if let Some(name) = &self.input_name {
                    let hash = ToHash::hash(&node);
                    account.known_as.insert(hash, name.clone());
                }
                context.should_save = true;
                return true;
            }
        }

        false
    }

    fn name(&self) -> String {
        String::from("Add Node")
    }
}
