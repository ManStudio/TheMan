use std::{collections::HashMap, sync::Arc};

use super::Pane;

use eframe::egui;

#[derive(Default)]
pub struct PaneOutStreams {}

impl Pane for PaneOutStreams {
    fn name(&self, _account: &crate::Account) -> String {
        String::from("Out Streams")
    }

    fn ui(
        &mut self,
        ui: &mut eframe::egui::Ui,
        context: &mut crate::gui::Context,
        the_man: &mut the_man::TheMan,
        _account: &mut crate::Account,
    ) {
        if ui.button("Start Screen Capture").clicked() {
            let the_man = the_man.clone();
            context.add_task(Box::pin(async move {
                the_man.screen_share().await;
            }));
        }

        ui.separator();

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

                        let last_frame = the_man.stream_last_video_frame(id).await;

                        let mut disconnect_from = None;
                        let mut connect_to = None;

                        egui::CollapsingHeader::new(format!("{id}-{name}")).show(ui, |ui| {
                            if let Some((width, height, bytes)) = last_frame {
                                egui::CollapsingHeader::new("Video Preview")
                                    .id_salt(id)
                                    .show(ui, |ui| {
                                        ui.ctx().request_repaint();

                                        let color_image = egui::ColorImage::from_rgba_unmultiplied(
                                            [width as usize, height as usize],
                                            &bytes,
                                        );
                                        let texture = ui.ctx().load_texture(
                                            format!("video_stream: {id}"),
                                            color_image,
                                            egui::TextureOptions::LINEAR,
                                        );

                                        let (rect, _) = ui.allocate_exact_size(
                                            egui::vec2(480., 270.),
                                            egui::Sense::empty(),
                                        );

                                        egui::paint_texture_at(
                                            ui.painter(),
                                            rect,
                                            &egui::ImageOptions::default(),
                                            &egui::load::SizedTexture::from_handle(&texture),
                                        );
                                    });
                            }
                            if ui.button("Open Preview Window").clicked() {
                                struct WindowTest {
                                    should_close: bool,
                                    receiver: platform::Receiver<(u32, u32, Arc<[u8]>)>,
                                    texture: Option<egui::TextureHandle>,
                                    id: usize,
                                }
                                impl crate::gui::Window for WindowTest {
                                    fn builder(&self) -> egui::ViewportBuilder {
                                        egui::ViewportBuilder::default()
                                    }
                                    fn show(&mut self, ctx: &egui::Context) {
                                        if ctx.input(|i| i.viewport().close_requested()) {
                                            self.should_close = true
                                        }
                                        ctx.request_repaint();
                                        egui::CentralPanel::default().show(ctx, |ui| {
                                            let mut last = None;
                                            while let Some(some) = self.receiver.try_recv() {
                                                last = Some(some)
                                            }

                                            if self.receiver.is_closed() {
                                                self.should_close = true;
                                            }

                                            if let Some((width, height, bytes)) = last {
                                                let color_image =
                                                    egui::ColorImage::from_rgba_unmultiplied(
                                                        [width as usize, height as usize],
                                                        &bytes,
                                                    );
                                                let texture = ui.ctx().load_texture(
                                                    format!("video_stream: preview {}", self.id),
                                                    color_image,
                                                    egui::TextureOptions::LINEAR,
                                                );
                                                self.texture = Some(texture);
                                            }
                                            if let Some(texture) = &self.texture {
                                                let (rect, _) = ui.allocate_exact_size(
                                                    ui.available_size(),
                                                    egui::Sense::empty(),
                                                );

                                                egui::paint_texture_at(
                                                    ui.painter(),
                                                    rect,
                                                    &egui::ImageOptions::default(),
                                                    &egui::load::SizedTexture::from_handle(texture),
                                                );
                                            }
                                        });
                                    }
                                    fn should_close(&self) -> bool {
                                        self.should_close
                                    }
                                }

                                let (sender, receiver) = platform::channel();
                                tokio::task::block_in_place(|| {
                                    tokio::runtime::Handle::current().block_on(async {
                                        let new_id = the_man
                                            .input_stream_video_create(
                                                "Preview window",
                                                tokio::spawn(std::future::pending()),
                                                sender,
                                            )
                                            .await;
                                        the_man.output_stream_connect(id, new_id).await;
                                    });
                                });

                                context.add_window(WindowTest {
                                    should_close: false,
                                    receiver,
                                    texture: None,
                                    id,
                                })
                            }
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

                            ui.menu_button("Connect", |ui| {
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
