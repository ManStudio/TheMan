use iroh_blobs::Hash;
use the_man::base64_serialize;
use tokio::sync::oneshot;

use eframe::egui;

use crate::{
    Account,
    gui::{
        component::with_name,
        popup::{PopupCreateConversation, PopupRequestMessages},
    },
};

use super::{Pane, PaneConversation};

#[derive(Default)]
pub struct PaneConversations {
    conversations: Vec<Hash>,
    conversations_receiver: Option<oneshot::Receiver<Vec<Hash>>>,

    conversation_refreshes: usize,
}

impl Pane for PaneConversations {
    fn name(&self, _account: &Account) -> String {
        String::from("Conversations")
    }

    fn ui(
        &mut self,
        ui: &mut eframe::egui::Ui,
        context: &mut crate::gui::Context,
        the_man: &mut the_man::TheMan,
        account: &mut crate::Account,
    ) {
        if let Some(mut receiver) = self.conversations_receiver.take() {
            if let Ok(conversations) = receiver.try_recv() {
                self.conversations = conversations;
            } else {
                self.conversations_receiver = Some(receiver);
            }
        }

        if self.conversations_receiver.is_none()
            && context.conversation_refreshes != self.conversation_refreshes
        {
            self.conversation_refreshes = context.conversation_refreshes;
            let (sender, receiver) = oneshot::channel();
            context.add_task(Box::pin(task_get_conversations(the_man.clone(), sender)));
            self.conversations_receiver = Some(receiver);
        }

        ui.horizontal(|ui| {
            if ui.button("Create").clicked() {
                context.add_popup(PopupCreateConversation::new(the_man.clone()));
            }
            if ui.button("Request Messages").clicked() {
                context.add_popup(PopupRequestMessages::new(the_man.clone()));
            }
        });

        egui::ScrollArea::vertical().show(ui, |ui| {
            for conversation in self.conversations.iter() {
                ui.horizontal(|ui| {
                    if ui.button("Open").clicked() {
                        let mut tab = PaneConversation::default();
                        tab.set_data(base64_serialize(conversation).unwrap());
                        context.add_tab(tab);
                    }
                    with_name(conversation, ui, context, account, |_| {});
                });
            }
        });
    }
}

async fn task_get_conversations(the_man: the_man::TheMan, sender: oneshot::Sender<Vec<Hash>>) {
    _ = sender.send(the_man.raw_conversations().await.unwrap());
}
