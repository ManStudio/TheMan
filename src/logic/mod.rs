use std::time::{Duration, Instant};

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
    pub registration_query: Option<libp2p::kad::QueryId>,
}

impl TheManLogic {
    pub fn new(state: TheManState, sender: Sender<Message>, reciver: Receiver<Message>) -> Self {
        Self {
            state,
            sender,
            reciver,
            bootstrap: None,
            subscribed: Vec::new(),
            registration_query: None,
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
                        let mut hasher = libp2p::multihash::Sha2_256::default();
                        hasher.update(account.name.as_bytes());
                        let hash = hasher.finalize();
                        const SECS: u64 = 60 * 60 * 24 * 3;
                        let instant = Instant::now() + Duration::from_secs(SECS);
                        self.registration_query = account.swarm.behaviour_mut().kademlia.put_record(
                            libp2p::kad::Record {
                                key: libp2p::kad::RecordKey::new(&libp2p::bytes::Bytes::copy_from_slice(hash)),
                                value: account.peer_id.to_bytes(),
                                publisher: None,
                                expires: Some(instant),
                            },
                            libp2p::kad::Quorum::One,
                        ).map_or_else(|e|{eprintln!("Cannot register itself: {e:?}"); None}, Some);
                        account.expires = instant;
                        if let Some(acc) = self.state.accounts.get_mut(account.index) {
                            acc.expires = chrono::Utc::now()
                                + chrono::Duration::from_std(
                                    account.expires.duration_since(Instant::now()),
                                )
                                .unwrap_or_else(|_| chrono::Duration::zero())
                        }
                        let _ = self.sender.try_send(Message::Accounts(self.state.accounts.clone()));
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
