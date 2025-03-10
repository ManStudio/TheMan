use std::{
    collections::{BTreeMap, VecDeque},
    str::FromStr,
    sync::Arc,
};

use base64::{prelude::BASE64_URL_SAFE_NO_PAD, Engine as _};
use cpal::{
    traits::{DeviceTrait, HostTrait},
    Stream,
};
use iroh::{endpoint::RemoteInfo, protocol::Router, NodeId, SecretKey};
pub mod protocol;
use iroh_blobs::Hash;
use media_man::{CodecAudioOpus, TCodecAudio};
use protocol::{ConversationHandle, Message, RawConversation, TheMan as ProtocolTheMan, Ticket};
use serde::{de::DeserializeOwned, Serialize};
use tracing::{error, info, warn};

pub struct CommandData {
    ticket: Ticket,
    alt: String,
}

pub enum CommandAuto {
    Start {
        idx: u32,
        codec_name: String,
        codec_settings: BTreeMap<String, String>,
    },
    Play {
        idx: u32,
        ticket: Ticket,
    },
    Stop {
        idx: u32,
    },
}

pub enum Command {
    Data(CommandData),
    Auto(CommandAuto),
}

impl std::str::FromStr for Command {
    type Err = ();

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if !text.starts_with('/') {
            return Err(());
        }

        let text = &text[1..];

        let mut iterator = text.split(' ');

        let Some(command) = iterator.next() else {
            return Err(());
        };

        match command {
            "data" => {
                let Some(ticket_data) = iterator.next() else {
                    return Err(());
                };

                let Ok(ticket) = base64_deserialize::<Ticket>(ticket_data) else {
                    return Err(());
                };

                let mut alt = iterator.fold(String::default(), |mut acc, segment| {
                    acc.push_str(segment);
                    acc.push(' ');
                    acc
                });

                if alt.ends_with(' ') {
                    alt.pop();
                }

                Ok(Self::Data(CommandData { ticket, alt }))
            }
            "auto" => {
                let Some(subcommand) = iterator.next() else {
                    return Err(());
                };

                match subcommand {
                    "start" => {
                        let Some(idx_text) = iterator.next() else {
                            return Err(());
                        };

                        let Ok(idx) = u32::from_str(idx_text) else {
                            return Err(());
                        };

                        let Some(codec_name) = iterator.next() else {
                            return Err(());
                        };

                        let codec_name = codec_name.to_owned();

                        let Some(codec_settings_text) = iterator.next() else {
                            return Err(());
                        };

                        let Ok(codec_settings) =
                            base64_deserialize::<BTreeMap<String, String>>(codec_settings_text)
                        else {
                            return Err(());
                        };

                        Ok(Self::Auto(CommandAuto::Start {
                            idx,
                            codec_name,
                            codec_settings,
                        }))
                    }
                    "play" => {
                        let Some(idx_text) = iterator.next() else {
                            return Err(());
                        };

                        let Ok(idx) = u32::from_str(idx_text) else {
                            return Err(());
                        };

                        let Some(ticket_text) = iterator.next() else {
                            return Err(());
                        };

                        let Ok(ticket) = base64_deserialize::<Ticket>(ticket_text) else {
                            return Err(());
                        };

                        Ok(Self::Auto(CommandAuto::Play { idx, ticket }))
                    }
                    "stop" => {
                        let Some(idx_text) = iterator.next() else {
                            return Err(());
                        };

                        let Ok(idx) = u32::from_str(idx_text) else {
                            return Err(());
                        };

                        Ok(Self::Auto(CommandAuto::Stop { idx }))
                    }
                    _ => Err(()),
                }
            }
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Command::Data(CommandData { ticket, alt }) => f.write_fmt(format_args!(
                "/data {} {alt}",
                base64_serialize(ticket).unwrap()
            )),
            Command::Auto(command_auto) => match command_auto {
                CommandAuto::Start {
                    idx,
                    codec_name,
                    codec_settings,
                } => f.write_fmt(format_args!(
                    "/auto start {idx} {codec_name} {}",
                    base64_serialize(codec_settings).unwrap()
                )),
                CommandAuto::Play { idx, ticket } => f.write_fmt(format_args!(
                    "/auto play {idx} {}",
                    base64_serialize(ticket).unwrap()
                )),
                CommandAuto::Stop { idx } => f.write_fmt(format_args!("/auto stop {idx}")),
            },
        }
    }
}

