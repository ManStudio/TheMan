use std::time::Instant;

use crate::state::TheManState;
use libp2p::{futures::StreamExt, gossipsub::TopicHash, multihash::Hasher};
use tokio::sync::mpsc::{Receiver, Sender};

use self::message::Message;

pub mod audio;
pub mod message;
pub mod network;

pub struct TheManLogic {
    pub state: TheManState,
    pub sender: Sender<Message>,
    pub reciver: Receiver<Message>,
    pub audio_sender: Sender<Message>,
    pub audio_receiver: Receiver<Message>,
    pub bootstrap: Option<libp2p::kad::QueryId>,
    pub bootstraping: bool,
    pub subscribed: Vec<TopicHash>,
    pub registration_query: Option<(libp2p::kad::QueryId, Instant)>,
    pub registration_step_1_query: Option<(libp2p::kad::QueryId, Vec<u8>)>,
    pub audio_counter: usize,
}

impl TheManLogic {
    pub fn new(
        state: TheManState,
        sender: Sender<Message>,
        reciver: Receiver<Message>,
        audio_sender: Sender<Message>,
        audio_receiver: Receiver<Message>,
    ) -> Self {
        Self {
            state,
            sender,
            reciver,
            bootstrap: None,
            subscribed: Vec::new(),
            registration_query: None,
            registration_step_1_query: None,
            audio_sender,
            audio_receiver,
            bootstraping: true,
            audio_counter: 0,
        }
    }

    pub async fn run(mut self) {
        let _ = self
            .sender
            .send(Message::Accounts(self.state.accounts.clone()))
            .await;

        let _ = self
            .audio_sender
            .send(Message::Audio(message::AudioMessage::CreateInputChannel {
                id: 0,
                codec: "opus".into(),
            }))
            .await;

        self.audio_counter += 1;

        loop {
            if let Some(account) = &mut self.state.account {
                let mut renew_account = tokio::time::Instant::from_std(account.expires);
                tokio::select! {
                    Some(message) = self.reciver.recv() => {
                        if let Message::ShutDown = &message {
                            let _ = self.audio_sender.send(Message::ShutDown).await;
                            break
                        }else{
                            self.on_message(message).await;
                        }

                    },
                    Some(message) = self.audio_receiver.recv() => {
                        self.on_audio_message(message).await;
                    }
                    event = account.swarm.select_next_some() => {
                        self.on_event(event).await;
                    }
                    _ = tokio::time::sleep_until(renew_account) => {
                        if account.auto_renew{
                        if self.registration_step_1_query.is_some() && self.registration_step_1_query.is_some(){continue}
                        if 600 > account.swarm.network_info().num_peers(){
                            continue;
                        }
                            let mut hasher = libp2p::multihash::Sha2_256::default();
                            hasher.update(account.name.as_bytes());
                            let hash = hasher.finalize();
                            self.registration_step_1_query = Some((account.swarm.behaviour_mut().kademlia.get_closest_peers(hash.to_vec()), hash.to_vec()));
                        }else{
                            renew_account = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
                        }
                    }
                }
            } else {
                tokio::select! {
                    Some(message) = self.reciver.recv() => {
                        if let Message::ShutDown = &message {
                            let _ = self.audio_sender.send(Message::ShutDown).await;
                            break
                        }else{
                            self.on_message(message).await;
                        }
                    }
                    Some(message) = self.audio_receiver.recv() => {
                        self.on_audio_message(message).await;
                    }
                }
            }
        }
        println!("Worker thread exited!");
    }
}
