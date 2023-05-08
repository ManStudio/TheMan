use std::{
    sync::{Arc, RwLock},
    time::Duration,
};

use eframe::egui;
use libp2p::{kad::kbucket::NodeStatus, Multiaddr, PeerId};

use crate::{
    logic::message::Message,
    save_state::{self, TheManSaveState},
    state::TheManState,
};

#[derive(Default)]
pub struct TheManGuiState {
    pub kademlia_status: Option<libp2p::swarm::NetworkInfo>,
    pub save: Option<TheManSaveState>,
    pub peers: Vec<(PeerId, NodeStatus, Vec<Multiaddr>)>,
}

pub struct TheMan {
    pub state: TheManGuiState,
    pub receiver: tokio::sync::mpsc::Receiver<Message>,
    pub sender: tokio::sync::mpsc::Sender<Message>,
    pub should_close: bool,
}

impl TheMan {
    pub fn new(
        receiver: tokio::sync::mpsc::Receiver<Message>,
        sender: tokio::sync::mpsc::Sender<Message>,
    ) -> Self {
        Self {
            state: TheManGuiState::default(),
            receiver,
            sender,
            should_close: false,
        }
    }

    pub fn process_events(&mut self) {
        while let Ok(message) = self.receiver.try_recv() {
            match message {
                Message::KademliaStatus(status) => self.state.kademlia_status = Some(status),
                Message::SaveResponse(res) => self.state.save = Some(res),
                Message::Peers(peers) => self.state.peers = peers,
                _ => {}
            }
        }
    }
}
impl eframe::App for TheMan {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.process_events();

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(kademlia_status) = &self.state.kademlia_status {
                egui::Window::new("Kademlia Status").show(ui.ctx(), |ui| {
                    ui.label(format!("Peers: {}", kademlia_status.num_peers()));
                    let conn = kademlia_status.connection_counters();
                    ui.label(format!("Connections: {}", conn.num_connections()));
                    ui.label(format!("Pending: {}", conn.num_pending()));
                    ui.label(format!("Pending incoming: {}", conn.num_pending_incoming()));
                    ui.label(format!("Pending outgoing: {}", conn.num_pending_outgoing()));
                    ui.label(format!("Established: {}", conn.num_established()));
                    ui.label(format!(
                        "Established incoming: {}",
                        conn.num_established_incoming()
                    ));
                    ui.label(format!(
                        "Established outgoing: {}",
                        conn.num_established_outgoing()
                    ));
                });
            }

            if ui.button("Bootstrap").clicked() {
                self.sender.try_send(Message::Bootstrap);
            }

            egui::Window::new("Peers")
                .resizable(true)
                .show(ui.ctx(), |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("Refresh").clicked() {
                            self.sender.try_send(Message::GetPeers);
                        }
                        ui.label(format!("Peers: {}", self.state.peers.len()));
                    });
                    let row_height = ui.text_style_height(&egui::TextStyle::Body);
                    egui::ScrollArea::both().show_rows(
                        ui,
                        row_height,
                        self.state.peers.len(),
                        |ui, range| {
                            for peer in &self.state.peers[range] {
                                ui.horizontal(|ui| {
                                    ui.label(format!("Id: {}", peer.0));
                                    ui.label(format!("Status: {:?}", peer.1));
                                    ui.label(format!("Adresses: {:?}", peer.2));
                                });
                            }
                        },
                    )
                });
        });
        ctx.request_repaint_after(Duration::from_secs(1) / 30)
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        println!("Send save");
        self.sender.try_send(Message::Save).unwrap();
        let save_state = loop {
            if let Some(save) = &self.state.save {
                break save;
            } else {
                self.process_events();
            }
        };
        storage.set_string("state", ron::to_string(save_state).unwrap());
        if self.should_close {
            self.sender.try_send(Message::ShutDown).unwrap();
        }
        log::debug!("Saved");
    }

    fn auto_save_interval(&self) -> std::time::Duration {
        Duration::from_secs(60)
    }

    fn on_close_event(&mut self) -> bool {
        self.should_close = true;
        true
    }
}
