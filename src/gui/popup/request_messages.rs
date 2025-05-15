use std::collections::BTreeSet;

use the_man::{base64_deserialize, base64_serialize, protocol::Ticket};

use super::Popup;

use eframe::egui;

pub struct PopupRequestMessages {
    the_man: the_man::TheMan,
    tickets: BTreeSet<Ticket>,

    input: String,
}

impl PopupRequestMessages {
    pub fn new(the_man: the_man::TheMan) -> Self {
        Self {
            the_man,
            tickets: Default::default(),
            input: Default::default(),
        }
    }
}

impl Popup for PopupRequestMessages {
    fn show(
        &mut self,
        ui: &mut eframe::egui::Ui,
        context: &mut crate::gui::Context,
        _account: &mut crate::Account,
    ) -> bool {
        ui.horizontal(|ui| {
            egui::TextEdit::singleline(&mut self.input)
                .hint_text("Ticket Message")
                .show(ui);
            match base64_deserialize::<Ticket>(&self.input) {
                Ok(ticket) => {
                    if ui.button("Add").clicked() {
                        self.input.clear();
                        self.tickets.insert(ticket);
                    }
                }
                Err(err) => {
                    if !self.input.is_empty() {
                        ui.colored_label(egui::Color32::RED, format!("Invalid Ticket: {err:?}"));
                    }
                }
            }
        });

        if !self.tickets.is_empty() && ui.button("Request Messages").clicked() {
            let the_man = self.the_man.clone();
            let tickets = self.tickets.iter().cloned().collect::<Vec<_>>();

            context.add_task(Box::pin(async move {
                for ticket in tickets {
                    the_man.recover(ticket).await;
                }
            }));

            return true;
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            let mut to_remove = Vec::new();
            for ticket in self.tickets.iter() {
                if ui
                    .button(base64_serialize(ticket).unwrap().to_string())
                    .clicked()
                {
                    to_remove.push(ticket.clone());
                }
            }
            for to_remove in to_remove {
                self.tickets.remove(&to_remove);
            }
        });
        false
    }

    fn name(&self) -> String {
        String::from("Request Messages")
    }
}
