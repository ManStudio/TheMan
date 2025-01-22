use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use chrono::Utc;
use ed25519::Signature;
use futures_lite::{future::Boxed, StreamExt};
use iroh::{
    endpoint::{self, Connection},
    protocol::ProtocolHandler,
    Endpoint, NodeAddr, NodeId, SecretKey,
};
use iroh_blobs::{
    downloader::DownloadRequest,
    net_protocol::Blobs,
    store::{bao_tree::blake3, Store},
    util::local_pool::LocalPoolHandle,
    BlobFormat, Hash, HashAndFormat,
};
use iroh_gossip::{
    net::{Gossip, GossipReceiver, GossipSender},
    proto::TopicId,
};
use serde::{Deserialize, Serialize};
use tokio::{
    sync::{
        mpsc::{Receiver, Sender},
        oneshot, watch, Mutex, RwLock,
    },
    task::JoinHandle,
};
use tracing::{debug, error, info};

pub type Time = chrono::DateTime<chrono::Utc>;

use crate::base64_serialize;

#[derive(Serialize, Deserialize)]
pub struct Signed {
    sign: Signature,
    bytes: Vec<u8>,
}

impl Signed {
    pub fn new(secret: &SecretKey, bytes: Vec<u8>) -> Self {
        let sign = secret.sign(&bytes);
        Self { sign, bytes }
    }

    pub fn get(&self, node_id: &NodeId) -> Result<&[u8], ed25519::signature::Error> {
        node_id.verify(&self.bytes, &self.sign)?;
        Ok(&self.bytes)
    }
}

pub const ALPN: &[u8] = b"the-man";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Ticket {
    pub owner_id: NodeId,
    pub nodes: Vec<NodeAddr>,
    pub hash_and_format: HashAndFormat,
}

impl Ticket {
    pub fn hash(&self) -> Hash {
        self.hash_and_format.hash
    }