pub struct InputStream {
    stream: Option<Stream>,
    task: tokio::task::JoinHandle<()>,
    sender: tokio::sync::mpsc::Sender<()>,
}

unsafe impl Send for InputStream {}
unsafe impl Sync for InputStream {}

pub struct OutputStream {
    sender: std::sync::mpsc::Sender<media_man::Packet>,
    stream: Stream,
}

unsafe impl Send for OutputStream {}
unsafe impl Sync for OutputStream {}

#[derive(Default)]
pub struct ActiveConversation {
    input_streams: BTreeMap<u32, InputStream>,
    output_streams: BTreeMap<NodeId, BTreeMap<u32, OutputStream>>,
}

enum ServiceRequest {
    AddDefault(Hash, Hash),
    StopDefault(Hash),
}

struct TheManService {
    protocol: ProtocolTheMan<iroh_blobs::store::fs::Store>,
    node_id: NodeId,
    message_receiver: tokio::sync::watch::Receiver<Option<Message>>,
    receiver: tokio::sync::mpsc::Receiver<ServiceRequest>,

    host: cpal::Host,
    audio_codecs: Vec<Box<dyn TCodecAudio>>,

    conversations: BTreeMap<Hash, ActiveConversation>,
}

impl TheManService {
    pub fn new(
        protocol: ProtocolTheMan<iroh_blobs::store::fs::Store>,
        node_id: NodeId,
        message_receiver: tokio::sync::watch::Receiver<Option<Message>>,
        receiver: tokio::sync::mpsc::Receiver<ServiceRequest>,
    ) -> Self {
        let host = cpal::default_host();
        info!("Using audio host: {}", host.id().name());

        let mut audio_codecs = Vec::<Box<dyn TCodecAudio>>::default();
        if let Some(opus) = CodecAudioOpus::new() {
            audio_codecs.push(Box::new(opus));
        } else {
            error!("Cannot load opus!");
        }

        Self {
            protocol,
            node_id,
            message_receiver,
            receiver,

            host,
            audio_codecs,
            conversations: BTreeMap::default(),
        }
    }

    pub async fn run(mut self) {
        loop {
            let message_receiver = {
                let mut i = 0;
                self.message_receiver.wait_for(move |v| {
                    let res = v.is_some() && i > 0;
                    i += 1;
                    res
                })
            };

            tokio::select! {
                message = async {
                    message_receiver.await
                    .expect("Cannot receive message from the the-man service.")
                    .clone()
                    .unwrap()
                } => {
                    if let Ok(command) = Command::from_str(&message.raw.data) {
                        self.handle_command(message, command).await;
                    }
                }
                Some(request) = self.receiver.recv() => {
                    self.handle_request(request).await;
                }
            }
        }
    }

