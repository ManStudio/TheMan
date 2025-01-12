use std::{collections::HashMap, sync::Arc};

use chrono::Utc;
use futures_lite::{future::Boxed, StreamExt};
use iroh::{protocol::ProtocolHandler, Endpoint, NodeId};
use iroh_blobs::{
    downloader::DownloadRequest,
    net_protocol::Blobs,
    store::{bao_tree::blake3, Store},
    ticket::BlobTicket,
    util::local_pool::LocalPoolHandle,
    Hash, HashAndFormat,
};
use iroh_gossip::{
    net::{Gossip, GossipReceiver, GossipSender},
    proto::TopicId,
};
use tokio::sync::{
    mpsc::{Receiver, Sender},
    oneshot, RwLock,
};
use tracing::{debug, error, info};

use crate::{base64_serialize, RawConversation, RawMessage};

pub const ALPN: &[u8] = b"the-man";

#[derive(Debug, Clone)]
pub struct TheMan<S: Store> {
    inner: Arc<Inner<S>>,
}

#[derive(Clone)]
pub struct Conversation {
    pub raw: RawConversation,
    pub ticket: BlobTicket,
    pub tails: Vec<BlobTicket>,
}

#[derive(Clone)]
pub struct Message {
    pub raw: RawMessage,
    pub ticket: BlobTicket,
}

struct TheManService {
    receiver: Receiver<ServiceRequest>,
    gossip_recv: GossipReceiver,
    gossip_sender: GossipSender,
    downloader: iroh_blobs::downloader::Downloader,
    blobs: iroh_blobs::rpc::client::blobs::MemClient,
    endpoint: iroh::Endpoint,

    messages: HashMap<Hash, Message>,

    conversations: HashMap<Hash, Conversation>,
}

enum ServiceRequest {
    Create(RawConversation, oneshot::Sender<Option<Hash>>),
    List(oneshot::Sender<Vec<Hash>>),
    GetMessage(Hash, oneshot::Sender<Option<Message>>),
    GetConversation(Hash, oneshot::Sender<Option<Conversation>>),
    Send(Hash, RawMessage, oneshot::Sender<Option<Hash>>),
    Recover(BlobTicket),
    Join(Vec<NodeId>),
}

impl TheManService {
    pub async fn run(&mut self) {
        loop {
            let mut messages_to_add = Vec::<BlobTicket>::new();

            tokio::select! {
                Ok(Some(event)) = self.gossip_recv.try_next() => {
                    self.handle_gossip(event, &mut messages_to_add).await;
                },
                request = self.receiver.recv() => {
                    let Some(request) = request else{
                        break;
                    };

                    self.handle_request(request, &mut messages_to_add).await;
                }
            }

            let mut conversations_to_add = Vec::<BlobTicket>::new();

            while !(messages_to_add.is_empty() && conversations_to_add.is_empty()) {
                for ticket in std::mem::take(&mut conversations_to_add) {
                    if self.conversations.contains_key(&ticket.hash()) {
                        continue;
                    }

                    let Some(bytes) = self.download(&ticket).await else {
                        continue;
                    };

                    let Ok(raw) = bincode::deserialize::<RawConversation>(&bytes) else {
                        error!(
                            "Cannot parse conversation: {}",
                            base64_serialize(&ticket.hash()).unwrap()
                        );
                        continue;
                    };

                    self.conversations.insert(
                        ticket.hash(),
                        Conversation {
                            raw,
                            ticket,
                            tails: vec![],
                        },
                    );
                }

                for ticket in std::mem::take(&mut messages_to_add) {
                    if self.messages.contains_key(&ticket.hash()) {
                        continue;
                    }

                    let Some(bytes) = self.download(&ticket).await else {
                        continue;
                    };

                    let Ok(raw) = bincode::deserialize::<RawMessage>(&bytes) else {
                        error!(
                            "Cannot parse message: {}",
                            base64_serialize(&ticket.hash()).unwrap()
                        );
                        continue;
                    };

                    let Some(conversation) = self.conversations.get_mut(&raw.conversation.hash())
                    else {
                        conversations_to_add.push(raw.conversation.clone());
                        messages_to_add.push(ticket);
                        continue;
                    };

                    if !conversation.raw.peers.contains(&ticket.node_addr().node_id) {
                        debug!("Some body send a message in a conversation that is not part of, peer_id: {}, conversation: {}, message_ticket: {}",
                                base64_serialize(&ticket.node_addr().node_id).unwrap(),
                                base64_serialize(&raw.conversation.hash()).unwrap(),
                                base64_serialize(&ticket).unwrap()
                            );
                        _ = self.blobs.delete_blob(ticket.hash()).await;
                        continue;
                    }

                    if let Some(last) = raw.last.clone() {
                        if !self.messages.contains_key(&last.hash()) {
                            messages_to_add.push(last);
                            messages_to_add.push(ticket);
                            continue;
                        }
                        if let Some(index) = conversation.tails.iter().position(|m| *m == last) {
                            conversation.tails[index] = ticket.clone();
                            self.messages.insert(ticket.hash(), Message { raw, ticket });
                            continue;
                        }
                    }

                    conversation.tails.push(ticket.clone());
                    self.messages.insert(ticket.hash(), Message { raw, ticket });
                }
            }
        }
    }

