use libp2p::{
    gossipsub::{IdentTopic, TopicHash},
    kad::{kbucket::NodeStatus, ProgressStep, QueryId, QueryResult, QueryStats},
    multiaddr::Protocol,
    swarm::AddressRecord,
    Multiaddr, PeerId,
};

use crate::{
    save_state::{Account, TheManSaveState},
    state::PeerStatus,
};

use super::TheManLogic;

#[derive(Debug)]
pub enum Message {
    SwarmStatus(libp2p::swarm::NetworkInfo),
    Save,
    SaveResponse(Option<TheManSaveState>),
    Bootstrap,
    GetBootNodes,
    BootNodes(Vec<(PeerId, NodeStatus, Vec<Multiaddr>)>),
    GetPeers,
    Peers(Vec<(PeerId, PeerStatus)>),
    Peer(PeerId),
    SetAccount(usize),
    GetAccounts,
    Accounts(Vec<Account>),
    UpdateAccounts(Vec<Account>),
    GetAdresses,
    Adresses(Vec<AddressRecord>),
    SearchPeerId(PeerId),
    ResSearchPeerId(PeerId, QueryId),
    KademliaQueryProgress(QueryId, QueryResult, QueryStats, ProgressStep),
    SubscribeTopic(IdentTopic),
    UnsubscibeTopic(IdentTopic),
    NewMessage(TopicHash, libp2p::gossipsub::Message),
    NewSubscribed(PeerId, TopicHash),
    DestroySubscriber(PeerId, TopicHash),
    SendMessage(TopicHash, Vec<u8>),
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
                                peer.status,
                                peer.node.value.iter().cloned().collect::<Vec<Multiaddr>>(),
                            ));
                        }
                    }
                    let _ = self.sender.try_send(Message::BootNodes(peers));
                }
            }
            Message::GetPeers => {
                let _ = self.sender.try_send(Message::Peers(
                    self.state
                        .peers
                        .iter()
                        .map(|(d, e)| (*d, e.clone()))
                        .collect::<Vec<_>>(),
                ));
            }
            Message::Bootstrap => {
                if let Some(account) = &mut self.state.account {
                    let _query_id = account.swarm.behaviour_mut().kademlia.bootstrap().unwrap();
                }
            }
            Message::GetAccounts => {
                let _ = self
                    .sender
                    .try_send(Message::Accounts(self.state.accounts.clone()));
            }
            Message::SetAccount(account) => {
                self.state.set_account(account);

                if let Some(account) = &mut self.state.account {
                    let _ = self
                        .sender
                        .try_send(Message::SwarmStatus(account.swarm.network_info()));
                    let _ = self.sender.try_send(Message::Peer(account.peer_id));

                    let _ = account
                        .swarm
                        .listen_on("/ip4/0.0.0.0/tcp/0".parse().unwrap());

                    self.bootstrap =
                        Some(account.swarm.behaviour_mut().kademlia.bootstrap().unwrap());

                    let _ = self
                        .sender
                        .try_send(Message::Accounts(self.state.accounts.clone()));
                }
            }
            Message::GetAdresses => {
                if let Some(account) = &mut self.state.account {
                    let adresses = account
                        .swarm
                        .external_addresses()
                        .cloned()
                        .collect::<Vec<AddressRecord>>();

                    let _ = self.sender.try_send(Message::Adresses(adresses));
                }
            }
            Message::UpdateAccounts(accounts) => {
                self.state.accounts = accounts;
                let _ = self
                    .sender
                    .try_send(Message::Accounts(self.state.accounts.clone()));
            }
            Message::SearchPeerId(peer_id) => {
                if let Some(account) = &mut self.state.account {
                    let query_id = account
                        .swarm
                        .behaviour_mut()
                        .kademlia
                        .get_closest_peers(peer_id);
                    let _ = self
                        .sender
                        .try_send(Message::ResSearchPeerId(peer_id, query_id));
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
            _ => {}
        }
    }
}
