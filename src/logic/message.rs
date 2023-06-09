use std::{collections::HashMap, time::Instant};

use chrono::Utc;
use egui::epaint::ahash::HashSet;
use libp2p::{
    gossipsub::{IdentTopic, TopicHash},
    kad::{ProgressStep, QueryId, QueryResult, QueryStats},
    multiaddr::Protocol,
    Multiaddr, PeerId,
};

use crate::{
    save_state::{Account, Friend, TheManSaveState},
    state::PeerStatus,
};

use super::TheManLogic;

#[derive(Debug)]
pub enum GuiMessage {
    Friends(Vec<Friend>),
    RefreshFriends,
}

#[derive(Debug)]
pub enum AudioMessage {
    CreateInputChannel { id: usize, codec: String },
    CreateOutputChannel { id: usize, codec: String },
    ResCreateInputChannel(usize, String),
    ResCreateOutputChannel(usize, String),
    DestroyInputChannel { id: usize },
    DestroyOuputChannel { id: usize },
    InputData { id: usize, data: Vec<u8> },
    OutputData { id: usize, data: Vec<u8> },
    InputError { id: usize, error: String },
    OutputError { id: usize, error: String },
}

#[derive(Debug)]
pub enum VoiceMessage {
    Connect(String),
    Disconnect(String),
    Request(String, PeerId),
    UnRequest(String, PeerId),
    Disconnected(PeerId),
    Accept(String, PeerId),
    Refuse(String, PeerId),
}

#[derive(Debug)]
pub enum Message {
    Gui(GuiMessage),
    Audio(AudioMessage),
    Voice(VoiceMessage),
    SwarmStatus(libp2p::swarm::NetworkInfo),
    Save,
    SaveResponse(Option<TheManSaveState>),
    BootstrapSet(bool),
    GetBootNodes,
    // TODO Add boot node status, in the current version of libp2p 0.52.0 NodeStatus is not public
    BootNodes(Vec<(PeerId, Vec<Multiaddr>)>),
    GetPeers,
    Peers(HashMap<PeerId, PeerStatus>),
    AccountActivate(usize, PeerId),
    SetAccount(usize),
    GetAccounts,
    Accounts(Vec<Account>),
    UpdateAccounts(Vec<Account>),
    GetAdresses,
    Adresses(HashSet<Multiaddr>),
    SearchForKey(Vec<u8>),
    ResSearchForKey(Vec<u8>, QueryId),
    SearchForRecord(Vec<u8>),
    ResSearchForRecord(Vec<u8>, QueryId),
    KademliaQueryProgress(QueryId, QueryResult, QueryStats, ProgressStep),
    SubscribeTopic(IdentTopic),
    UnsubscibeTopic(IdentTopic),
    NewMessage(TopicHash, libp2p::gossipsub::Message),
    NewSubscribed(PeerId, TopicHash),
    DestroySubscriber(PeerId, TopicHash),
    SendMessage(TopicHash, Vec<u8>),
    FindMe,
    ShutDown,
}

unsafe impl Send for Message {}
unsafe impl Sync for Message {}