    pub async fn download(&self, ticket: &BlobTicket) -> Option<bytes::Bytes> {
        let handle = self
            .downloader
            .queue(DownloadRequest::new(
                HashAndFormat::new(ticket.hash(), ticket.format()),
                [ticket.node_addr().clone()],
            ))
            .await;

        info!("Downloading: {}", base64_serialize(&ticket.hash()).unwrap());
        match handle.await {
            Ok(stats) => {
                info!(
                        "Downloaded: {}, in time: {:?}, Bytes Written: {}, Bytes Readed: {} Speed: {} MBs",
                        base64_serialize(&ticket.hash()).unwrap(),
                        stats.elapsed,
                        stats.bytes_written,
                        stats.bytes_read,
                        stats.mbits()
                    );
            }
            Err(err) => {
                error!(
                    "Cannot download: {}, {err}",
                    base64_serialize(&ticket.hash()).unwrap()
                );
            }
        }

        let status = self.blobs.status(ticket.hash()).await.unwrap();

        match status {
            iroh_blobs::rpc::client::blobs::BlobStatus::Complete { size } => {
                println!("Blob with size: {size}");
                Some(self.blobs.read_to_bytes(ticket.hash()).await.unwrap())
            }
            _ => None,
        }
    }

    pub async fn handle_request(
        &mut self,
        request: ServiceRequest,
        messages_to_add: &mut Vec<BlobTicket>,
    ) {
        match request {
            ServiceRequest::Create(raw_conversation, sender) => {
                let bytes = bincode::serialize(&raw_conversation).unwrap();

                let Ok(node_addr) = self.endpoint.node_addr().await else {
                    error!("Cannot get the node_addr");
                    if sender.send(None).is_err() {
                        error!("Cannot send");
                    }
                    return;
                };

                let Ok(res) = self.blobs.add_bytes(bytes).await else {
                    error!("Cannot add bytes");
                    if sender.send(None).is_err() {
                        error!("Cannot send");
                    }
                    return;
                };

                let Ok(ticket) = BlobTicket::new(node_addr, res.hash, res.format) else {
                    error!("Cannot create ticket");
                    if sender.send(None).is_err() {
                        error!("Cannot send");
                    }
                    return;
                };

                self.conversations.insert(
                    ticket.hash(),
                    Conversation {
                        raw: raw_conversation,
                        ticket: ticket.clone(),
                        tails: vec![],
                    },
                );

                if sender.send(Some(ticket.hash())).is_err() {
                    error!("Cannot send");
                }
            }
            ServiceRequest::List(sender) => {
                let Ok(_) = sender.send(self.conversations.keys().copied().collect::<Vec<_>>())
                else {
                    error!("Cannot send");
                    return;
                };
            }
            ServiceRequest::GetMessage(hash, sender) => {
                let Some(message) = self.messages.get(&hash) else {
                    if sender.send(None).is_err() {
                        error!("Cannot send");
                    }
                    return;
                };

                let Ok(_) = sender.send(Some(message.clone())) else {
                    error!("Cannot send");
                    return;
                };
            }
            ServiceRequest::GetConversation(hash, sender) => {
                let Some(conversations) = self.conversations.get(&hash) else {
                    if sender.send(None).is_err() {
                        error!("Cannot send");
                    }
                    return;
                };

                let Ok(_) = sender.send(Some(conversations.clone())) else {
                    error!("Cannot send");
                    return;
                };
            }
            ServiceRequest::Send(hash, raw_message, sender) => {
                let Some(conversation) = self.conversations.get_mut(&hash) else {
                    error!("Cannot find conversation");
                    if sender.send(None).is_err() {
                        error!("Cannot send");
                    }
                    return;
                };

                let to_replace = raw_message
                    .last
                    .as_ref()
                    .and_then(|last| conversation.tails.iter().position(|m| m == last));

                let Ok(node_addr) = self.endpoint.node_addr().await else {
                    error!("Cannot get the node_addr");
                    if sender.send(None).is_err() {
                        error!("Cannot send");
                    }
                    return;
                };

                let Ok(bytes) = bincode::serialize(&raw_message) else {
                    error!("Cannot serialize message");
                    if sender.send(None).is_err() {
                        error!("Cannot send");
                    }
                    return;
                };

                let Ok(res) = self.blobs.add_bytes(bytes).await else {
                    error!("Cannot add message as blob");
                    if sender.send(None).is_err() {
                        error!("Cannot send");
                    }
                    return;
                };

                let Ok(ticket) = BlobTicket::new(node_addr, res.hash, res.format) else {
                    error!("Cannot create ticket");
                    if sender.send(None).is_err() {
                        error!("Cannot send");
                    }
                    return;
                };

                let Ok(bytes) = bincode::serialize(&ticket) else {
                    error!("Cannot serialize ticket");
                    if sender.send(None).is_err() {
                        error!("Cannot send");
                    }
                    return;
                };

                let Ok(_) = self.gossip_sender.broadcast(bytes.into()).await else {
                    error!("Cannot broadcast message ticket");
                    if sender.send(None).is_err() {
                        error!("Cannot send");
                    }
                    return;
                };

                self.messages.insert(
                    ticket.hash(),
                    Message {
                        raw: raw_message,
                        ticket: ticket.clone(),
                    },
                );

                if let Some(index) = to_replace {
                    conversation.tails[index] = ticket.clone();
                } else {
                    conversation.tails.push(ticket.clone());
                }

                if sender.send(Some(ticket.hash())).is_err() {
                    error!("Cannot send");
                }
            }
            ServiceRequest::Recover(blob_ticket) => {
                messages_to_add.push(blob_ticket);
            }
            ServiceRequest::Join(peers) => {
                if let Err(err) = self.gossip_sender.join_peers(peers).await {
                    error!("Cannot join: {err}");
                }
            }
        }
    }

