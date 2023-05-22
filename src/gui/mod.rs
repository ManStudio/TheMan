use std::{collections::HashMap, time::Duration};

use eframe::egui;
use libp2p::{
    gossipsub::TopicHash,
    kad::{kbucket::NodeStatus, ProgressStep, QueryId, QueryResult, QueryStats},
    swarm::AddressRecord,
    Multiaddr, PeerId,
};

use crate::{
    logic::message::Message,
    save_state::{Account, Friend, TheManSaveState},
    state::PeerStatus,
};

mod tabs;
use tabs::*;

pub struct TheManGuiState {
    pub kademlia_status: Option<libp2p::swarm::NetworkInfo>,
    pub save: Option<Option<TheManSaveState>>,
    pub bootnodes: Vec<(PeerId, NodeStatus, Vec<Multiaddr>)>,
    pub peers: Vec<(PeerId, PeerStatus)>,
    pub peer_id: Option<PeerId>,
    pub name: Option<String>,
    pub receiver: tokio::sync::mpsc::Receiver<Message>,
    pub sender: tokio::sync::mpsc::Sender<Message>,
    pub adresses: Vec<AddressRecord>,
    pub accounts: Vec<Account>,
    pub kademlia_query_progress: HashMap<QueryId, (QueryResult, QueryStats, ProgressStep)>,
    pub query_id_for_key: HashMap<Vec<u8>, QueryId>,
    pub query_id_for_record: HashMap<Vec<u8>, QueryId>,
    pub messages: HashMap<TopicHash, Vec<libp2p::gossipsub::Message>>,
    pub subscribers: HashMap<TopicHash, Vec<PeerId>>,
    pub voice_connected: HashMap<String, HashMap<PeerId, bool>>,
    pub friends: Vec<Friend>,
    pub register_names: HashMap<PeerId, String>,
    pub bootstraping: bool,
}

impl TheManGuiState {
    pub fn send(&mut self, message: Message) {
        let _ = self.sender.try_send(message);
    }
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
        tab_manager.register::<TabSwarmStatus>(); // 0
        tab_manager.register::<TabBootNodes>(); // 1
        tab_manager.register::<TabPeers>(); // 2
        tab_manager.register::<TabMySelf>(); // 3
        tab_manager.register::<TabAccounts>(); // 4
        tab_manager.register::<TabDiscover>(); // 5
        tab_manager.register::<TabAccount>(); // 6
        tab_manager.register::<TabMessageChannel>(); // 7
        tab_manager.register::<TabChannels>(); // 8
        tab_manager.register::<TabQuery>(); // 9
        tab_manager.register::<TabQuerys>(); // 10
        tab_manager.register::<TabVoiceChannel>(); // 11

        tab_manager.execute("o0;o1;o2;o3;o4");

        Self {
            state: TheManGuiState {
                kademlia_status: None,
                save: None,
                bootnodes: Vec::new(),
                peers: Vec::new(),
                receiver,
                sender,
                peer_id: None,
                accounts: Vec::new(),
                adresses: Vec::new(),
                kademlia_query_progress: HashMap::new(),
                query_id_for_key: HashMap::new(),
                messages: HashMap::new(),
                subscribers: HashMap::new(),
                name: None,
                query_id_for_record: HashMap::new(),
                bootstraping: true,
                voice_connected: HashMap::new(),
                friends: Vec::new(),
                register_names: HashMap::new(),
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
                Message::AccountActivate(account_index, peer_id) => {
                    if let Some(account) = self.state.accounts.get(account_index) {
                        self.state.name = Some(account.name.clone());
                    }
                    self.state.peer_id = Some(peer_id)
                }
                Message::Accounts(accounts) => self.state.accounts = accounts,
                Message::Adresses(adresses) => self.state.adresses = adresses,
                Message::ResSearchForKey(key, query_id) => {
                    self.state.query_id_for_key.insert(key, query_id);
                }
                Message::KademliaQueryProgress(query_id, result, stats, step) => {
                    self.state
                        .kademlia_query_progress
                        .insert(query_id, (result, stats, step));
                }
                Message::ResSearchForRecord(key, query_id) => {
                    self.state.query_id_for_record.insert(key, query_id);
                }
                Message::NewMessage(topic, message) => {
                    if let Some(messages) = self.state.messages.get_mut(&topic) {
                        messages.push(message)
                    } else {
                        self.state.messages.insert(topic, vec![message]);
                    }
                }
                Message::NewSubscribed(peer_id, topic) => {
                    if let Some(subscribed) = self.state.subscribers.get_mut(&topic) {
                        subscribed.push(peer_id)
                    } else {
                        self.state.subscribers.insert(topic, vec![peer_id]);
                    }
                }
                Message::DestroySubscriber(peer_id, topic) => {
                    if let Some(subscribed) = self.state.subscribers.get_mut(&topic) {
                        subscribed.retain(|p| *p != peer_id);
                    }
                }
                Message::Voice(crate::logic::message::VoiceMessage::Request(channel, peer_id)) => {
                    if let Some(channel) = self.state.voice_connected.get_mut(&channel) {
                        channel.insert(peer_id, false);
                    } else {
                        let mut hash = HashMap::new();
                        hash.insert(peer_id, false);
                        self.state.voice_connected.insert(channel, hash);
                    }
                }
                Message::Voice(crate::logic::message::VoiceMessage::UnRequest(
                    channel,
                    peer_id,
                )) => {
                    if let Some(channel) = self.state.voice_connected.get_mut(&channel) {
                        channel.retain(|peer, _| *peer != peer_id);
                    }
                }
                Message::Voice(crate::logic::message::VoiceMessage::Disconnected(peer_id)) => {
                    for (_, channel) in self.state.voice_connected.iter_mut() {
                        channel.retain(|peer, _| *peer != peer_id);
                    }
                }
                Message::Gui(crate::logic::message::GuiMessage::Friends(friends)) => {
                    for friend in friends.iter() {
                        self.state
                            .register_names
                            .insert(friend.peer_id, friend.name.clone());
                    }
                    self.state.friends = friends;
                }
                _ => {}
            }
        }
    }
}

impl eframe::App for TheMan {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.one_time {
            // ctx.set_debug_on_hover(true);
            self.one_time = true;
        }

        self.process_events();

        self.tab_manager.ui(ctx, &mut self.state);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.state.sender.try_send(Message::Save).unwrap();
        let save_state = loop {
            if let Some(save) = &self.state.save {
                break save;
            } else {
                self.process_events();
            }
        };

        if let Some(save) = save_state {
            storage.set_string("state", ron::to_string(save).unwrap());
            println!("Saved");
        }

        if self.should_close {
            self.state.sender.try_send(Message::ShutDown).unwrap();
        }
    }

    fn auto_save_interval(&self) -> std::time::Duration {
        Duration::MAX
    }

    fn on_close_event(&mut self) -> bool {
        self.should_close = true;
        true
    }
}
