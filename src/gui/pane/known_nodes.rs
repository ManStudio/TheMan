use crate::{
    Account,
    gui::{ToHash, component::with_name, popup::PopupAddNode},
};

use super::Pane;

use eframe::egui;
use iroh::NodeId;

#[derive(Default)]
pub struct PaneKnownNodes {}

impl Pane for PaneKnownNodes {
    fn name(&self, _account: &Account) -> String {
        String::from("Known Nodes")
    }

    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        context: &mut crate::gui::Context,
        the_man: &mut the_man::TheMan,
        account: &mut crate::Account,
    ) {
        if ui.button("Add Node").clicked() {
            context.add_popup(PopupAddNode::default());
        }

        ui.separator();

        egui::ScrollArea::vertical().show(ui, |ui| {
            let mut to_remove = Vec::<NodeId>::new();

            for known in account.known_nodes.iter().cloned().collect::<Vec<_>>() {
                ui.horizontal(|ui| {
                    let connected = 'status: {
                        if let Some(info) = the_man.remote_info(known) {
                            if info
                                .latency
                                .map(|l| l < std::time::Duration::from_secs_f32(1.))
                                .unwrap_or(false)
                            {
                                ui.colored_label(egui::Color32::GREEN, "C");
                                break 'status true;
                            }
                        }
                        ui.colored_label(egui::Color32::RED, "D");
                        false
                    };

                    with_name(&known, ui, context, account, |ui| {
                        if ui.small_button("Remove").clicked() {
                            to_remove.push(known);
                            ui.close_menu();
                        }
                    });

                    if !connected && ui.button("Connect").clicked() {
                        let the_man = the_man.clone();
                        let node_id = known;
                        context.add_task(Box::pin(async move {
                            the_man.connect(node_id).await;
                        }));
                    }
                });
            }

            for to_remove in to_remove {
                account.known_nodes.remove(&to_remove);
                account.known_as.remove(&ToHash::hash(&to_remove));
                context.should_save = true;
            }
        });
    }
}