    pub async fn handle_gossip(
        &mut self,
        event: iroh_gossip::net::Event,
        messages_to_add: &mut Vec<BlobTicket>,
    ) {
        let iroh_gossip::net::Event::Gossip(event) = event else {
            info!("gossip Lagged");
            return;
        };

        match event {
            iroh_gossip::net::GossipEvent::Joined(vec) => {
                println!("Joined: {vec:?}");
            }
            iroh_gossip::net::GossipEvent::NeighborUp(public_key) => {
                println!("NeighborUp: {public_key}");
            }
            iroh_gossip::net::GossipEvent::NeighborDown(public_key) => {
                println!("NeighborDown: {public_key}");
            }
            iroh_gossip::net::GossipEvent::Received(message) => {
                match bincode::deserialize::<BlobTicket>(&message.content) {
                    Err(err) => println!("Error when parsing topic event: {err}"),
                    Ok(message_ticket) => {
                        messages_to_add.push(message_ticket);
                    }
                }
            }
        }
    }
}

pub struct ConversationHandle<S: Store> {
    inner: TheMan<S>,
    hash: Hash,
}

impl<S: Store> ConversationHandle<S> {
    pub fn hash(&self) -> &Hash {
        &self.hash
    }

    pub async fn get(&self) -> Conversation {
        let (s, r) = oneshot::channel();
        self.inner
            .send(ServiceRequest::GetConversation(self.hash, s))
            .await;

        r.await.unwrap().unwrap()
    }

    pub async fn messages(&self) -> Vec<Message> {
        let conversation = self.get().await;
        let mut messages = Vec::default();
        for ticket in conversation.tails.iter() {
            let (s, r) = oneshot::channel();
            self.inner
                .send(ServiceRequest::GetMessage(ticket.hash(), s))
                .await;
            let msg = r.await.unwrap().unwrap();
            messages.push(msg);
        }

        let mut i = 0;
        loop {
            let Some(message) = messages.get(i) else {
                break;
            };
            if let Some(last) = message.raw.last.clone() {
                if !messages.iter().any(|m| m.ticket == last) {
                    let (s, r) = oneshot::channel();
                    self.inner
                        .send(ServiceRequest::GetMessage(last.hash(), s))
                        .await;
                    let msg = r.await.unwrap().unwrap();
                    messages.push(msg);
                }
            }
            i += 1;
        }

        messages.sort_by_key(|m| m.raw.time);

        messages
    }

