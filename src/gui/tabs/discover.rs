use libp2p::PeerId;

use crate::logic::message::Message;

use super::Tab;

#[derive(Default, Clone)]
pub struct TabDiscover {
    id: usize,
    peer_id: String,
    name: String,
    waiting_for_peer: Option<PeerId>,
    waiting_for_name: Option<String>,
}

impl Tab for TabDiscover {
    fn name(&self) -> &str {
        "Discover"
    }

    fn update(
        &mut self,
        ui: &mut eframe::egui::Ui,
        state: &mut crate::gui::TheManGuiState,
    ) -> Option<String> {
        ui.horizontal(|ui| {
            ui.label("Name: ");
            ui.text_edit_singleline(&mut self.name);
            ui.separator();
            if ui.button("Search").clicked() {
                self.waiting_for_name = Some(self.name.clone());
                state.send(Message::SearchByName(self.name.clone()));
            }
        });

        ui.separator();

        ui.horizontal(|ui| {
            ui.label("PeerId: ");
            ui.text_edit_singleline(&mut self.peer_id);
            ui.separator();
            if ui.button("Search").clicked() {
                if let Ok(peer_id) = self.peer_id.parse::<PeerId>() {
                    self.waiting_for_peer = Some(peer_id);
                    state.send(Message::SearchPeerId(peer_id));
                } else {
                    eprintln!("Cannot Parse PeerId");
                }
            }
        });

        ui.separator();

        //
        // Start search by name
        //

        let Some(name) = &self.waiting_for_name else{return None};
        if let Some(query_id) = state.query_id_for_names.get(name) {
            ui.label(format!("QueryId: {:?}", query_id));
            if let Some((res, stats, step)) = state.kademlia_query_progress.get(query_id) {
                ui.label("Search by Name Results: ");

                if let libp2p::kad::QueryResult::GetRecord(res) = res {
                    match res {
                        Ok(finded) => match finded {
                            libp2p::kad::GetRecordOk::FoundRecord(finded) => {
                                if let Some(from) = finded.peer {
                                    if ui
                                        .selectable_label(false, format!("From: {}", from))
                                        .clicked()
                                    {
                                        ui.output_mut(|out| out.copied_text = from.to_string())
                                    }
                                }

                                if let Some(original_publisher) = finded.record.publisher {
                                    if ui
                                        .selectable_label(
                                            false,
                                            format!("Publisher: {}", original_publisher),
                                        )
                                        .clicked()
                                    {
                                        ui.output_mut(|out| {
                                            out.copied_text = original_publisher.to_string()
                                        })
                                    }

                                    if original_publisher.to_bytes() == finded.record.value {
                                        ui.label("You are safe!");
                                    } else {
                                        ui.label("👀👀👀 You are not safe!!! 👀👀👀");
                                    }
                                }

                                if ui
                                    .selectable_label(
                                        false,
                                        format!("Key: {:?}", finded.record.key),
                                    )
                                    .clicked()
                                {
                                    ui.output_mut(|out| {
                                        out.copied_text = format!("{:?}", finded.record.key)
                                    })
                                }

                                let expires = finded.record.expires.map(|instant| {
                                    chrono::Utc::now()
                                        + chrono::Duration::from_std(
                                            instant.duration_since(std::time::Instant::now()),
                                        )
                                        .unwrap_or(chrono::Duration::zero())
                                });

                                if let Some(expires) = expires {
                                    let expire = expires.format("%d/%m/%Y %H:%M").to_string();
                                    if ui
                                        .selectable_label(false, format!("Expire: {}", expire))
                                        .clicked()
                                    {
                                        ui.output_mut(|out| out.copied_text = expire.clone())
                                    }
                                } else {
                                    ui.label("Expires: Never! Until nodes forgets him!");
                                }

                                ui.separator();

                                if let Ok(peer_id) = PeerId::from_bytes(&finded.record.value) {
                                    if ui
                                        .selectable_label(
                                            false,
                                            format!("Was found his Id is: {}", peer_id),
                                        )
                                        .clicked()
                                    {
                                        ui.output_mut(|out| out.copied_text = peer_id.to_string())
                                    }
                                    self.peer_id = peer_id.to_string();
                                    self.waiting_for_peer = Some(peer_id);
                                    let _ = state.sender.try_send(Message::SearchPeerId(peer_id));
                                    self.waiting_for_name = None;
                                    ui.label(format!("We will try to connect to: {}", peer_id));
                                } else {
                                    ui.label("This is invalid but here is the information!");

                                    match String::from_utf8(finded.record.value.clone()) {
                                        Ok(value) => {
                                            if ui
                                                .selectable_label(
                                                    false,
                                                    format!("String: {}", value),
                                                )
                                                .clicked()
                                            {
                                                ui.output_mut(|out| out.copied_text = value.clone())
                                            }
                                        }
                                        Err(err) => {
                                            let bytes = err.as_bytes();

                                            if ui
                                                .selectable_label(
                                                    false,
                                                    format!("Bytes: {:?}", bytes),
                                                )
                                                .clicked()
                                            {
                                                ui.output_mut(|out| {
                                                    out.copied_text = format!("{:?}", bytes)
                                                })
                                            }
                                        }
                                    }
                                }
                            }
                            libp2p::kad::GetRecordOk::FinishedWithNoAdditionalRecord { .. } => {
                                ui.label("Nothing was found!");
                            }
                        },
                        Err(err) => match err {
                            libp2p::kad::GetRecordError::NotFound { .. } => {
                                ui.label("Was not found!");
                            }
                            libp2p::kad::GetRecordError::QuorumFailed { .. } => {
                                ui.label("Was never added to the network!");
                            }
                            libp2p::kad::GetRecordError::Timeout { .. } => {
                                ui.label("Timeout!");
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
                ui.label(format!("Step: {}", step.count));
                if !step.last {
                    ui.spinner();
                }
            } else {
                ui.spinner();
            }
        } else {
            ui.spinner();
        }

        ui.separator();

        //
        // Search by peer_id
        //

        let Some(peer_id) = &self.waiting_for_peer else{return None};
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

                            ui.label(format!(
                                "You are connected! to: {}",
                                finded.peers.first().unwrap()
                            ));
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
                ui.label(format!("Step: {}", step.count));
                if !step.last {
                    ui.spinner();
                }
            } else {
                ui.spinner();
            }
        } else {
            ui.spinner();
        }

        //
        // End search by peer
        //

        None
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
