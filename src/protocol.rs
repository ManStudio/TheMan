use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    pin::Pin,
    sync::{Arc, Weak},
};

use chrono::Utc;
use ed25519::Signature;
use futures_util::StreamExt;
use iroh::{
    Endpoint, NodeId, SecretKey,
    endpoint::{Connection, RecvStream, SendStream},
    protocol::{AcceptError, ProtocolHandler},
};
use iroh_blobs::{
    BlobFormat, Hash, HashAndFormat, api::downloader::DownloadRequest, net_protocol::Blobs,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{
    Mutex, RwLock,
    mpsc::{Receiver, Sender, UnboundedSender},
    oneshot, watch,
};
use tracing::{debug, error, info, info_span, trace, warn};

pub type Time = chrono::DateTime<chrono::Utc>;

use crate::{base64_deserialize, base64_serialize};

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

impl bincode::Encode for Signed {
    fn encode<E: bincode::enc::Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), bincode::error::EncodeError> {
        bincode::Encode::encode(&self.sign.to_bytes(), encoder)?;
        bincode::Encode::encode(&self.bytes, encoder)?;
        Ok(())
    }
}

impl<Context> bincode::Decode<Context> for Signed {
    fn decode<D: bincode::de::Decoder<Context = Context>>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        let sign_bytes = <[u8; Signature::BYTE_SIZE] as bincode::Decode<Context>>::decode(decoder)?;
        let bytes = <Vec<u8> as bincode::Decode<Context>>::decode(decoder)?;

        Ok(Self {
            sign: Signature::from_bytes(&sign_bytes),
            bytes,
        })
    }
}

pub const ALPN: &[u8] = b"the-man";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Ticket {
    pub owner_id: NodeId,
    pub hash_and_format: HashAndFormat,
    pub ttl: u16,
}

impl bincode::enc::Encode for Ticket {
    fn encode<E: bincode::enc::Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), bincode::error::EncodeError> {
        bincode::Encode::encode(self.owner_id.as_bytes(), encoder)?;
        bincode::Encode::encode(self.hash_and_format.hash.as_bytes(), encoder)?;
        bincode::Encode::encode(&self.hash_and_format.format.is_hash_seq(), encoder)?;
        bincode::Encode::encode(&self.ttl, encoder)?;
        Ok(())
    }
}

impl<Context> bincode::de::Decode<Context> for Ticket {
    fn decode<D: bincode::de::Decoder<Context = Context>>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        let owner_id = <[u8; 32] as bincode::Decode<Context>>::decode(decoder)?;
        let hash = <[u8; 32] as bincode::Decode<Context>>::decode(decoder)?;
        let is_seq = <bool as bincode::Decode<Context>>::decode(decoder)?;
        let ttl = <u16 as bincode::Decode<Context>>::decode(decoder)?;

        Ok(Self {
            owner_id: NodeId::from_bytes(&owner_id).unwrap(),
            hash_and_format: HashAndFormat {
                hash: Hash::from_bytes(hash),
                format: if is_seq {
                    BlobFormat::HashSeq
                } else {
                    BlobFormat::Raw
                },
            },
            ttl,
        })
    }
}

impl<'a, Context> bincode::de::BorrowDecode<'a, Context> for Ticket {
    fn borrow_decode<D: bincode::de::BorrowDecoder<'a, Context = Context>>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        let owner_id = <[u8; 32] as bincode::BorrowDecode<Context>>::borrow_decode(decoder)?;
        let hash = <[u8; 32] as bincode::BorrowDecode<Context>>::borrow_decode(decoder)?;
        let is_seq = <bool as bincode::BorrowDecode<Context>>::borrow_decode(decoder)?;
        let ttl = <u16 as bincode::BorrowDecode<Context>>::borrow_decode(decoder)?;

