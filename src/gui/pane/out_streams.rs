use std::collections::HashMap;

use super::Pane;

use eframe::egui;

#[derive(Default)]
pub struct PaneOutStreams {}

impl Pane for PaneOutStreams {
    fn name(&self, account: &crate::Account) -> String {
        String::from("Out Streams")
    }

    fn ui(
        &mut self,
        ui: &mut eframe::egui::Ui,
        context: &mut crate::gui::Context,
        the_man: &mut the_man::TheMan,
        account: &mut crate::Account,
    ) {
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                tokio::runtime::Handle::current().block_on(async {
                    let inputs = the_man.input_streams().await;
                    let mut inputs_names = HashMap::<usize, String>::default();
                    for id in inputs.iter() {
                        if let Some(name) = the_man.stream_name(*id).await {
                            inputs_names.insert(*id, name);
                        }
                    }
                    for id in the_man.output_streams().await {
                        let name = the_man
                            .stream_name(id)
                            .await
                            .unwrap_or_else(|| String::from("Unknown"));
                        let connections = the_man.output_stream_connections(id).await;

                        let mut disconnect_from = None;
                        let mut connect_to = None;

                        egui::CollapsingHeader::new(format!("{id}-{name}")).show(ui, |ui| {
                            ui.heading("Connections");
                            for connection in connections.iter() {
                                if ui
                                    .button(format!(
                                        "{connection}-{}",
                                        inputs_names
                                            .get(connection)
                                            .map(String::as_str)
                                            .unwrap_or("Unknown")
                                    ))
                                    .clicked()
                                {
                                    disconnect_from = Some(connection);
                                }
                            }

                            egui::menu::menu_button(ui, "Connect", |ui| {
                                for input in inputs.iter() {
                                    if ui
                                        .small_button(format!(
                                            "{input}-{}",
                                            inputs_names
                                                .get(input)
                                                .map(String::as_str)
                                                .unwrap_or("Unknown")
                                        ))
                                        .clicked()
                                    {
                                        connect_to = Some(*input);
                                    }
                                }
                            });
                        });

                        if let Some(connection) = disconnect_from {
                            the_man.output_stream_disconnect(id, *connection).await;
                        }
                        if let Some(connect_to) = connect_to {
                            the_man.output_stream_connect(id, connect_to).await;
                        }
                    }
                });
            });
    }
}