    async fn handle_command(&mut self, message: Message, command: Command) {
        match &command {
            Command::Data(command_data) => warn!("Data command is not implemented."),
            Command::Auto(command_auto) => {
                // auto command can only be received from other nodes.
                if message.ticket.owner_id == self.node_id {
                    return;
                }

                let Some(conversation) = self
                    .protocol
                    .get_conversation(message.raw.conversation.hash())
                    .await
                else {
                    error!(
                        "Cannot find conversation for {command} from {}",
                        base64_serialize(&message.ticket.owner_id).unwrap()
                    );
                    return;
                };

                if !conversation
                    .get()
                    .await
                    .raw
                    .nodes
                    .contains(&message.ticket.owner_id)
                {
                    error!(
                        "{} send a command to a conversation that is not part of",
                        base64_serialize(&message.ticket.owner_id).unwrap()
                    );
                    return;
                }

                match command_auto {
                    CommandAuto::Start {
                        idx,
                        codec_name,
                        codec_settings,
                    } => {
                        for audio_codec in self.audio_codecs.iter() {
                            if audio_codec.name() != *codec_name {
                                continue;
                            }

                            let (sender, receiver) =
                                std::sync::mpsc::channel::<media_man::Packet>();
                            let stream = {
                                let decoder_settings = audio_codec
                                    .default_decoder_settings(
                                        media_man::SampleFormat::F32,
                                        48000,
                                        1,
                                    )
                                    .unwrap();
                                let mut decoder =
                                    audio_codec.create_decoder(decoder_settings).unwrap();

                                let default_output = self
                                    .host
                                    .default_output_device()
                                    .expect("Cannot find default audio output device");
                                info!(
                                    "Using default audio output device: {:?}",
                                    default_output.name()
                                );

                                let mut buffer = VecDeque::<f32>::default();
                                let idx = *idx;
                                default_output
                                    .build_output_stream(
                                        &cpal::StreamConfig {
                                            channels: 1,
                                            sample_rate: cpal::SampleRate(48000),
                                            buffer_size: cpal::BufferSize::Fixed(960),
                                        },
                                        move |samples: &mut [f32], _info| {
                                            while let Ok(packet) = receiver.try_recv() {
                                                match decoder.decode(packet) {
                                                    Ok(mut frames) => {
                                                        buffer.extend(frames.remove(0).to_f32())
                                                    }
                                                    Err(err) => {
                                                        error!("{err:?} when decoding for {idx}");
                                                    }
                                                }
                                            }

                                            for sample in samples {
                                                *sample = buffer.pop_front().unwrap_or(0.0);
                                            }
                                        },
                                        |err| error!("Default output error: {err}"),
                                        None,
                                    )
                                    .expect("Cannot create default output audio stream.")
                            };

                            let conversation = self
                                .conversations
                                .entry(message.raw.conversation.hash())
                                .or_default();
                            let output = conversation
                                .output_streams
                                .entry(message.ticket.owner_id)
                                .or_default();
                            output.insert(*idx, OutputStream { sender, stream });
                        }
                    }
                    CommandAuto::Play { idx, ticket } => {
                        let Some(conversation) =
                            self.conversations.get_mut(&message.raw.conversation.hash())
                        else {
                            error!("Play before start???");
                            return;
                        };

                        let Some(output) = conversation
                            .output_streams
                            .get_mut(&message.ticket.owner_id)
                        else {
                            error!("Play before start???");
                            return;
                        };

                        let Some(output_stream) = output.get_mut(idx) else {
                            error!("Play before start???");
                            return;
                        };

                        let data = self.protocol.get(ticket.clone()).await;

                        if output_stream
                            .sender
                            .send(media_man::Packet { data })
                            .is_err()
                        {
                            output.remove(idx);
                        }
                    }
                    CommandAuto::Stop { idx } => {
                        let Some(conversation) =
                            self.conversations.get_mut(&message.raw.conversation.hash())
                        else {
                            error!("Stop before start???");
                            return;
                        };

                        let Some(output) = conversation
                            .output_streams
                            .get_mut(&message.ticket.owner_id)
                        else {
                            error!("Stop before start???");
                            return;
                        };

                        output.remove(idx);
                    }
                }
            }
        }
    }

