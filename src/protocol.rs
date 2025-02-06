use std::{
    collections::BTreeMap,
    sync::{Arc, Weak},
};

use chrono::Utc;
use ed25519::Signature;
use futures_lite::{future::Boxed, StreamExt};
use futures_util::stream::BoxStream;
use iroh::{
    endpoint::{self, Connection, RecvStream, SendStream},
    protocol::ProtocolHandler,
    Endpoint, NodeAddr, NodeId, SecretKey,
};
use iroh_blobs::{
    downloader::DownloadRequest, net_protocol::Blobs, store::Store,
    util::local_pool::LocalPoolHandle, BlobFormat, Hash, HashAndFormat,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{
    mpsc::{Receiver, Sender},
    oneshot, watch, RwLock,
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
    pub hash_and_format: HashAndFormat,
}

impl Ticket {
    pub fn hash(&self) -> Hash {
        self.hash_and_format.hash
    }

    pub fn format(&self) -> BlobFormat {
        self.hash_and_format.format
    }

    pub fn providers(&self) -> impl std::iter::Iterator<Item = &NodeId> {
        std::iter::once(&self.owner_id)
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

#[derive(Debug)]
pub struct TreeEntry {
    prev: RwLock<Option<Arc<TreeEntry>>>,
    hash: Hash,
    nexts: RwLock<Vec<Weak<TreeEntry>>>,
}

impl TreeEntry {
    pub async fn back_find(self: &Arc<Self>, hash: Hash) -> Option<(usize, Arc<TreeEntry>)> {
        let mut depth = 0;
        let mut prev = Some(self.clone());

        while let Some(this) = prev.take() {
            if this.hash == hash {
                return Some((depth, this));
            }

            prev = this.prev.read().await.clone();
            depth += 1;
        }

        None
    }

    pub async fn prev(&self) -> Option<Arc<TreeEntry>> {
        self.prev.read().await.clone()
    }

    pub async fn next(&self) -> Option<Arc<TreeEntry>> {
        self.nexts
            .read()
            .await
            .first()
            .and_then(|entry| entry.upgrade())
    }

    pub fn hash(&self) -> Hash {
        self.hash
    }
}

#[derive(Clone)]
pub struct Conversation {
    pub raw: RawConversation,
    pub ticket: Ticket,
    pub tails: Vec<Arc<TreeEntry>>,
}

impl Conversation {
    pub async fn add_message(&mut self, hash: Hash, last: Option<Hash>) {
        if let Some(last) = last {
            'adding: {
                for tail in self.tails.iter_mut() {
                    let Some((depth, last)) = tail.back_find(last).await.clone() else {
                        continue;
                    };

                    let entry = Arc::new(TreeEntry {
                        prev: RwLock::new(Some(last.clone())),
                        hash,
                        nexts: RwLock::default(),
                    });

                    last.nexts.write().await.push(Arc::downgrade(&entry));
                    if depth == 0 {
                        *tail = entry;
                    } else {
                        self.tails.push(entry);
                    }

                    break 'adding;
                }

                panic!("The last entry cannot be found");
            }
        } else {
            let entry = Arc::new(TreeEntry {
                prev: RwLock::new(None),
                hash,
                nexts: RwLock::default(),
            });

            self.tails.push(entry);
        }
    }
}

#[derive(Debug, Clone)]
pub struct Message {
    pub raw: RawMessage,
    pub ticket: Ticket,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Packet {
    Welcome,
    SendMessage(Ticket),
}

pub enum ServiceRequest {
    Create(RawConversation, oneshot::Sender<Option<Hash>>),
    List(oneshot::Sender<Vec<Hash>>),
    GetMessage(Hash, oneshot::Sender<Option<Message>>),
    GetConversation(Hash, oneshot::Sender<Option<Conversation>>),
    Send(RawMessage, oneshot::Sender<Option<Hash>>),
    Recover(Ticket),
    RequestMessageSubscription(oneshot::Sender<watch::Receiver<Option<Message>>>),
    Add(Connection, (SendStream, RecvStream)),
}

#[allow(dead_code)]
struct Conn {
    connection: Connection,
    sender: SendStream,
    receiver: BoxStream<'static, Vec<Packet>>,
}

struct TheManService {
    connections: BTreeMap<NodeId, Conn>,
    message_sender: watch::Sender<Option<Message>>,
    receiver: Receiver<ServiceRequest>,
    downloader: iroh_blobs::downloader::Downloader,
    blobs: iroh_blobs::rpc::client::blobs::MemClient,
    endpoint: iroh::Endpoint,

    messages: BTreeMap<Hash, Message>,
    conversations: BTreeMap<Hash, Conversation>,
}

impl TheManService {
    pub async fn run(&mut self) {
        loop {
            let mut messages_to_add = Vec::<Ticket>::new();

            let recv = (!self.connections.is_empty()).then(|| {
                futures_util::future::select_all(self.connections.iter_mut().map(
                    |(node_id, conn)| {
                        Box::pin(async move { (*node_id, conn.receiver.next().await) })
                    },
                ))
            });

            tokio::select! {
                (recv, _, _) = async { if let Some(recv) = recv {recv.await} else {std::future::pending().await} } => {
                    let Some(packets) = recv.1 else{
                        info!("Disconnected from: {}", base64_serialize(&recv.0).unwrap());
                        self.connections.remove(&recv.0);
                        continue;
                    };

                    for packet in packets{
                    self.handle_packet(recv.0, packet, &mut messages_to_add).await;

                    }
                }
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
                        conversations_to_add.push(raw.conversation.clone());
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

                    conversation
                        .add_message(ticket.hash(), raw.last.as_ref().map(|ticket| ticket.hash()))
                        .await;

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
                ticket.providers().copied().collect::<Vec<_>>(),
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
            ServiceRequest::Add(connection, (mut sender, receiver)) => {
                let node_id = endpoint::get_remote_node_id(&connection)
                    .expect("Cannot get connection node_id");
                info!("Connected to: {}", base64_serialize(&node_id).unwrap());
                let mut buffer = Vec::default();
                ciborium::into_writer(&Packet::Welcome, &mut buffer).unwrap();
                sender
                    .write_all(&buffer)
                    .await
                    .expect("Cannot write welcome packet");

                let receiver = futures_lite::stream::unfold(
                    (receiver, [0u8; 1024]),
                    move |(mut receiver, mut buffer)| async move {
                        match receiver.read(&mut buffer).await {
                            Ok(Some(len)) => {
                                let mut cursor = std::io::Cursor::new(&buffer[0..len]);

                                let mut packets = Vec::default();

                                loop {
                                    let Ok(packet) =
                                        ciborium::from_reader::<Packet, _>(&mut cursor)
                                    else {
                                        error!(
                                            "Cannot read packet from: {}",
                                            base64_serialize(&node_id).unwrap()
                                        );
                                        return None;
                                    };

                                    info!("Readed {}, from {}", cursor.position(), len);

                                    packets.push(packet);

                                    if cursor.position() == len as u64 {
                                        break;
                                    }
                                }

                                Some((packets, (receiver, buffer)))
                            }
                            Ok(None) => None,
                            Err(err) => {
                                error!("{err} from: {}", base64_serialize(&node_id).unwrap());
                                None
                            }
                        }
                    },
                );
                self.connections.insert(
                    node_id,
                    Conn {
                        connection,
                        sender,
                        receiver: Box::pin(receiver),
                    },
                );
            }
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
                    owner_id: self.endpoint.node_id(),
                    hash_and_format: HashAndFormat {
                        hash: res.hash,
                        format: res.format,
                    },
                };

                let last = raw_message.last.as_ref().map(|ticket| ticket.hash());

                let msg = Message {
                    raw: raw_message,
                    ticket: ticket.clone(),
                };

                _ = self.message_sender.send(Some(msg.clone()));

                self.messages.insert(ticket.hash(), msg);

                conversation.add_message(ticket.hash(), last).await;

                if sender.send(Some(ticket.hash())).is_err() {
                    error!("Cannot send");
                }

                let mut buffer = Vec::default();
                ciborium::into_writer(&Packet::SendMessage(ticket), &mut buffer).unwrap();

                for node_id in conversation.raw.nodes.iter() {
                    if *node_id == self.endpoint.node_id() {
                        continue;
                    }

                    let Some(conn) = self.connections.get_mut(node_id) else {
                        info!(
                            "Cannot send message to: {}",
                            base64_serialize(node_id).unwrap()
                        );
                        continue;
                    };

                    if let Err(err) = conn.sender.write_all(&buffer).await {
                        error!(
                            "{err} when sending to: {}",
                            base64_serialize(node_id).unwrap()
                        );
                    }
                }
            }
            ServiceRequest::Recover(ticket) => {
                messages_to_add.push(ticket);
            }
            ServiceRequest::RequestMessageSubscription(sender) => {
                if let Err(err) = sender.send(self.message_sender.subscribe()) {
                    error!("Cannot send subscription receiver! {err:?}");
                }
            }
        }
    }

    pub async fn handle_packet(
        &mut self,
        node_id: NodeId,
        packet: Packet,
        messages_to_add: &mut Vec<Ticket>,
    ) {
        match packet {
            Packet::Welcome => {
                info!("Welcome from: {}", base64_serialize(&node_id).unwrap());

                let connection = self
                    .connections
                    .get_mut(&node_id)
                    .expect("Welcome message but no connection, HOW????");
                for conversation in self.conversations.values() {
                    if !conversation.raw.nodes.contains(&node_id) {
                        continue;
                    }

                    info!(
                        "Sending conversation tails to: {}",
                        base64_serialize(&node_id).unwrap()
                    );

                    for tail in conversation.tails.iter() {
                        info!("Sending tail: {}", base64_serialize(&tail.hash).unwrap());
                        let mut buffer = Vec::new();
                        let msg = self.messages.get(&tail.hash).expect("Cannot send message entry because we don't have the message from the entry");
                        ciborium::into_writer(
                            &Packet::SendMessage(msg.ticket.clone()),
                            &mut buffer,
                        )
                        .expect("Cannot serialize Token???");
                        match connection.sender.write_all(&buffer).await {
                            Ok(_) => {}
                            Err(err) => {
                                error!(
                                    "{err} Cannot send tail to {}",
                                    base64_serialize(&node_id).unwrap()
                                );
                            }
                        }
                    }
                }
            }

            Packet::SendMessage(ticket) => {
                info!(
                    "Received {} from: {}",
                    base64_serialize(&ticket).unwrap(),
                    base64_serialize(&node_id).unwrap()
                );
                messages_to_add.push(ticket);
            }
        }
    }
}

pub struct ConversationHandle<S: Store> {
    inner: Arc<Inner<S>>,
    conversation: Hash,
    last: Option<Ticket>,
}

impl<S: Store> ConversationHandle<S> {
    pub fn hash(&self) -> &Hash {
        &self.conversation
    }

    pub async fn get(&self) -> Conversation {
        let (s, r) = oneshot::channel();
        self.inner
            .send_request(ServiceRequest::GetConversation(self.conversation, s))
            .await;

        r.await.unwrap().unwrap()
    }

    pub fn set_last(&mut self, last: Ticket) {
        self.last = Some(last);
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

    pub async fn send(&mut self, msg: impl Into<String>) {
        let conversation = self.get().await.ticket;
        let (sender, receiver) = tokio::sync::oneshot::channel();

        self.inner
            .send_request(ServiceRequest::Send(
                RawMessage {
                    last: self.last.clone(),
                    time: chrono::Utc::now(),
                    conversation,
                    data: msg.into(),
                },
                sender,
            ))
            .await;

        if let Ok(Some(message_hash)) = receiver.await {
            let (sender, receiver) = tokio::sync::oneshot::channel();
            self.inner
                .send_request(ServiceRequest::GetMessage(message_hash, sender))
                .await;
            if let Ok(Some(msg)) = receiver.await {
                self.last = Some(msg.ticket);
            }
        }
    }
}

#[derive(Debug)]
struct Inner<S: Store> {
    pub sender: RwLock<Option<Sender<ServiceRequest>>>,
    #[allow(unused)]
    pub blobs: Blobs<S>,
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
}

#[derive(Debug, Clone)]
pub struct TheMan<S: Store> {
    inner: Arc<Inner<S>>,
    endpoint: Endpoint,
}

impl<S: Store> TheMan<S> {
    pub async fn spawn(blobs: Blobs<S>, endpoint: Endpoint, local_pool: &LocalPoolHandle) -> Self {
        let (sender, receiver) = tokio::sync::mpsc::channel(16);

        {
            let blobs = blobs.clone();
            let endpoint = endpoint.clone();
            local_pool.spawn_detached(move || async move {
                TheManService {
                    receiver,
                    downloader: blobs.downloader().clone(),
                    blobs: blobs.client().clone(),
                    messages: BTreeMap::default(),
                    conversations: BTreeMap::default(),
                    endpoint,
                    message_sender: watch::Sender::new(None),
                    connections: BTreeMap::new(),
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
            }),
            endpoint,
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
            conversation: hash,
            last: None,
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
                    conversation: hash,
                    last: None,
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
            conversation: hash,
            last: None,
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
        for node_id in ticket.providers() {
            self.connect(*node_id).await;
        }
        self.inner
            .send_request(ServiceRequest::Recover(ticket))
            .await;
    }

    pub async fn connect(&self, node_id: NodeId) {
        info!("Connecting to: {}", base64_serialize(&node_id).unwrap());
        let Ok(conn) = self.endpoint.connect(node_id, ALPN).await else {
            error!("Cannot connect to: {}", base64_serialize(&node_id).unwrap());
            return;
        };

        let Ok(bi) = conn
            .open_bi()
            .await
            .inspect_err(|err| error!("Cannot create channel: {err}"))
        else {
            return;
        };

        self.inner.send_request(ServiceRequest::Add(conn, bi)).await;
    }

    pub async fn store(&self, data: Vec<u8>) -> Ticket {
        let res = self
            .inner
            .blobs
            .client()
            .add_bytes(data)
            .await
            .expect("Cannot add to store");
        Ticket {
            owner_id: self.endpoint.node_id(),
            hash_and_format: HashAndFormat {
                hash: res.hash,
                format: res.format,
            },
        }
    }

    pub async fn get(&self, ticket: Ticket) -> Vec<u8> {
        let res = self
            .inner
            .blobs
            .downloader()
            .queue(DownloadRequest::new(
                ticket.hash_and_format,
                ticket.providers().map(|node_id| NodeAddr::from(*node_id)),
            ))
            .await
            .await
            .expect("Cannot download");

        let bytes = self
            .inner
            .blobs
            .client()
            .read_to_bytes(ticket.hash())
            .await
            .expect("Cannot get bytes");
        bytes.into()
    }
}

impl<S: Store> ProtocolHandler for TheMan<S> {
    fn accept(&self, connecting: iroh::endpoint::Connecting) -> Boxed<anyhow::Result<()>> {
        let inner = self.inner.clone();
        Box::pin(async move {
            info!("Connecting..");
            let connection = connecting
                .await
                .inspect_err(|err| error!("Connect Fail: {err}"))?;
            let bi = connection
                .accept_bi()
                .await
                .inspect_err(|err| error!("Channel Fail: {err}"))?;
            inner
                .send_request(ServiceRequest::Add(connection, bi))
                .await;

            Ok(())
        })
    }

    fn shutdown(&self) -> Boxed<()> {
        let inner = self.inner.clone();
        Box::pin(async move {
            inner.sender.write().await.take();
        })
    }
}
