use crate::state::TheManState;
use libp2p::{
    futures::StreamExt,
    gossipsub::{IdentTopic, TopicHash},
};
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
}

impl TheManLogic {
    pub fn new(state: TheManState, sender: Sender<Message>, reciver: Receiver<Message>) -> Self {
        Self {
            state,
            sender,
            reciver,
            bootstrap: None,
            subscribed: Vec::new(),
        }
    }

    pub async fn run(mut self) {
        let _ = self
            .sender
            .try_send(Message::Accounts(self.state.accounts.clone()));

        loop {
            if let Some(account) = &mut self.state.account {
                tokio::select! {
                    Some(message) = self.reciver.recv() => {
                        if let Message::ShutDown = &message {break}else{
                            self.on_message(message).await;
                        }

                    },
                    event = account.swarm.select_next_some() => {
                        self.on_event(event).await;
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
