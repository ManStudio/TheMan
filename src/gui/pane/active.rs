use crate::gui::ToHash;

use super::Pane;

use eframe::egui;
use the_man::base64_serialize;

#[derive(Default)]
pub struct PaneActive {}

impl Pane for PaneActive {
    fn name(&self, account: &crate::Account) -> String {
        String::from("Active")
    }

    fn ui(
        &mut self,
        ui: &mut eframe::egui::Ui,
        context: &mut crate::gui::Context,
        the_man: &mut the_man::TheMan,
        account: &mut crate::Account,
    ) {
        egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            for (i, info) in the_man.remote_infos().iter().enumerate() {
                ui.group(|ui| {
                    egui::CollapsingHeader::new(
                        account
                            .known_as
                            .get(&ToHash::hash(&info.node_id))
                            .cloned()
                            .unwrap_or_else(|| {
                                base64_serialize(&info.node_id).unwrap().to_string()
                            }),
                    )
                    .show(ui, |ui| {
                        if let Some(relay) = &info.relay_url {
                            egui::CollapsingHeader::new("Relay").show(ui, |ui| {
                                ui.label(format!("URL: {}", relay.relay_url));
                                if let Some(last_alive) = relay.last_alive {
                                    ui.label(format!("Last Alive: {}", last_alive.as_secs_f32()));
                                } else {
                                    ui.label("Last Alive: None");
                                }
                                if let Some(latency) = relay.latency {
                                    ui.label(format!("Latency: {}", latency.as_secs_f32()));
                                } else {
                                    ui.label("Latency: None");
                                }
                            });
                        } else {
                            ui.label("Relay: None");
                        }

                        ui.heading("Addresses");
                        for address in info.addrs.iter() {
                            egui::CollapsingHeader::new(format!("{}", address.addr)).show(
                                ui,
                                |ui| {
                                    if let Some(latency) = address.latency {
                                        ui.label(format!("Latency: {}", latency.as_secs_f32()));
                                    } else {
                                        ui.label("Latency: None");
                                    }

                                    if let Some((duration, control)) = address.last_control {
                                        ui.label(format!(
                                            "Last Control: {control}: {}",
                                            duration.as_secs_f32()
                                        ));
                                    } else {
                                        ui.label("Last Control: None");
                                    }

                                    if let Some(last_payload) = address.last_payload {
                                        ui.label(format!(
                                            "Last payload: {}",
                                            last_payload.as_secs_f32()
                                        ));
                                    } else {
                                        ui.label("Last Payload: None");
                                    }

                                    if let Some(last_alive) = address.last_alive {
                                        ui.label(format!(
                                            "Last Alive: {}",
                                            last_alive.as_secs_f32()
                                        ));
                                    } else {
                                        ui.label("Last Alive: None");
                                    }

                                    ui.heading("Sources");
                                    for (source, duration) in address.sources.iter() {
                                        ui.label(format!("{source}: {}", duration.as_secs_f32()));
                                    }
                                    ui.separator();
                                },
                            );
                        }
                        ui.separator();

                        ui.label(format!("Connection Type: {}", info.conn_type));

                        if let Some(latency) = info.latency {
                            ui.label(format!("Latency: {}", latency.as_secs_f32()));
                        } else {
                            ui.label("Latency: None");
                        }

                        if let Some(last_used) = info.last_used {
                            ui.label(format!("Last Used: {}", last_used.as_secs_f32()));
                        } else {
                            ui.label("Last Used: None");
                        }
                    });
                });
            }
        });
    }
}
