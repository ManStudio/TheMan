use libp2p::{kad::QueryId, PeerId};

use crate::logic::message::Message;

use super::Tab;

#[derive(Default, Clone)]
pub struct TabDiscover {
    id: usize,
    peer_id: String,
    waiting_for: Option<PeerId>,
}

impl Tab for TabDiscover {
    fn name(&self) -> &str {
        "Discover"
    }

    fn update(&mut self, ui: &mut eframe::egui::Ui, state: &mut crate::gui::TheManGuiState) {
        ui.horizontal(|ui| {
            ui.label("PeerId: ");
            ui.text_edit_singleline(&mut self.peer_id);
            ui.separator();
            if ui.button("Search").clicked() {
                if let Ok(peer_id) = self.peer_id.parse::<PeerId>() {
                    self.waiting_for = Some(peer_id);
                    state.send(Message::SearchPeerId(peer_id));
                } else {
                    eprintln!("Cannot Parse PeerId");
                }
            }
        });

        let Some(peer_id) = &self.waiting_for else{return};
        if let Some(query_id) = state.query_id_for_peers.get(peer_id) {
            ui.label(format!("QueryId: {:?}", query_id));
            if let Some((res, stats, step)) = state.kademlia_query_progress.get(query_id) {
                ui.label("Results: ");

                if let libp2p::kad::QueryResult::GetClosestPeers(res) = res {
                    match res {
                        Ok(finded) => {
                            ui.horizontal(|ui| {
                                if ui
                                    .selectable_label(false, format!("{:?}", finded.key))
                                    .clicked()
                                {
                                    ui.output_mut(|out| {
                                        out.copied_text = format!("{:?}", finded.key)
                                    })
                                }
                            });
                            ui.label("Peers:");
                            for peer in finded.peers.iter() {
                                if ui.selectable_label(false, peer.to_string()).clicked() {
                                    ui.output_mut(|out| out.copied_text = peer.to_string())
                                }
                            }
                        }
                        Err(err) => match err {
                            libp2p::kad::GetClosestPeersError::Timeout { key, peers } => {
                                ui.label("Timeout: ");
                                ui.horizontal(|ui| {
                                    if ui.selectable_label(false, format!("{:?}", key)).clicked() {
                                        ui.output_mut(|out| out.copied_text = format!("{:?}", key))
                                    }
                                });
                                ui.label("Peers:");
                                for peer in peers.iter() {
                                    if ui.selectable_label(false, peer.to_string()).clicked() {
                                        ui.output_mut(|out| out.copied_text = peer.to_string())
                                    }
                                }
                            }
                        },
                    }
                } else {
                    ui.label("This is invalid");
                }

                ui.separator();
                ui.label("Status: ");
                ui.label(format!("Requests: {}", stats.num_requests()));
                ui.label(format!("Sucesses: {}", stats.num_successes()));
                ui.label(format!("Failures: {}", stats.num_failures()));
                ui.label(format!("Pending: {}", stats.num_pending()));
                if let Some(duration) = stats.duration() {
                    ui.label(format!("Duration: {}", duration.as_secs_f32()));
                }
                if !step.last {
                    ui.spinner();
                }
            } else {
                ui.spinner();
            }
        } else {
            ui.spinner();
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
