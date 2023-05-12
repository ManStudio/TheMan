use std::time::Instant;

use crate::state::TheManState;
use libp2p::{futures::StreamExt, gossipsub::TopicHash, multihash::Hasher};
use tokio::sync::mpsc::{Receiver, Sender};

use self::message::Message;

pub mod message;
pub mod network;

pub struct TheManLogic {
    pub state: TheManState,
    pub sender: Sender<Message>,
    pub reciver: Receiver<Message>,
    pub bootstrap: Option<libp2p::kad::QueryId>,
    pub subscribed: Vec<TopicHash>,
    pub registration_query: Option<(libp2p::kad::QueryId, Instant)>,
    pub registration_step_1_query: Option<(libp2p::kad::QueryId, Vec<u8>)>,
    pub egui_ctx: eframe::egui::Context,
}

impl TheManLogic {
    pub fn new(
        state: TheManState,
        sender: Sender<Message>,
        reciver: Receiver<Message>,
        egui_ctx: eframe::egui::Context,
    ) -> Self {
        Self {
            state,
            sender,
            reciver,
            bootstrap: None,
            subscribed: Vec::new(),
            registration_query: None,
            egui_ctx,
            registration_step_1_query: None,
        }
    }

    pub async fn run(mut self) {
        let _ = self
            .sender
            .try_send(Message::Accounts(self.state.accounts.clone()));

        loop {
            if let Some(account) = &mut self.state.account {
                let renew_account = tokio::time::Instant::from_std(account.expires);
                tokio::select! {
                    Some(message) = self.reciver.recv() => {
                        if let Message::ShutDown = &message {break}else{
                            self.on_message(message).await;
                        }

                    },
                    event = account.swarm.select_next_some() => {
                        self.on_event(event).await;
                    }
                    _ = tokio::time::sleep_until(renew_account) => {
                        if self.registration_step_1_query.is_some() && self.registration_step_1_query.is_some(){continue}
                        if 600 > account.swarm.network_info().num_peers(){
                            continue;
                        }
                        let mut hasher = libp2p::multihash::Sha2_256::default();
                        hasher.update(account.name.as_bytes());
                        let hash = hasher.finalize();
                        self.registration_step_1_query = Some((account.swarm.behaviour_mut().kademlia.get_closest_peers(hash.to_vec()), hash.to_vec()));
                    }
                }
            } else {
                tokio::select! {
                    Some(message) = self.reciver.recv() => {
                        if let Message::ShutDown = &message {break}else{
                            self.on_message(message).await;
                        }
                    }
                }
            }
        }
        println!("Worker thread exited!");
    }
}