    pub async fn send(&self, msg: impl Into<String>) {
        let conversation = self.get().await;
        let mut messages = Vec::default();
        for ticket in conversation.tails.iter() {
            let (s, r) = oneshot::channel();
            self.inner
                .send(ServiceRequest::GetMessage(ticket.hash(), s))
                .await;
            let msg = r.await.unwrap().unwrap();
            messages.push(msg);
        }

        messages.sort_by_key(|m| m.raw.time);

        let (s, r) = oneshot::channel();
        if let Some(message) = messages.last() {
            self.inner
                .send(ServiceRequest::Send(
                    conversation.ticket.hash(),
                    RawMessage {
                        last: Some(message.ticket.clone()),
                        time: Utc::now(),
                        conversation: conversation.ticket,
                        data: msg.into(),
                    },
                    s,
                ))
                .await;
        } else {
            self.inner
                .send(ServiceRequest::Send(
                    conversation.ticket.hash(),
                    RawMessage {
                        last: None,
                        time: Utc::now(),
                        conversation: conversation.ticket,
                        data: msg.into(),
                    },
                    s,
                ))
                .await;
        }

        if r.await.ok().flatten().is_none() {
            error!("Message was not sent!");
        }
    }
}

impl<S: Store> TheMan<S> {
    pub async fn spawn(
        gossip: Gossip,
        blobs: Blobs<S>,
        endpoint: Endpoint,
        local_pool: &LocalPoolHandle,
    ) -> Self {
        let topic_id = TopicId::from_bytes(*blake3::hash(b"the-man").as_bytes());
        let (sender, receiver) = tokio::sync::mpsc::channel(16);
        let (gossip_sender, gossip_recv) = gossip.subscribe(topic_id, vec![]).unwrap().split();

        {
            let blobs = blobs.clone();
            local_pool.spawn_detached(move || async move {
                TheManService {
                    receiver,
                    gossip_sender,
                    gossip_recv,
                    downloader: blobs.downloader().clone(),
                    blobs: blobs.client().clone(),
                    messages: HashMap::default(),
                    conversations: HashMap::default(),
                    endpoint,
                }
                .run()
                .await;
            });
        }

        Self {
            inner: Arc::new(Inner {
                sender: RwLock::new(Some(sender)),
                blobs,
            }),
        }
    }

    pub async fn create(&self, raw: RawConversation) -> Option<ConversationHandle<S>> {
        let (s, r) = oneshot::channel();
        self.send(ServiceRequest::Create(raw, s)).await;
        let hash = r.await.ok().flatten()?;

        Some(ConversationHandle {
            inner: self.clone(),
            hash,
        })
    }

    pub async fn conversations(&self) -> Option<Vec<ConversationHandle<S>>> {
        let (s, r) = oneshot::channel();
        self.send(ServiceRequest::List(s)).await;
        let hashes = r.await.ok()?;
        Some(
            hashes
                .into_iter()
                .map(|hash| ConversationHandle {
                    inner: self.clone(),
                    hash,
                })
                .collect::<Vec<_>>(),
        )
    }

    pub async fn get_conversation(&self, hash: Hash) -> Option<ConversationHandle<S>> {
        let (s, r) = oneshot::channel();
        self.send(ServiceRequest::GetConversation(hash, s)).await;

        let _ = r.await.ok().flatten()?;

        Some(ConversationHandle {
            inner: self.clone(),
            hash,
        })
    }

    pub async fn recover(&self, ticket: BlobTicket) {
        self.send(ServiceRequest::Recover(ticket)).await;
    }

    pub async fn join(&self, peers: Vec<NodeId>) {
        self.send(ServiceRequest::Join(peers)).await;
    }

    async fn send(&self, request: ServiceRequest) {
        let Some(sender) = &*self.inner.sender.read().await else {
            error!("Cannot grab sender");
            return;
        };

        if let Err(err) = sender.send(request).await {
            error!("Cannot send request to Service: {err}");
        }
    }
}

#[derive(Debug)]
struct Inner<S: Store> {
    pub sender: RwLock<Option<Sender<ServiceRequest>>>,
    #[allow(unused)]
    pub blobs: Blobs<S>,
}

impl<S: Store> ProtocolHandler for TheMan<S> {
    fn accept(&self, _conn: iroh::endpoint::Connecting) -> Boxed<anyhow::Result<()>> {
        Box::pin(async move { Ok(()) })
    }

    fn shutdown(&self) -> Boxed<()> {
        let inner = self.inner.clone();
        Box::pin(async move {
            inner.sender.write().await.take();
        })
    }
}