    async fn handle_request(&mut self, request: ServiceRequest) {
        match request {
            ServiceRequest::AddDefault(conversation_id, last_message) => {
                let mut conversation = self
                    .protocol
                    .get_conversation(conversation_id)
                    .await
                    .unwrap();
                let last = self
                    .protocol
                    .get_message(last_message)
                    .await
                    .expect("Cannot get message")
                    .ticket;
                conversation.set_last(last);

                conversation
                    .send(format!(
                        "{}",
                        Command::Auto(CommandAuto::Start {
                            idx: 0,
                            codec_name: "opus".into(),
                            codec_settings: BTreeMap::default()
                        })
                    ))
                    .await;

                let (p_sender, mut p_receiver) = tokio::sync::mpsc::channel::<Vec<u8>>(8);
                let (s_sender, mut s_receiver) = tokio::sync::mpsc::channel::<()>(1);
                let stream = {
                    let codec = self
                        .audio_codecs
                        .iter()
                        .find(|codec| codec.name() == "opus")
                        .expect("Cannot get opus codec");
                    let encoder_settings = codec
                        .default_encoder_settings(media_man::SampleFormat::F32, 48000, 1)
                        .unwrap();
                    let mut encoder = codec.create_encoder(encoder_settings).unwrap();

                    let input_device = self
                        .host
                        .default_input_device()
                        .expect("Cannot get the default audio input device.");
                    input_device
                        .build_input_stream(
                            &cpal::StreamConfig {
                                channels: 1,
                                sample_rate: cpal::SampleRate(48000),
                                buffer_size: cpal::BufferSize::Fixed(960),
                            },
                            move |samples: &[f32], _info| {
                                encoder
                                    .encode(&[&media_man::FrameAudio::f32_new(samples.to_vec())])
                                    .expect("Cannot encode");
                                while let Some(packet) = encoder.get_packet() {
                                    p_sender.try_send(packet.data).expect("Cannot send packet");
                                }
                            },
                            |err| error!("{err} from default input stream"),
                            None,
                        )
                        .unwrap()
                };

                let active = self.conversations.entry(conversation_id).or_default();
                let task = {
                    let protocol = self.protocol.clone();
                    tokio::spawn(async move {
                        loop {
                            tokio::select! {
                                Some(packet) = p_receiver.recv() => {
                                    let ticket = protocol.store(packet).await;

                                    conversation
                                        .send(format!("{}",
                                            Command::Auto(CommandAuto::Play { idx: 0, ticket })
                                        ))
                                        .await;
                                }
                                Some(_shutdown) = s_receiver.recv() => {
                                    conversation.send(format!("{}", Command::Auto(CommandAuto::Stop { idx: 0 }))).await;
                                    break;
                                }
                            }
                        }
                    })
                };
                active.input_streams.insert(
                    0,
                    InputStream {
                        stream: Some(stream),
                        task,
                        sender: s_sender,
                    },
                );
            }
            ServiceRequest::StopDefault(conversation_id) => {
                let active = self.conversations.entry(conversation_id).or_default();
                let Some(mut stream) = active.input_streams.remove(&0) else {
                    error!("There is no input stream to remove");
                    return;
                };

                _ = stream.stream.take();

                stream.sender.send(()).await.unwrap();
                stream.task.await.unwrap();
            }
        }
    }
}

#[derive(Clone)]
pub struct TheMan {
    node: Router,
    gossip: iroh_gossip::net::Gossip,
    protocol: ProtocolTheMan<iroh_blobs::store::fs::Store>,
    message_receiver: tokio::sync::watch::Receiver<Option<Message>>,
    sender: tokio::sync::mpsc::Sender<ServiceRequest>,

    task: Arc<tokio::task::JoinHandle<()>>,
}

unsafe impl Send for TheMan {}
unsafe impl Sync for TheMan {}

impl std::fmt::Debug for TheMan {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Ok(())
    }
}

impl TheMan {
    pub async fn new(name: String, secret_key: SecretKey) -> Self {
        let endpoint = iroh::Endpoint::builder()
            .discovery_n0()
            .secret_key(secret_key)
            .bind()
            .await
            .unwrap();

        let gossip = iroh_gossip::net::Gossip::builder()
            .spawn(endpoint.clone())
            .await
            .unwrap();

        let blobs = iroh_blobs::net_protocol::Blobs::persistent(format!("{name}-store"))
            .await
            .expect("Cannot create store")
            .build(&endpoint);

        let sender = tokio::sync::watch::Sender::<Option<Message>>::new(None);
        let message_receiver = sender.subscribe();

        let protocol = protocol::TheMan::spawn(blobs.clone(), endpoint.clone(), sender).await;

        println!("NodeId: {}", base64_serialize(&endpoint.node_id()).unwrap());

        let (sender, receiver) = tokio::sync::mpsc::channel::<ServiceRequest>(8);
        let task = {
            let protocol = protocol.clone();
            let node_id = endpoint.node_id();
            let message_receiver = message_receiver.clone();
            tokio::spawn(async move {
                TheManService::new(protocol, node_id, message_receiver, receiver)
                    .run()
                    .await;
            })
        };

        info!("Create node");
        let node = iroh::protocol::Router::builder(endpoint)
            .accept(iroh_gossip::ALPN, gossip.clone())
            .accept(iroh_blobs::ALPN, blobs.clone())
            .accept(protocol::ALPN, protocol.clone())
            .spawn()
            .await
            .unwrap();

        TheMan {
            node,
            gossip,
            protocol,
            task: Arc::new(task),
            message_receiver,
            sender,
        }
    }