impl TheManLogic {
    pub async fn on_message(&mut self, message: Message) {
        match message {
            Message::Save => {
                if let Some(account) = &mut self.state.account {
                    let save_state = {
                        let mut nodes = Vec::new();
                        for connection in account.swarm.behaviour_mut().kademlia.kbuckets() {
                            for peer in connection.iter() {
                                for adress in peer.node.value.iter() {
                                    if let Some(Protocol::P2p(_)) = adress.iter().last() {
                                        nodes.push(adress.clone());
                                    } else {
                                        let mut adress = adress.clone();
                                        adress.push(Protocol::P2p(
                                            (*peer.node.key.preimage()).into(),
                                        ));
                                    }
                                }
                            }
                        }

                        if let Some(acc) = self.state.accounts.get_mut(account.index) {
                            acc.expires = Utc::now()
                                + chrono::Duration::from_std(
                                    account.expires.duration_since(Instant::now()),
                                )
                                .unwrap_or_else(|_| chrono::Duration::zero());
                            acc.friends = account.friends.clone();
                        }

                        TheManSaveState {
                            bootnodes: nodes,
                            accounts: self.state.accounts.clone(),
                        }
                    };
                    let _ = self
                        .sender
                        .try_send(Message::SaveResponse(Some(save_state)));
                } else {
                    let _ = self.sender.try_send(Message::SaveResponse(None));
                }
            }
            Message::GetBootNodes => {
                if let Some(account) = &mut self.state.account {
                    let mut peers = Vec::new();
                    for kbucket in account.swarm.behaviour_mut().kademlia.kbuckets() {
                        for peer in kbucket.iter() {
                            peers.push((
                                *peer.node.key.preimage(),
                                peer.node.value.iter().cloned().collect::<Vec<Multiaddr>>(),
                            ));
                        }
                    }
                    let _ = self.sender.try_send(Message::BootNodes(peers));
                }
            }
            Message::GetPeers => {
                let _ = self
                    .sender
                    .try_send(Message::Peers(self.state.peers.clone()));
            }
            Message::BootstrapSet(value) => {
                if let Some(account) = &mut self.state.account {
                    if value {
                        self.bootstrap =
                            Some(account.swarm.behaviour_mut().kademlia.bootstrap().unwrap());
                    }
                }
                self.bootstraping = value;
            }
            Message::GetAccounts => {
                let _ = self
                    .sender
                    .try_send(Message::Accounts(self.state.accounts.clone()));
            }
            Message::SetAccount(account_index) => {
                // Cleanup
                self.subscribed.clear();
                self.registration_query = None;
                self.registration_step_1_query = None;
                self.state.peers.clear();
                // Cleanup channels!
                if let Some(account) = &mut self.state.account {
                    for (_, hash) in account.voice_channels.iter() {
                        hash.iter().for_each(|(_, channel_id)| {
                            let _ = self.audio_sender.try_send(Message::Audio(
                                AudioMessage::DestroyOuputChannel { id: *channel_id },
                            ));
                        });
                    }
                }
                self.state.set_account(account_index);

                if let Some(account) = &mut self.state.account {
                    let _ = self
                        .sender
                        .try_send(Message::SwarmStatus(account.swarm.network_info()));
                    let _ = self
                        .sender
                        .try_send(Message::AccountActivate(account_index, account.peer_id));

                    let _ = account
                        .swarm
                        .listen_on("/ip4/0.0.0.0/tcp/40002".parse().unwrap());

                    let _ = account
                        .swarm
                        .listen_on("/ip4/127.0.0.1/tcp/40002".parse().unwrap());

                    let _ = account
                        .swarm
                        .listen_on("/ip6/::/tcp/40002".parse().unwrap());

                    println!(
                        "Listening: {:?}",
                        account.swarm.listeners().collect::<Vec<&Multiaddr>>()
                    );

                    if self.bootstraping {
                        self.bootstrap =
                            Some(account.swarm.behaviour_mut().kademlia.bootstrap().unwrap());
                    }

                    let _ = self
                        .sender
                        .try_send(Message::Accounts(self.state.accounts.clone()));

                    let _ = self
                        .sender
                        .try_send(Message::Gui(GuiMessage::Friends(account.friends.clone())));

                    for friend in account.friends.iter() {
                        let _ = account
                            .swarm
                            .behaviour_mut()
                            .kademlia
                            .get_closest_peers(friend.peer_id);
                    }
                }
            }
            Message::GetAdresses => {
                if let Some(account) = &mut self.state.account {
                    let adresses = account
                        .swarm
                        .external_addresses()
                        .cloned()
                        .collect::<HashSet<Multiaddr>>();

                    let _ = self.sender.try_send(Message::Adresses(adresses));
                }
            }
            Message::UpdateAccounts(accounts) => {
                self.state.accounts = accounts;
                let _ = self
                    .sender
                    .try_send(Message::Accounts(self.state.accounts.clone()));
            }
            Message::SearchForKey(peer_id) => {
                if let Some(account) = &mut self.state.account {
                    let query_id = account
                        .swarm
                        .behaviour_mut()
                        .kademlia
                        .get_closest_peers(peer_id.clone());
                    let _ = self
                        .sender
                        .try_send(Message::ResSearchForKey(peer_id, query_id));
                }
            }
            Message::SubscribeTopic(topic) => {
                if let Some(account) = &mut self.state.account {
                    let _ = account.swarm.behaviour_mut().gossipsub.subscribe(&topic);
                    self.subscribed.push(topic.hash());
                }
            }
            Message::UnsubscibeTopic(topic) => {
                if let Some(account) = &mut self.state.account {
                    let _ = account.swarm.behaviour_mut().gossipsub.unsubscribe(&topic);
                }
            }
            Message::SendMessage(topic, message) => {
                if let Some(account) = &mut self.state.account {
                    let _ = account
                        .swarm
                        .behaviour_mut()
                        .gossipsub
                        .publish(topic, message);
                }
            }
            Message::SearchForRecord(key) => {
                if let Some(account) = &mut self.state.account {
                    let query_id = account
                        .swarm
                        .behaviour_mut()
                        .kademlia
                        .get_record(libp2p::kad::record::Key::new(&key));
                    let _ = self
                        .sender
                        .try_send(Message::ResSearchForRecord(key, query_id));
                }
            }
            Message::Voice(VoiceMessage::Connect(channel)) => {
                if let Some(account) = &mut self.state.account {
                    account.swarm.behaviour_mut().the_man.connect(channel);
                }
            }
            Message::Voice(VoiceMessage::Disconnect(channel)) => {
                if let Some(account) = &mut self.state.account {
                    if let Some(channel) = account.voice_channels.remove(&channel) {
                        for (_, id) in channel {
                            let _ = self
                                .audio_sender
                                .try_send(Message::Audio(AudioMessage::DestroyOuputChannel { id }));
                        }
                    }
                    account.swarm.behaviour_mut().the_man.disconnect(channel);
                }
            }
            Message::Voice(VoiceMessage::Accept(channel, peer_id)) => {
                if let Some(account) = &mut self.state.account {
                    account
                        .swarm
                        .behaviour_mut()
                        .the_man
                        .accept(channel, peer_id);
                }
            }
            Message::Voice(VoiceMessage::Refuse(channel, peer_id)) => {
                if let Some(account) = &mut self.state.account {
                    if let Some(channel) = account.voice_channels.get_mut(&channel) {
                        if let Some(id) = channel.remove(&peer_id) {
                            let _ = self
                                .audio_sender
                                .try_send(Message::Audio(AudioMessage::DestroyOuputChannel { id }));
                        }
                    }
                    account
                        .swarm
                        .behaviour_mut()
                        .the_man
                        .refuse(channel, peer_id);
                }
            }
            Message::Gui(GuiMessage::Friends(friends)) => {
                if let Some(account) = &mut self.state.account {
                    account.friends = friends.clone();
                    let _ = self
                        .sender
                        .try_send(Message::Gui(GuiMessage::Friends(friends)));
                }
            }
            Message::Gui(GuiMessage::RefreshFriends) => {
                if let Some(account) = &mut self.state.account {
                    for friend in account.friends.iter() {
                        let _ = account
                            .swarm
                            .behaviour_mut()
                            .kademlia
                            .get_closest_peers(friend.peer_id);
                    }
                }
            }
            _ => {}
        }
    }
}