    pub fn format(&self) -> BlobFormat {
        self.hash_and_format.format
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawMessage {
    pub last: Option<Ticket>,
    pub time: Time,
    pub conversation: Ticket,
    pub data: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct RawConversation {
    pub nodes: Vec<NodeId>,
    pub time: Time,
}

impl RawConversation {
    pub fn new(nodes: impl Into<Vec<NodeId>>) -> Self {
        Self {
            nodes: nodes.into(),
            time: Utc::now(),
        }
    }
}

#[derive(Clone)]
pub struct Conversation {
    pub raw: RawConversation,
    pub ticket: Ticket,
    pub tails: Vec<Ticket>,
}

#[derive(Debug, Clone)]
pub struct Message {
    pub raw: RawMessage,
    pub ticket: Ticket,
}

pub enum ServiceRequest {
    Create(RawConversation, oneshot::Sender<Option<Hash>>),
    List(oneshot::Sender<Vec<Hash>>),
    GetMessage(Hash, oneshot::Sender<Option<Message>>),
    GetConversation(Hash, oneshot::Sender<Option<Conversation>>),
    Send(RawMessage, oneshot::Sender<Option<Hash>>),
    Recover(Ticket),
    Join(Vec<NodeId>),
    RequestMessageSubscription(oneshot::Sender<watch::Receiver<Option<Message>>>),
}

struct TheManService {
    message_sender: watch::Sender<Option<Message>>,
    receiver: Receiver<ServiceRequest>,
    gossip_recv: GossipReceiver,
    gossip_sender: GossipSender,
    downloader: iroh_blobs::downloader::Downloader,
    blobs: iroh_blobs::rpc::client::blobs::MemClient,
    endpoint: iroh::Endpoint,

    messages: HashMap<Hash, Message>,

    conversations: HashMap<Hash, Conversation>,
}

impl TheManService {
    pub async fn run(&mut self) {
        loop {
            let mut messages_to_add = Vec::<Ticket>::new();

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

            let mut conversations_to_add = Vec::<Ticket>::new();

            while !(messages_to_add.is_empty() && conversations_to_add.is_empty()) {
                for ticket in std::mem::take(&mut conversations_to_add) {
                    if self.conversations.contains_key(&ticket.hash()) {
                        continue;
                    }

                    let Some(bytes) = self.download(&ticket).await else {
                        continue;
                    };

                    let Ok(signed) = bincode::deserialize::<Signed>(&bytes) else {
                        error!(
                            "Cannot parse signed conversation: {}",
                            base64_serialize(&ticket.hash()).unwrap()
                        );
                        continue;
                    };

                    let Ok(bytes) = signed.get(&ticket.owner_id) else {
                        error!(
                            "Cannot verify signiture of conversation: {}",
                            base64_serialize(&ticket.hash()).unwrap()
                        );
                        continue;
                    };

                    let Ok(raw) = bincode::deserialize::<RawConversation>(bytes) else {
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

                    let Ok(signed) = bincode::deserialize::<Signed>(&bytes) else {
                        error!(
                            "Cannot parse signed message: {}",
                            base64_serialize(&ticket.hash()).unwrap()
                        );
                        continue;
                    };

                    let Ok(bytes) = signed.get(&ticket.owner_id) else {
                        error!(
                            "Cannot verify signiture of message: {}",
                            base64_serialize(&ticket.hash()).unwrap()
                        );
                        continue;
                    };

                    let Ok(raw) = bincode::deserialize::<RawMessage>(bytes) else {
                        error!(
                            "Cannot parse message: {}",
                            base64_serialize(&ticket.hash()).unwrap()
                        );
                        continue;
                    };

                    let Some(conversation) = self.conversations.get_mut(&raw.conversation.hash())
                    else {
                        let mut conversation_ticket = raw.conversation.clone();
                        conversation_ticket
                            .nodes
                            .extend(ticket.nodes.iter().cloned());
                        conversations_to_add.push(conversation_ticket);
                        messages_to_add.push(ticket);
                        continue;
                    };

                    if !conversation.raw.nodes.contains(&ticket.owner_id) {
                        debug!("Some body send a message in a conversation that is not part of, peer_id: {}, conversation: {}, message_ticket: {}",
                                base64_serialize(&ticket.owner_id).unwrap(),
                                base64_serialize(&raw.conversation.hash()).unwrap(),
                                base64_serialize(&ticket).unwrap()
                            );
                        _ = self.blobs.delete_blob(ticket.hash()).await;
                        continue;
                    }

                    if let Some(last) = raw.last.clone() {
                        if !self.messages.contains_key(&last.hash()) {
                            let mut last_ticket = last.clone();
                            last_ticket.nodes.extend(ticket.nodes.iter().cloned());
                            messages_to_add.push(last_ticket);
                            messages_to_add.push(ticket);
                            continue;
                        }
                        if let Some(index) = conversation
                            .tails
                            .iter()
                            .position(|m| m.hash() == last.hash())
                        {
                            conversation.tails[index] = ticket.clone();
                            let message = Message { raw, ticket };
                            self.messages.insert(message.ticket.hash(), message.clone());
                            _ = self.message_sender.send(Some(message));
                            continue;
                        }
                    }

                    conversation.tails.push(ticket.clone());
                    let message = Message { raw, ticket };
                    self.messages.insert(message.ticket.hash(), message.clone());
                    _ = self.message_sender.send(Some(message));
                }
            }
        }
    }

    pub async fn download(&self, ticket: &Ticket) -> Option<bytes::Bytes> {
        let handle = self
            .downloader
            .queue(DownloadRequest::new(
                HashAndFormat::new(ticket.hash(), ticket.format()),
                ticket.nodes.clone(),
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
        messages_to_add: &mut Vec<Ticket>,
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

                let bytes =
                    bincode::serialize(&Signed::new(self.endpoint.secret_key(), bytes)).unwrap();

                let Ok(res) = self.blobs.add_bytes(bytes).await else {
                    error!("Cannot add bytes");
                    if sender.send(None).is_err() {
                        error!("Cannot send");
                    }
                    return;
                };

                let ticket = Ticket {
                    owner_id: node_addr.node_id,
                    nodes: vec![node_addr],
                    hash_and_format: HashAndFormat {
                        hash: res.hash,
                        format: res.format,
                    },
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
            ServiceRequest::Send(raw_message, sender) => {
                let Some(conversation) =
                    self.conversations.get_mut(&raw_message.conversation.hash())
                else {
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

                let bytes =
                    bincode::serialize(&Signed::new(self.endpoint.secret_key(), bytes)).unwrap();

                let Ok(res) = self.blobs.add_bytes(bytes).await else {
                    error!("Cannot add message as blob");
                    if sender.send(None).is_err() {
                        error!("Cannot send");
                    }
                    return;
                };

                let ticket = Ticket {
                    owner_id: node_addr.node_id,
                    nodes: vec![node_addr],
                    hash_and_format: HashAndFormat {
                        hash: res.hash,
                        format: res.format,
                    },
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
            ServiceRequest::Recover(ticket) => {
                messages_to_add.push(ticket);
            }
            ServiceRequest::Join(peers) => {
                if let Err(err) = self.gossip_sender.join_peers(peers).await {
                    error!("Cannot join: {err}");
                }
            }
            ServiceRequest::RequestMessageSubscription(sender) => {
                if let Err(err) = sender.send(self.message_sender.subscribe()) {
                    error!("Cannot send subscription receiver! {err:?}");
                }
            }
        }
    }

    pub async fn handle_gossip(
        &mut self,
        event: iroh_gossip::net::Event,
        messages_to_add: &mut Vec<Ticket>,
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
                match bincode::deserialize::<Ticket>(&message.content) {
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
    inner: Arc<Inner<S>>,
    hash: Hash,
}

impl<S: Store> ConversationHandle<S> {
    pub fn hash(&self) -> &Hash {
        &self.hash
    }

    pub async fn get(&self) -> Conversation {
        let (s, r) = oneshot::channel();
        self.inner
            .send_request(ServiceRequest::GetConversation(self.hash, s))
            .await;

        r.await.unwrap().unwrap()
    }

    pub async fn messages(&self) -> Vec<Message> {
        let conversation = self.get().await;
        let mut messages = Vec::default();
        for ticket in conversation.tails.iter() {
            let (s, r) = oneshot::channel();
            self.inner
                .send_request(ServiceRequest::GetMessage(ticket.hash(), s))
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
                        .send_request(ServiceRequest::GetMessage(last.hash(), s))
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
                .send_request(ServiceRequest::GetMessage(ticket.hash(), s))
                .await;
            let msg = r.await.unwrap().unwrap();
            messages.push(msg);
        }

        messages.sort_by_key(|m| m.raw.time);

        let (s, r) = oneshot::channel();
        if let Some(message) = messages.last() {
            self.inner
                .send_request(ServiceRequest::Send(
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
                .send_request(ServiceRequest::Send(
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

#[derive(Debug)]
struct Inner<S: Store> {
    pub sender: RwLock<Option<Sender<ServiceRequest>>>,
    #[allow(unused)]
    pub blobs: Blobs<S>,
    connections: RwLock<BTreeMap<NodeId, Connection>>,
    watch_tasts: Mutex<Vec<JoinHandle<()>>>,
    endpoint: Endpoint,
}

impl<S: Store> Inner<S> {
    async fn send_request(&self, request: ServiceRequest) {
        let Some(sender) = &*self.sender.read().await else {
            error!("Cannot grab sender");
            return;
        };

        if let Err(err) = sender.send(request).await {
            error!("Cannot send request to Service: {err}");
        }
    }

    async fn add_connection(self: &Arc<Self>, connection: Connection) {
        let mut connections = self.connections.write().await;
        let Ok(node_id) = endpoint::get_remote_node_id(&connection) else {
            error!("Cannot get connection node_id");
            return;
        };
        info!("Connected to {}", base64_serialize(&node_id).unwrap());
        connections.insert(node_id, connection.clone());
        {
            let conn = connection.clone();
            let inner = self.clone();

            let handle = tokio::spawn(async move {
                conn.closed().await;
                info!("Disconnected from: {}", base64_serialize(&node_id).unwrap());

                inner.connections.write().await.remove(&node_id);

                inner.refresh().await;
            });

            self.watch_tasts.lock().await.push(handle);
        }

        {
            self.watch_tasts
                .lock()
                .await
                .retain(|task| !task.is_finished());
        }
        drop(connections);
        self.refresh().await;
    }

    async fn refresh(&self) {
        let nodes_ids = self
            .connections
            .read()
            .await
            .keys()
            .copied()
            .collect::<Vec<_>>();
        self.send_request(ServiceRequest::Join(nodes_ids)).await;
    }
}

#[derive(Debug, Clone)]
pub struct TheMan<S: Store> {
    inner: Arc<Inner<S>>,
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
            let endpoint = endpoint.clone();
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
                    message_sender: watch::Sender::new(None),
                }
                .run()
                .await;

                info!("TheMan Service stopped!");
            });
        }

        Self {
            inner: Arc::new(Inner {
                sender: RwLock::new(Some(sender)),
                blobs,
                connections: RwLock::default(),
                endpoint,
                watch_tasts: Mutex::default(),
            }),
        }
    }

    pub async fn create(&self, raw: RawConversation) -> Option<ConversationHandle<S>> {
        let (s, r) = oneshot::channel();
        self.inner
            .send_request(ServiceRequest::Create(raw, s))
            .await;
        let hash = r.await.ok().flatten()?;

        Some(ConversationHandle {
            inner: self.inner.clone(),
            hash,
        })
    }

    pub async fn raw_conversations(&self) -> Option<Vec<Hash>> {
        let (s, r) = oneshot::channel();
        self.inner.send_request(ServiceRequest::List(s)).await;
        r.await.ok()
    }

    pub async fn conversations(&self) -> Option<Vec<ConversationHandle<S>>> {
        Some(
            self.raw_conversations()
                .await?
                .into_iter()
                .map(|hash| ConversationHandle {
                    inner: self.inner.clone(),
                    hash,
                })
                .collect::<Vec<_>>(),
        )
    }

    pub async fn get_conversation(&self, hash: Hash) -> Option<ConversationHandle<S>> {
        let (s, r) = oneshot::channel();
        self.inner
            .send_request(ServiceRequest::GetConversation(hash, s))
            .await;

        let _ = r.await.ok().flatten()?;

        Some(ConversationHandle {
            inner: self.inner.clone(),
            hash,
        })
    }

    pub async fn get_message(&self, hash: Hash) -> Option<Message> {
        let (s, r) = oneshot::channel();
        self.inner
            .send_request(ServiceRequest::GetMessage(hash, s))
            .await;

        r.await.unwrap()
    }

    pub async fn subscribe_messages(&self) -> watch::Receiver<Option<Message>> {
        let (s, r) = oneshot::channel();
        self.inner
            .send_request(ServiceRequest::RequestMessageSubscription(s))
            .await;

        r.await.unwrap()
    }

    pub async fn send_message(&self, raw: RawMessage) -> Option<Hash> {
        let (s, r) = oneshot::channel();
        self.inner.send_request(ServiceRequest::Send(raw, s)).await;
        r.await.unwrap()
    }

    pub async fn recover(&self, ticket: Ticket) {
        for node_id in ticket.nodes.iter() {
            self.connect(node_id.node_id).await;
        }
        self.inner
            .send_request(ServiceRequest::Recover(ticket))
            .await;
    }

    pub async fn connect(&self, node_id: NodeId) {
        info!("Connecting to: {}", base64_serialize(&node_id).unwrap());
        let Ok(conn) = self.inner.endpoint.connect(node_id, ALPN).await else {
            error!("Cannot connect to: {}", base64_serialize(&node_id).unwrap());
            return;
        };

        self.inner.add_connection(conn).await;
    }
}

impl<S: Store> ProtocolHandler for TheMan<S> {
    fn accept(&self, connection: iroh::endpoint::Connecting) -> Boxed<anyhow::Result<()>> {
        let inner = self.inner.clone();
        Box::pin(async move {
            inner.add_connection(connection.await?).await;

            Ok(())
        })
    }

    fn shutdown(&self) -> Boxed<()> {
        let inner = self.inner.clone();
        Box::pin(async move {
            let mut connections = inner.connections.write().await;
            let connections = std::mem::take(&mut *connections);
            drop(connections);

            inner.sender.write().await.take();
        })
    }
}
