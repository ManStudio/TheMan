use eframe::egui;

use crate::save_state::ChannelType;

use super::Tab;

#[derive(Default)]
pub struct TabChannels {
    id: usize,

    channel_type: ChannelType,
    channel_name: String,
}

impl Tab for TabChannels {
    fn name(&self) -> &str {
        "Channels"
    }

    fn update(
        &mut self,
        ui: &mut eframe::egui::Ui,
        state: &mut crate::gui::TheManGuiState,
    ) -> Option<String> {
        let mut script = String::new();

        ui.label("Channels");
        for channel in state.channels.iter() {
            match &channel.1 {
                ChannelType::Message => {
                    if ui
                        .selectable_label(false, format!(" {}", channel.0))
                        .clicked()
                    {
                        script = format!("o7,{}", channel.0);
                    }
                }
                ChannelType::Voice => {
                    if ui
                        .selectable_label(false, format!("響 {}", channel.0))
                        .clicked()
                    {
                        script = format!("o11,{}", channel.0);
                    }
                }
            }
        }

        ui.separator();
        ui.label("Add Channel");
        egui::ComboBox::new("channel_type", "Select Channel Type")
            .selected_text(match self.channel_type {
                ChannelType::Message => "Message",
                ChannelType::Voice => "Voice",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.channel_type, ChannelType::Message, "Message");
                ui.selectable_value(&mut self.channel_type, ChannelType::Voice, "Voice");
            });
        ui.horizontal(|ui| {
            ui.label("Channel Name: ");
            ui.text_edit_singleline(&mut self.channel_name)
        });

        if ui.button("Add").clicked() {
            state
                .channels
                .push((self.channel_name.clone(), self.channel_type.clone()));
        }

        if script.is_empty() {
            None
        } else {
            Some(script)
        }
    }

    fn hidden(&self) -> bool {
        false
    }

    fn recive(&mut self, _message: String) {}

    fn clone_box(&self) -> Box<dyn Tab> {
        Box::<Self>::default()
    }

    fn id(&self) -> usize {
        self.id
    }

    fn set_id(&mut self, id: usize) {
        self.id = id
    }
}
