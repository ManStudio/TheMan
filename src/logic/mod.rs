use crate::state::TheManState;
use libp2p::futures::StreamExt;
use tokio::sync::mpsc::{Receiver, Sender};

use self::message::Message;

pub mod message;
pub mod network;

pub struct TheManLogic {
    pub state: TheManState,
    pub sender: Sender<Message>,
    pub reciver: Receiver<Message>,
    pub bootstrap: Option<libp2p::kad::QueryId>,
}

impl TheManLogic {
    pub fn new(state: TheManState, sender: Sender<Message>, reciver: Receiver<Message>) -> Self {
        Self {
            state,
            sender,
            reciver,
            bootstrap: None,
        }
    }

    pub async fn run(mut self) {
        let _ = self
            .sender
            .try_send(Message::SwarmStatus(self.state.swarm.network_info()));

        let _ = self
            .state
            .swarm
            .listen_on("/ip4/0.0.0.0/tcp/0".parse().unwrap());

        self.bootstrap = Some(
            self.state
                .swarm
                .behaviour_mut()
                .kademlia
                .bootstrap()
                .unwrap(),
        );

        loop {
            tokio::select! {
                Some(message) = self.reciver.recv() => {
                    if let Message::ShutDown = &message {break}else{
                        self.on_message(message).await;
                    }

                },
                event = self.state.swarm.select_next_some() => {
                    self.on_event(event).await;
                }
            }
        }
        println!("Worker thread exited!");
    }
}
