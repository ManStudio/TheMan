use std::time::Duration;

use eframe::egui;
use libp2p::{kad::kbucket::NodeStatus, Multiaddr, PeerId};

use crate::{logic::message::Message, save_state::TheManSaveState, state::PeerStatus};

mod tabs;
use tabs::*;

pub struct TheManGuiState {
    pub kademlia_status: Option<libp2p::swarm::NetworkInfo>,
    pub save: Option<TheManSaveState>,
    pub bootnodes: Vec<(PeerId, NodeStatus, Vec<Multiaddr>)>,
    pub peers: Vec<(PeerId, PeerStatus)>,
    pub receiver: tokio::sync::mpsc::Receiver<Message>,
    pub sender: tokio::sync::mpsc::Sender<Message>,
}

pub struct TheMan {
    pub state: TheManGuiState,
    pub tab_manager: TabManager,
    pub should_close: bool,
    pub one_time: bool,
}

impl TheMan {
    pub fn new(
        receiver: tokio::sync::mpsc::Receiver<Message>,
        sender: tokio::sync::mpsc::Sender<Message>,
    ) -> Self {
        let mut tab_manager = TabManager::new();
        tab_manager.register::<TabSwarmStatus>();
        tab_manager.register::<TabBootNodes>();
        tab_manager.register::<TabPeers>();

        tab_manager.execute("o0;o1;o2");

        Self {
            state: TheManGuiState {
                kademlia_status: None,
                save: None,
                bootnodes: Vec::new(),
                peers: Vec::new(),
                receiver,
                sender,
            },
            should_close: false,
            one_time: false,
            tab_manager,
        }
    }

    pub fn process_events(&mut self) {
        while let Ok(message) = self.state.receiver.try_recv() {
            match message {
                Message::SwarmStatus(status) => {
                    self.state.kademlia_status = Some(status);
                    let _ = self.state.sender.try_send(Message::GetBootNodes);
                    let _ = self.state.sender.try_send(Message::GetPeers);
                }
                Message::SaveResponse(res) => self.state.save = Some(res),
                Message::BootNodes(nodes) => self.state.bootnodes = nodes,
                Message::Peers(peers) => self.state.peers = peers,
                _ => {}
            }
        }
    }
}

impl eframe::App for TheMan {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.one_time {
            self.one_time = true;
        }

        self.process_events();

        egui::CentralPanel::default().show(ctx, |ui| {
            self.tab_manager.ui(ui, &mut self.state);
        });

        ctx.request_repaint_after(Duration::from_secs(1) / 30)
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        println!("Send save");
        self.state.sender.try_send(Message::Save).unwrap();
        let save_state = loop {
            if let Some(save) = &self.state.save {
                break save;
            } else {
                self.process_events();
            }
        };
        storage.set_string("state", ron::to_string(save_state).unwrap());
        if self.should_close {
            self.state.sender.try_send(Message::ShutDown).unwrap();
        }
        log::debug!("Saved");
    }

    fn auto_save_interval(&self) -> std::time::Duration {
        Duration::MAX
    }

    fn on_close_event(&mut self) -> bool {
        self.should_close = true;
        true
    }
}
