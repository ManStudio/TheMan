use gui_deps::*;

use std::collections::BTreeSet;

use eframe::egui;
use iroh::NodeId;
use the_man::{base64_serialize, protocol::RawConversation};

use crate::gui::ToHash;

use super::Popup;

pub struct PopupCreateConversation {
    the_man: the_man::TheMan,
    nodes: BTreeSet<NodeId>,
}

impl PopupCreateConversation {
    pub fn new(the_man: the_man::TheMan) -> Self {
        Self {
            the_man,
            nodes: BTreeSet::default(),
        }
    }
}

impl Popup for PopupCreateConversation {
    fn show(
        &mut self,
        ui: &mut egui::Ui,
        context: &mut crate::gui::Context,
        account: &mut crate::Account,
    ) -> bool {
        if ui.button("Create").clicked() {
            let raw = RawConversation {
                nodes: self
                    .nodes
                    .iter()
                    .copied()
                    .chain([self.the_man.node_id()])
                    .collect::<Vec<_>>(),
                time: chrono::Utc::now(),
            };
            let the_man = self.the_man.clone();
            context.add_task(Box::pin(async move {
                the_man.create(raw).await;
            }));
            return true;
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            for known in account.known_nodes.iter() {
                let selected = self.nodes.contains(known);
                if ui
                    .selectable_label(
                        selected,
                        account
                            .known_as
                            .get(&ToHash::hash(known))
                            .cloned()
                            .unwrap_or_else(|| base64_serialize(known).unwrap()),
                    )
                    .clicked()
                {
                    if selected {
                        self.nodes.remove(known);
                    } else {
                        self.nodes.insert(*known);
                    }
                }
            }
        });

        false
    }

    fn name(&self) -> String {
        String::from("Create Conversation")
    }
}