    pub fn subscribe_messages(&self) -> tokio::sync::watch::Receiver<Option<Message>> {
        self.message_receiver.clone()
    }

    pub async fn send_message(&self, raw: protocol::RawMessage) -> Option<iroh_blobs::Hash> {
        self.protocol.send_message(raw).await
    }

    pub async fn get_message(&self, hash: iroh_blobs::Hash) -> Option<Message> {
        self.protocol.get_message(hash).await
    }

    pub async fn get_conversation(
        &self,
        hash: iroh_blobs::Hash,
    ) -> Option<ConversationHandle<iroh_blobs::store::fs::Store>> {
        self.protocol.get_conversation(hash).await
    }

    pub async fn raw_conversations(&self) -> Option<Vec<iroh_blobs::Hash>> {
        self.protocol.raw_conversations().await
    }

    pub async fn create(
        &self,
        raw: RawConversation,
    ) -> Option<ConversationHandle<iroh_blobs::store::fs::Store>> {
        self.protocol.create(raw).await
    }

    pub async fn recover(&self, ticket: Ticket) {
        self.protocol.recover(ticket).await
    }

    pub fn node_id(&self) -> NodeId {
        self.node.endpoint().node_id()
    }

    pub fn secret(&self) -> SecretKey {
        self.node.endpoint().secret_key().clone()
    }

    pub async fn add_conversation_default_stream(
        &self,
        conversation_id: Hash,
        reply_to_message: Hash,
    ) {
        self.sender
            .send(ServiceRequest::AddDefault(
                conversation_id,
                reply_to_message,
            ))
            .await
            .unwrap();
    }

    pub async fn stop_conversation_default_stream(&self, conversation_id: Hash) {
        self.sender
            .send(ServiceRequest::StopDefault(conversation_id))
            .await
            .unwrap();
    }

    pub fn remote_info(&self, node_id: NodeId) -> Option<RemoteInfo> {
        self.node.endpoint().remote_info(node_id)
    }

    pub fn remote_infos(&self) -> Vec<RemoteInfo> {
        self.node.endpoint().remote_info_iter().collect()
    }

    pub async fn connect(&self, node_id: NodeId) {
        self.protocol.connect(node_id).await;
    }
}

pub fn base64_serialize<T: Serialize>(value: &T) -> Result<String, Base64DecodeError> {
    let value = bincode::serde::encode_to_vec(value, bincode::config::legacy())?;
    Ok(BASE64_URL_SAFE_NO_PAD.encode(value))
}

#[derive(Debug)]
pub enum Base64DecodeError {
    BincodeDecode(bincode::error::DecodeError),
    BincodeEncode(bincode::error::EncodeError),
    Base64(base64::DecodeError),
}

impl From<base64::DecodeError> for Base64DecodeError {
    fn from(value: base64::DecodeError) -> Self {
        Self::Base64(value)
    }
}

impl From<bincode::error::DecodeError> for Base64DecodeError {
    fn from(value: bincode::error::DecodeError) -> Self {
        Self::BincodeDecode(value)
    }
}

impl From<bincode::error::EncodeError> for Base64DecodeError {
    fn from(value: bincode::error::EncodeError) -> Self {
        Self::BincodeEncode(value)
    }
}

pub fn base64_deserialize<T: DeserializeOwned>(
    value: impl AsRef<[u8]>,
) -> Result<T, Base64DecodeError> {
    let bytes = BASE64_URL_SAFE_NO_PAD.decode(value)?;
    Ok(bincode::serde::decode_from_slice(&bytes, bincode::config::legacy())?.0)
}