        Ok(Self {
            owner_id: NodeId::from_bytes(&owner_id).unwrap(),
            hash_and_format: HashAndFormat {
                hash: Hash::from_bytes(hash),
                format: if is_seq {
                    BlobFormat::HashSeq
                } else {
                    BlobFormat::Raw
                },
            },
            ttl,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum TicketFor {
    Data,
    Message,
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

impl bincode::Encode for RawMessage {
    fn encode<E: bincode::enc::Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), bincode::error::EncodeError> {
        bincode::Encode::encode(&self.last, encoder)?;
        bincode::Encode::encode(&self.time.timestamp(), encoder)?;
        bincode::Encode::encode(&self.time.timestamp_subsec_nanos(), encoder)?;
        bincode::Encode::encode(&self.conversation, encoder)?;
        bincode::Encode::encode(&self.data, encoder)?;

        Ok(())
    }
}

impl<Context> bincode::Decode<Context> for RawMessage {
    fn decode<D: bincode::de::Decoder<Context = Context>>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        let last = <Option<Ticket> as bincode::Decode<Context>>::decode(decoder)?;
        let time_s = <i64 as bincode::Decode<Context>>::decode(decoder)?;
        let time_nanos = <u32 as bincode::Decode<Context>>::decode(decoder)?;
        let conversation = <Ticket as bincode::Decode<Context>>::decode(decoder)?;
        let data = <String as bincode::Decode<Context>>::decode(decoder)?;

        Ok(Self {
            last,
            time: Time::from_timestamp(time_s, time_nanos).unwrap(),
            conversation,
            data,
        })
    }
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

impl bincode::Encode for RawConversation {
    fn encode<E: bincode::enc::Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), bincode::error::EncodeError> {
        bincode::Encode::encode(
            unsafe { std::mem::transmute::<&Vec<NodeId>, &Vec<[u8; 32]>>(&self.nodes) },
            encoder,
        )?;
        bincode::Encode::encode(&self.time.timestamp(), encoder)?;
        bincode::Encode::encode(&self.time.timestamp_subsec_nanos(), encoder)?;

        Ok(())
    }
}

impl<Context> bincode::Decode<Context> for RawConversation {
    fn decode<D: bincode::de::Decoder<Context = Context>>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        let nodes = <Vec<[u8; 32]> as bincode::Decode<Context>>::decode(decoder)?;
        let time = <i64 as bincode::Decode<Context>>::decode(decoder)?;
        let nanos = <u32 as bincode::Decode<Context>>::decode(decoder)?;

        Ok(Self {
            nodes: unsafe { std::mem::transmute::<Vec<[u8; 32]>, Vec<NodeId>>(nodes) },
            time: Time::from_timestamp(time, nanos).unwrap(),
        })
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

    pub fn blocking_prev(&self) -> Option<Arc<TreeEntry>> {
        self.prev.blocking_read().clone()
    }

    pub async fn next(&self) -> Option<Arc<TreeEntry>> {
        self.nexts
            .read()
            .await
            .first()
            .and_then(|entry| entry.upgrade())
    }

    pub fn blocking_next(&self) -> Option<Arc<TreeEntry>> {
        self.nexts
            .blocking_read()
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

                error!("The last entry cannot be found");

                let entry = Arc::new(TreeEntry {
                    prev: RwLock::new(None),
                    hash,
                    nexts: RwLock::default(),
                });

                self.tails.push(entry);
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

#[derive(Debug, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
pub enum Packet {
    Welcome,
    SendMessage(Ticket),
    StartStream {
        conversation_id: [u8; 32],
        idx: u32,
        codec: String,
        settings: String,
    },
    PlayStream {
        conversation_id: [u8; 32],
        idx: u32,
        data: Vec<u8>,
    },
    StopStream {
        conversation_id: [u8; 32],
        idx: u32,
    },
}

#[derive(Debug, Clone)]
pub enum StreamEvent {
    Start {
        conversation_id: Hash,
        idx: u32,
        codec: String,
        settings: String,
    },
    Play {
        conversation_id: Hash,
        idx: u32,
        data: Arc<[u8]>,
    },
    Stop {
        conversation_id: Hash,
        idx: u32,
    },
}

impl StreamEvent {
    pub fn conversation_id(&self) -> Hash {
        match self {
            StreamEvent::Start {
                conversation_id, ..
            }
            | StreamEvent::Play {
                conversation_id, ..
            }
            | StreamEvent::Stop {
                conversation_id, ..
            } => *conversation_id,
        }
    }

    pub fn idx(&self) -> u32 {
        match self {
            StreamEvent::Start { idx, .. }
            | StreamEvent::Play { idx, .. }
            | StreamEvent::Stop { idx, .. } => *idx,
        }
    }
}

pub enum LazyData {
    ToLoad(Ticket),
    Loaded(Ticket, Arc<[u8]>),
}

impl LazyData {
    pub async fn get(&mut self, blobs: &iroh_blobs::net_protocol::Blobs) -> Option<Arc<[u8]>> {
        match self {
            LazyData::ToLoad(ticket) => match blobs.store().get_bytes(ticket.hash()).await {
                Ok(data) => {
                    let Ok((signed, _len)) =
                        bincode::decode_from_slice::<Signed, _>(&data, bincode::config::legacy())
                    else {
                        error!("Cannot parse Signed");
                        return None;
                    };
                    let Ok(data) = signed.get(&ticket.owner_id) else {
                        error!("Cannot verify");
                        return None;
                    };
                    let data: Arc<[u8]> = Vec::from(data).into();
                    *self = LazyData::Loaded(ticket.clone(), data.clone());
                    Some(data)
                }
                Err(err) => {
                    error!(
                        "Cannot read data: {}, {err}",
                        base64_serialize(ticket).unwrap()
                    );
                    None
                }
            },
            LazyData::Loaded(_, data) => Some(data.clone()),
        }
    }

    pub fn ticket(&mut self) -> &mut Ticket {
        match self {
            LazyData::ToLoad(ticket) => ticket,
            LazyData::Loaded(ticket, _) => ticket,
        }
    }
}

enum ServiceRequest {
    Create(RawConversation, oneshot::Sender<Option<Hash>>),
    List(oneshot::Sender<Vec<Hash>>),
    GetMessage(Hash, oneshot::Sender<Option<Message>>),
    GetConversation(Hash, oneshot::Sender<Option<Conversation>>),
    Send(RawMessage, u16, oneshot::Sender<Option<Hash>>),
    Recover(Ticket),
    RequestMessageSubscription(oneshot::Sender<watch::Receiver<Option<Message>>>),
    Add(Connection, (SendStream, RecvStream)),

    Get(Ticket, oneshot::Sender<Option<Arc<[u8]>>>),
    Store(Vec<u8>, u16, oneshot::Sender<Ticket>),

    SetTTL(TicketFor, Hash, u16),

    AddStreamSender(UnboundedSender<(NodeId, StreamEvent)>),
    SendStream(StreamEvent),
}

#[allow(dead_code)]
struct Conn {
    connection: Connection,
    sender: SendStream,
    receiver: Pin<Box<dyn futures_util::Stream<Item = Vec<Packet>> + Send + Sync>>,
}

struct TheManService {
    messages_to_resolv: Vec<Ticket>,
    conversations_to_resolv: Vec<Ticket>,
    datas_to_resolv: Vec<Ticket>,

    connections: BTreeMap<NodeId, Conn>,
    message_sender: watch::Sender<Option<Message>>,
    receiver: Receiver<ServiceRequest>,
    downloader: iroh_blobs::api::downloader::Downloader,
    blobs: iroh_blobs::net_protocol::Blobs,
    endpoint: iroh::Endpoint,

    messages: BTreeMap<Hash, Message>,
    dead_messages: BTreeSet<Hash>,

    conversations: BTreeMap<Hash, Conversation>,
    datas: BTreeMap<Hash, LazyData>,

    tickets_to_delete: BTreeMap<Hash, TicketFor>,

    every_second: tokio::time::Interval,

    stream_senders: Vec<UnboundedSender<(NodeId, StreamEvent)>>,
}

impl TheManService {
    pub async fn run(&mut self) {
        for ticket in std::mem::take(&mut self.datas_to_resolv) {
            if ticket.ttl != 0 {
                self.tickets_to_delete
                    .insert(ticket.hash(), TicketFor::Data);
            }
            self.datas.insert(ticket.hash(), LazyData::ToLoad(ticket));
        }

        let mut messages_to_add = std::mem::take(&mut self.messages_to_resolv);
        let mut conversations_to_add = std::mem::take(&mut self.conversations_to_resolv);
        loop {
            if messages_to_add.is_empty() && conversations_to_add.is_empty() {
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
                    _ = async { if !self.tickets_to_delete.is_empty() {self.every_second.tick().await} else {std::future::pending().await} } => {
                        self.handle_tick().await;
                    }
                    request = self.receiver.recv() => {
                        let Some(request) = request else{
                            break;
                        };

                        self.handle_request(request, &mut messages_to_add).await;
                    }
                }
            }

            for ticket in std::mem::take(&mut conversations_to_add) {
                if self.conversations.contains_key(&ticket.hash()) {
                    continue;
                }

                let Some(bytes) = self.get_signed_data(&ticket).await else {
                    error!(
                        "Cannot get conversation: {}",
                        base64_serialize(&ticket.hash()).unwrap()
                    );
                    continue;
                };

                self.blobs
                    .store()
                    .tags()
                    .set(tag_conversation(&ticket), ticket.hash_and_format)
                    .await
                    .unwrap();

                let Ok(raw) = bincode::decode_from_slice::<RawConversation, _>(
                    &bytes,
                    bincode::config::legacy(),
                ) else {
                    error!(
                        "Cannot parse conversation: {}",
                        base64_serialize(&ticket.hash()).unwrap()
                    );
                    continue;
                };

                let raw = raw.0;

                self.conversations.insert(
                    ticket.hash(),
                    Conversation {
                        raw,
                        ticket,
                        tails: vec![],
                    },
                );

                debug!("Conversation added");
                _ = self.message_sender.send(None);
            }

            let mut skip = false;
            for ticket in std::mem::take(&mut messages_to_add) {
                if self.messages.contains_key(&ticket.hash()) {
                    continue;
                }

                if skip {
                    messages_to_add.push(ticket);
                    continue;
                }

                let Some(bytes) = self.get_signed_data(&ticket).await else {
                    error!(
                        "Cannot get message: {}",
                        base64_serialize(&ticket.hash()).unwrap()
                    );
                    self.dead_messages.insert(ticket.hash());
                    continue;
                };

                self.blobs
                    .store()
                    .tags()
                    .set(tag_message(&ticket), ticket.hash_and_format)
                    .await
                    .unwrap();

                if ticket.ttl != 0 {
                    self.tickets_to_delete
                        .insert(ticket.hash(), TicketFor::Message);
                }

                let Ok(raw) =
                    bincode::decode_from_slice::<RawMessage, _>(&bytes, bincode::config::legacy())
                else {
                    error!(
                        "Cannot parse message: {}",
                        base64_serialize(&ticket.hash()).unwrap()
                    );
                    self.dead_messages.insert(ticket.hash());
                    continue;
                };

                let raw = raw.0;

                let Some(conversation) = self.conversations.get_mut(&raw.conversation.hash())
                else {
                    conversations_to_add.push(raw.conversation.clone());
                    messages_to_add.push(ticket);
                    skip = true;
                    continue;
                };

                if !conversation.raw.nodes.contains(&ticket.owner_id) {
                    debug!(
                        "Some body send a message in a conversation that is not part of, peer_id: {}, conversation: {}, message_ticket: {}",
                        base64_serialize(&ticket.owner_id).unwrap(),
                        base64_serialize(&raw.conversation.hash()).unwrap(),
                        base64_serialize(&ticket).unwrap()
                    );
                    _ = self.blobs.store().delete([ticket.hash()]).await;
                    continue;
                }

                if let Some(last) = raw.last.clone() {
                    if !self.messages.contains_key(&last.hash())
                        && !self.dead_messages.contains(&last.hash())
                    {
                        messages_to_add.push(last);
                        messages_to_add.push(ticket);
                        skip = true;
                        continue;
                    }
                }

                conversation
                    .add_message(ticket.hash(), raw.last.as_ref().map(|ticket| ticket.hash()))
                    .await;

                debug!(
                    "Added message: {}",
                    base64_serialize(&ticket.hash()).unwrap()
                );

                let message = Message { raw, ticket };
                self.messages.insert(message.ticket.hash(), message.clone());
                _ = self.message_sender.send(Some(message));
            }
        }
    }

    pub async fn download(&self, ticket: &Ticket) -> Option<bytes::Bytes> {
        let now = std::time::Instant::now();

        let handle = self
            .downloader
            .download_with_opts(DownloadRequest::new(
                HashAndFormat::new(ticket.hash(), ticket.format()),
                ticket.providers().copied().collect::<Vec<_>>(),
                iroh_blobs::api::downloader::SplitStrategy::None,
            ))
            .await;

        info!("Downloading: {}", base64_serialize(&ticket.hash()).unwrap());
        match handle {
            Ok(_) => {
                info!(
                    "Downloaded: {}, in time: {:0.2}",
                    base64_serialize(&ticket.hash()).unwrap(),
                    now.elapsed().as_secs_f32()
                );
            }
            Err(err) => {
                error!(
                    "Cannot download: {}, {err}",
                    base64_serialize(&ticket.hash()).unwrap()
                );
            }
        }

        let status = self.blobs.store().status(ticket.hash()).await.unwrap();

        match status {
            iroh_blobs::api::blobs::BlobStatus::Complete { size } => {
                println!("Blob with size: {size}");

                Some(self.blobs.store().get_bytes(ticket.hash()).await.unwrap())
            }
            _ => None,
        }
    }

    async fn get_signed_data(&self, ticket: &Ticket) -> Option<Vec<u8>> {
        let status = self.blobs.store().status(ticket.hash()).await.unwrap();
        let bytes = match status {
            iroh_blobs::api::blobs::BlobStatus::Complete { .. } => {
                self.blobs.store().get_bytes(ticket.hash()).await.unwrap()
            }
            iroh_blobs::api::blobs::BlobStatus::Partial { .. }
            | iroh_blobs::api::blobs::BlobStatus::NotFound => self.download(ticket).await?,
        };

        let Ok(signed) = bincode::decode_from_slice::<Signed, _>(&bytes, bincode::config::legacy())
        else {
            error!(
                "Cannot parse signed ticket data: {}",
                base64_serialize(&ticket).unwrap()
            );
            return None;
        };

        let signed = signed.0;

        let Ok(bytes) = signed.get(&ticket.owner_id) else {
            error!(
                "Cannot verify signiture of ticket: {}",
                base64_serialize(&ticket).unwrap()
            );
            return None;
        };

        Some(bytes.to_vec())
    }

    pub async fn handle_request(
        &mut self,
        request: ServiceRequest,
        messages_to_add: &mut Vec<Ticket>,
    ) {
        info_span!("handle_request");
        match request {
            ServiceRequest::Add(connection, (mut sender, receiver)) => {
                let node_id = connection
                    .remote_node_id()
                    .expect("Cannot get connection node_id");
                info!("Connected to: {}", base64_serialize(&node_id).unwrap());
                let mut buffer = Vec::default();
                bincode::encode_into_std_write(
                    &Packet::Welcome,
                    &mut buffer,
                    bincode::config::standard(),
                )
                .unwrap();
                sender
                    .write_all(&buffer)
                    .await
                    .expect("Cannot write welcome packet");

                const SIZE: usize = 1024 * 1024 * 100;

                let receiver = futures_util::stream::unfold(
                    (receiver, vec![0u8; SIZE], 0usize),
                    move |(mut receiver, mut buffer, mut pos)| async move {
                        match receiver.read(&mut buffer[pos..]).await {
                            Ok(Some(len)) => {
                                pos += len;
                                let mut packets = Vec::default();

                                loop {
                                    match bincode::decode_from_slice::<Packet, _>(
                                        &buffer[..pos],
                                        bincode::config::standard(),
                                    ) {
                                        Ok((packet, c)) => {
                                            packets.push(packet);

                                            buffer.copy_within(c..c + pos, 0);
                                            pos -= c;

                                            if pos == 0 {
                                                break;
                                            }
                                        }
                                        Err(err) => {
                                            if let bincode::error::DecodeError::UnexpectedEnd {
                                                ..
                                            } = &err
                                            {
                                                return Some((packets, (receiver, buffer, pos)));
                                            }

                                            if let bincode::error::DecodeError::Io {
                                                inner, ..
                                            } = &err
                                            {
                                                if let std::io::ErrorKind::UnexpectedEof =
                                                    inner.kind()
                                                {
                                                    return Some((
                                                        packets,
                                                        (receiver, buffer, pos),
                                                    ));
                                                }
                                            }
                                            error!(
                                                "Cannot read packet from: {}, {err}",
                                                base64_serialize(&node_id).unwrap()
                                            );
                                            return None;
                                        }
                                    }
                                }

                                Some((packets, (receiver, buffer, pos)))
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
                let bytes =
                    bincode::encode_to_vec(&raw_conversation, bincode::config::legacy()).unwrap();

                let node_id = self.blobs.endpoint().node_id();

                let bytes = bincode::encode_to_vec(
                    Signed::new(self.endpoint.secret_key(), bytes),
                    bincode::config::legacy(),
                )
                .unwrap();

                let Ok(res) = self.blobs.store().add_bytes(bytes).await else {
                    error!("Cannot add bytes");
                    if sender.send(None).is_err() {
                        error!("Cannot send");
                    }
                    return;
                };

                let ticket = Ticket {
                    owner_id: node_id,
                    hash_and_format: HashAndFormat {
                        hash: res.hash,
                        format: res.format,
                    },
                    ttl: 0,
                };

                self.blobs
                    .store()
                    .tags()
                    .set(tag_conversation(&ticket), ticket.hash_and_format)
                    .await
                    .unwrap();

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
                _ = self.message_sender.send(None);
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

            ServiceRequest::Send(raw_message, ttl, sender) => {
                let Some(conversation) =
                    self.conversations.get_mut(&raw_message.conversation.hash())
                else {
                    error!("Cannot find conversation");
                    if sender.send(None).is_err() {
                        error!("Cannot send");
                    }
                    return;
                };

                let Ok(bytes) = bincode::encode_to_vec(&raw_message, bincode::config::legacy())
                else {
                    error!("Cannot serialize message");
                    if sender.send(None).is_err() {
                        error!("Cannot send");
                    }
                    return;
                };

                let bytes = bincode::encode_to_vec(
                    Signed::new(self.endpoint.secret_key(), bytes),
                    bincode::config::legacy(),
                )
                .unwrap();

                let Ok(res) = self
                    .blobs
                    .store()
                    .add_bytes(bytes)
                    .await
                    .map_err(|err| error!("{err}: Cannot add message as blob"))
                else {
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
                    ttl,
                };

                self.blobs
                    .store()
                    .tags()
                    .set(tag_message(&ticket), ticket.hash_and_format)
                    .await
                    .unwrap();
                _ = self.blobs.store().tags().delete(res.name).await;

                if ticket.ttl != 0 {
                    self.tickets_to_delete
                        .insert(ticket.hash(), TicketFor::Message);
                }

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
                bincode::encode_into_std_write(
                    Packet::SendMessage(ticket),
                    &mut buffer,
                    bincode::config::standard(),
                )
                .unwrap();

                let self_node_id = self.endpoint.node_id();

                for node_id in conversation.raw.nodes.iter() {
                    if *node_id == self_node_id {
                        continue;
                    }

                    let Some(conn) = self.connections.get_mut(node_id) else {
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

            ServiceRequest::Get(ticket, sender) => {
                if let Some(data) = self.datas.get_mut(&ticket.hash()) {
                    _ = sender.send(data.get(&self.blobs).await);
                    return;
                }
                if let Some(res) = self.download(&ticket).await {
                    let data: Vec<u8> = res.into();
                    let data: Arc<[u8]> = data.into();
                    if ticket.ttl != 0 {
                        self.tickets_to_delete
                            .insert(ticket.hash(), TicketFor::Data);
                    }
                    self.datas
                        .insert(ticket.hash(), LazyData::Loaded(ticket, data.clone()));

                    _ = sender.send(Some(data));
                } else {
                    _ = sender.send(None);
                }
            }

            ServiceRequest::Store(data, ttl, sender) => {
                let arc: Arc<[u8]> = data.clone().into();
                let res = self
                    .blobs
                    .store()
                    .add_bytes(data)
                    .await
                    .expect("Cannot add to store");

                let ticket = Ticket {
                    owner_id: self.endpoint.node_id(),
                    hash_and_format: HashAndFormat {
                        hash: res.hash,
                        format: res.format,
                    },
                    ttl,
                };

                if ttl != 0 {
                    self.tickets_to_delete
                        .insert(ticket.hash(), TicketFor::Data);
                }

                self.datas
                    .insert(ticket.hash(), LazyData::Loaded(ticket.clone(), arc));

                if let Err(err) = self
                    .blobs
                    .store()
                    .tags()
                    .set(tag_data(&ticket), ticket.hash_and_format)
                    .await
                {
                    error!(
                        "Cannot set tag for data: {err}, {}",
                        base64_serialize(&ticket.hash()).unwrap()
                    );
                } else {
                    _ = sender.send(ticket);
                }
                _ = self.blobs.store().tags().delete(res.name).await;
            }

            ServiceRequest::SetTTL(f, hash, ttl) => {
                if ttl == 0 {
                    self.tickets_to_delete.remove(&hash);
                }
                match f {
                    TicketFor::Data => {
                        if let Some(lazy) = self.datas.get_mut(&hash) {
                            let last_ticket = lazy.ticket().clone();
                            let ticket = lazy.ticket();
                            ticket.ttl = ttl;
                            _ = self
                                .blobs
                                .store()
                                .tags()
                                .set(tag_data(ticket), ticket.hash_and_format)
                                .await;
                            _ = self
                                .blobs
                                .store()
                                .tags()
                                .delete(tag_data(&last_ticket))
                                .await;
                            self.tickets_to_delete.insert(hash, f);
                        }
                    }
                    TicketFor::Message => {
                        if let Some(msg) = self.messages.get_mut(&hash) {
                            let last_ticket = msg.ticket.clone();
                            msg.ticket.ttl = ttl;
                            _ = self
                                .blobs
                                .store()
                                .tags()
                                .set(tag_message(&msg.ticket), msg.ticket.hash_and_format)
                                .await;
                            _ = self
                                .blobs
                                .store()
                                .tags()
                                .delete(tag_message(&last_ticket))
                                .await;
                            self.tickets_to_delete.insert(hash, f);
                        }
                    }
                }
            }

            ServiceRequest::AddStreamSender(sender) => {
                self.stream_senders.retain(|s| !s.is_closed());

                self.stream_senders.push(sender);
            }

            ServiceRequest::SendStream(event) => {
                let Some(conversation) = self.conversations.get(&event.conversation_id()) else {
                    error!(
                        "Cannot find conversation to send stream event: {}",
                        base64_serialize(&event.conversation_id()).unwrap()
                    );
                    return;
                };

                let packet = match event {
                    StreamEvent::Start {
                        conversation_id,
                        idx,
                        codec,
                        settings,
                    } => Packet::StartStream {
                        conversation_id: *conversation_id.as_bytes(),
                        idx,
                        codec,
                        settings,
                    },
                    StreamEvent::Play {
                        conversation_id,
                        idx,
                        data,
                    } => Packet::PlayStream {
                        conversation_id: *conversation_id.as_bytes(),
                        idx,
                        data: data.to_vec(),
                    },
                    StreamEvent::Stop {
                        conversation_id,
                        idx,
                    } => Packet::StopStream {
                        conversation_id: *conversation_id.as_bytes(),
                        idx,
                    },
                };

                let mut buffer = Vec::default();
                if let Err(err) = bincode::encode_into_std_write(
                    &packet,
                    &mut buffer,
                    bincode::config::standard(),
                ) {
                    error!("Cannot encode packet: {err}");
                    return;
                }

                let self_node_id = self.endpoint.node_id();

                for node_id in conversation.raw.nodes.iter() {
                    if *node_id == self_node_id {
                        continue;
                    }

                    let Some(conn) = self.connections.get_mut(node_id) else {
                        warn!("Is not connected to {}", base64_serialize(node_id).unwrap());
                        continue;
                    };

                    if let Err(err) = conn.sender.write_all(&buffer).await {
                        error!(
                            "Cannot send to {} {err}",
                            base64_serialize(node_id).unwrap()
                        );
                    }
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
        tracing::info_span!("handle_packet", from = base64_serialize(&node_id).unwrap());
        match packet {
            Packet::Welcome => {
                debug!("Welcome from: {}", base64_serialize(&node_id).unwrap());

                let connection = self
                    .connections
                    .get_mut(&node_id)
                    .expect("Welcome message but no connection, HOW????");
                for conversation in self.conversations.values() {
                    if !conversation.raw.nodes.contains(&node_id) {
                        continue;
                    }

                    debug!(
                        "Sending conversation tails to: {}",
                        base64_serialize(&node_id).unwrap()
                    );

                    for tail in conversation.tails.iter() {
                        debug!("Sending tail: {}", base64_serialize(&tail.hash).unwrap());
                        let mut buffer = Vec::new();
                        let Some(msg) = self.messages.get(&tail.hash) else {
                            error!(
                                "Cannot send message entry because we don't have the message from the entry: {}",
                                base64_serialize(&tail.hash).unwrap()
                            );
                            continue;
                        };
                        bincode::encode_into_std_write(
                            Packet::SendMessage(msg.ticket.clone()),
                            &mut buffer,
                            bincode::config::standard(),
                        )
                        .expect("Cannot serialize Ticket???");
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
                debug!(
                    "Received {} from: {}",
                    base64_serialize(&ticket).unwrap(),
                    base64_serialize(&node_id).unwrap()
                );
                messages_to_add.push(ticket);
            }

            Packet::StartStream {
                conversation_id: conversation_hash,
                idx,
                codec,
                settings,
            } => {
                self.stream_senders.retain(|s| !s.is_closed());

                for sender in self.stream_senders.iter_mut() {
                    _ = sender.send((
                        node_id,
                        StreamEvent::Start {
                            conversation_id: Hash::from_bytes(conversation_hash),
                            idx,
                            codec: codec.clone(),
                            settings: settings.clone(),
                        },
                    ));
                }
            }

            Packet::PlayStream {
                conversation_id: conversation_hash,
                idx,
                data,
            } => {
                self.stream_senders.retain(|s| !s.is_closed());

                let data = Arc::<[u8]>::from(data);

                for sender in self.stream_senders.iter_mut() {
                    _ = sender.send((
                        node_id,
                        StreamEvent::Play {
                            conversation_id: Hash::from_bytes(conversation_hash),
                            idx,
                            data: data.clone(),
                        },
                    ));
                }
            }

            Packet::StopStream {
                conversation_id: conversation_hash,
                idx,
            } => {
                self.stream_senders.retain(|s| !s.is_closed());

                for sender in self.stream_senders.iter_mut() {
                    _ = sender.send((
                        node_id,
                        StreamEvent::Stop {
                            conversation_id: Hash::from_bytes(conversation_hash),
                            idx,
                        },
                    ));
                }
            }
        }
    }

    pub async fn handle_tick(&mut self) {
        tracing::info_span!("handle_tick");

        trace!("To Delete Tick");

        let mut to_remove = Vec::default();
        self.tickets_to_delete.retain(|hash, f| {
            let ticket = match f {
                TicketFor::Data => self.datas.get_mut(hash).unwrap().ticket(),
                TicketFor::Message => &mut self.messages.get_mut(hash).unwrap().ticket,
            };

            if ticket.ttl == 1 {
                to_remove.push((*f, *hash));
                return false;
            }

            ticket.ttl -= 1;

            true
        });

        for (f, hash) in to_remove {
            let ticket = match f {
                TicketFor::Data => self.datas.get_mut(&hash).unwrap().ticket(),
                TicketFor::Message => &mut self.messages.get_mut(&hash).unwrap().ticket,
            };

            match f {
                TicketFor::Data => {
                    if let Err(err) = self.blobs.store().tags().delete(tag_data(ticket)).await {
                        error!(
                            "Cannot delete data: {err}, {}",
                            base64_serialize(&hash).unwrap()
                        );
                    } else {
                        _ = self.datas.remove(&hash);
                    }
                }
                TicketFor::Message => {
                    if let Err(err) = self.blobs.store().tags().delete(tag_message(ticket)).await {
                        error!(
                            "Cannot delete message: {err}, {}",
                            base64_serialize(&hash).unwrap()
                        );
                    } else {
                        _ = self.messages.remove(&hash).unwrap();
                    }
                }
            }
        }

        for (hash, f) in self.tickets_to_delete.iter() {
            let ticket = match f {
                TicketFor::Data => self.datas.get_mut(hash).unwrap().ticket(),
                TicketFor::Message => &mut self.messages.get_mut(hash).unwrap().ticket,
            };

            match f {
                TicketFor::Data => {
                    debug!("Deleted data: {}", base64_serialize(&hash).unwrap());
                    if let Err(err) = self
                        .blobs
                        .store()
                        .tags()
                        .delete(tag_data(&Ticket {
                            ttl: ticket.ttl + 1,
                            ..ticket.clone()
                        }))
                        .await
                    {
                        error!(
                            "Cannot begin refresh data: {err}, {}",
                            base64_serialize(hash).unwrap()
                        );
                    } else if let Err(err) = self
                        .blobs
                        .store()
                        .tags()
                        .set(tag_data(ticket), ticket.hash_and_format)
                        .await
                    {
                        error!(
                            "Cannot end refresh data: {err}, {}",
                            base64_serialize(hash).unwrap()
                        );
                    }
                }
                TicketFor::Message => {
                    debug!("Deleted message: {}", base64_serialize(hash).unwrap());
                    if let Err(err) = self
                        .blobs
                        .store()
                        .tags()
                        .delete(tag_message(&Ticket {
                            ttl: ticket.ttl + 1,
                            ..ticket.clone()
                        }))
                        .await
                    {
                        error!(
                            "Cannot begin refresh message: {err}, {}",
                            base64_serialize(hash).unwrap()
                        );
                    } else if let Err(err) = self
                        .blobs
                        .store()
                        .tags()
                        .set(tag_message(ticket), ticket.hash_and_format)
                        .await
                    {
                        error!(
                            "Cannot end refresh message: {err}, {}",
                            base64_serialize(hash).unwrap()
                        );
                    }
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct ConversationHandle {
    inner: Arc<Inner>,
    conversation: Hash,
    last: Option<Ticket>,
}

impl ConversationHandle {
    pub fn hash(&self) -> &Hash {
        &self.conversation
    }

    pub async fn ticket(&self) -> Ticket {
        self.get().await.ticket
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

    pub async fn send(&mut self, msg: impl Into<String>, ttl: u16) {
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
                ttl,
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
            } else {
                error!("Cannot get the sent message.");
            }
        } else {
            error!("Cannot send a message");
        }
    }
}

#[derive(Debug)]
struct Inner {
    pub sender: RwLock<Option<Sender<ServiceRequest>>>,
    #[allow(unused)]
    pub blobs: Blobs,
}

impl Inner {
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
pub struct TheMan {
    inner: Arc<Inner>,
    endpoint: Endpoint,
    task: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

impl TheMan {
    pub async fn spawn(
        blobs: Blobs,
        endpoint: Endpoint,
        message_sender: watch::Sender<Option<Message>>,
    ) -> Self {
        let mut messages_to_resolv = Vec::new();
        let mut conversations_to_resolv = Vec::new();
        let mut datas_to_resolv = Vec::new();

        let mut tags = blobs.store().tags().list().await.unwrap();

        while let Some(Ok(tag)) = tags.next().await {
            debug!("TAG: {:?}", tag.name.0);

            if tag.name.0.starts_with(b"message/") {
                let ticket_data = tag.name.0.strip_prefix(b"message/").unwrap();
                let ticket = base64_deserialize::<Ticket>(ticket_data).expect("Cannot deserialize");
                messages_to_resolv.push(ticket);
                continue;
            }

            if tag.name.0.starts_with(b"conversation/") {
                let ticket_data = tag.name.0.strip_prefix(b"conversation/").unwrap();
                let ticket = base64_deserialize::<Ticket>(ticket_data).expect("Cannot deserialize");
                conversations_to_resolv.push(ticket);
                continue;
            }

            if tag.name.0.starts_with(b"data/") {
                let ticket_data = tag.name.0.strip_prefix(b"data/").unwrap();
                let ticket = base64_deserialize::<Ticket>(ticket_data).expect("Cannot deserialize");
                datas_to_resolv.push(ticket);
                continue;
            }

            if tag.name.0.starts_with(b"auto-") {
                warn!("Temp Tag detected!!!");
                _ = blobs.store().tags().delete(tag.name.0).await;

                continue;
            }
        }

        let (sender, receiver) = tokio::sync::mpsc::channel(16);

        let task = {
            let blobs = blobs.clone();
            let endpoint = endpoint.clone();
            tokio::spawn(async move {
                TheManService {
                    messages_to_resolv,
                    conversations_to_resolv,
                    datas_to_resolv,
                    receiver,
                    downloader: blobs.store().downloader(&endpoint).clone(),
                    messages: BTreeMap::default(),
                    conversations: BTreeMap::default(),
                    endpoint,
                    message_sender,
                    connections: BTreeMap::new(),
                    datas: BTreeMap::default(),
                    blobs,
                    tickets_to_delete: Default::default(),
                    every_second: tokio::time::interval(tokio::time::Duration::from_secs(1)),
                    dead_messages: Default::default(),
                    stream_senders: Default::default(),
                }
                .run()
                .await;

                info!("TheMan Service stopped!");
            })
        };

        Self {
            inner: Arc::new(Inner {
                sender: RwLock::new(Some(sender)),
                blobs,
            }),
            endpoint,
            task: Arc::new(Mutex::new(Some(task))),
        }
    }

    pub async fn create(&self, raw: RawConversation) -> Option<ConversationHandle> {
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

    pub async fn conversations(&self) -> Option<Vec<ConversationHandle>> {
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

    pub async fn get_conversation(&self, hash: Hash) -> Option<ConversationHandle> {
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

    pub async fn send_message(&self, raw: RawMessage, ttl: u16) -> Option<Hash> {
        let (s, r) = oneshot::channel();
        self.inner
            .send_request(ServiceRequest::Send(raw, ttl, s))
            .await;
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

    pub async fn store(&self, data: Vec<u8>, ttl: u16) -> Ticket {
        let (sender, receiver) = oneshot::channel();
        self.inner
            .send_request(ServiceRequest::Store(data, ttl, sender))
            .await;

        receiver.await.unwrap()
    }

    pub async fn get(&self, ticket: Ticket) -> Vec<u8> {
        let (sender, receiver) = oneshot::channel();
        self.inner
            .send_request(ServiceRequest::Get(ticket, sender))
            .await;
        receiver.await.unwrap().expect("Cannot get data").to_vec()
    }

    pub async fn message_set_ttl(&self, hash: Hash, ttl: u16) {
        self.inner
            .send_request(ServiceRequest::SetTTL(TicketFor::Message, hash, ttl))
            .await;
    }

    pub async fn add_stream_sender(&self, sender: UnboundedSender<(NodeId, StreamEvent)>) {
        self.inner
            .send_request(ServiceRequest::AddStreamSender(sender))
            .await;
    }

    pub async fn send_stream(&self, event: StreamEvent) {
        self.inner
            .send_request(ServiceRequest::SendStream(event))
            .await;
    }
}

impl ProtocolHandler for TheMan {
    async fn accept(&self, connection: iroh::endpoint::Connection) -> Result<(), AcceptError> {
        let inner = self.inner.clone();
        let bi = connection
            .accept_bi()
            .await
            .inspect_err(|err| error!("Channel Fail: {err}"))?;
        inner
            .send_request(ServiceRequest::Add(connection, bi))
            .await;

        Ok(())
    }

    fn shutdown(&self) -> impl Future<Output = ()> {
        let inner = self.inner.clone();
        let task = self.task.clone();
        async move {
            inner.sender.write().await.take();
            if let Some(task) = task.lock().await.take() {
                _ = task.await;
            }
        }
    }
}

fn tag_conversation(ticket: &Ticket) -> iroh_blobs::api::Tag {
    iroh_blobs::api::Tag(format!("conversation/{}", base64_serialize(ticket).unwrap()).into())
}

fn tag_message(ticket: &Ticket) -> iroh_blobs::api::Tag {
    iroh_blobs::api::Tag(format!("message/{}", base64_serialize(ticket).unwrap()).into())
}

fn tag_data(ticket: &Ticket) -> iroh_blobs::api::Tag {
    iroh_blobs::api::Tag(format!("data/{}", base64_serialize(ticket).unwrap()).into())
}
