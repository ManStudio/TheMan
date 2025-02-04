use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use base64::{prelude::BASE64_URL_SAFE_NO_PAD, Engine as _};
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Host, Stream,
};
use iroh::{protocol::Router, NodeId, SecretKey};
pub mod protocol;
use iroh_blobs::Hash;
use media_man::{CodecAudioOpus, FrameAudio, SampleFormat, TCodecAudio};
use protocol::{ConversationHandle, Message, RawConversation, TheMan as ProtocolTheMan, Ticket};
use serde::{de::DeserializeOwned, Serialize};
use tokio::sync::Mutex;
use tracing::{error, info};

pub struct InputStream {
    stream: Stream,
    tasks: Arc<std::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>>,
}

unsafe impl Send for InputStream {}
unsafe impl Sync for InputStream {}

pub struct OutputStream {
    sender: std::sync::mpsc::Sender<media_man::Packet>,
    stream: Stream,
}

const FRAME_SIZE: usize = 960;

unsafe impl Send for OutputStream {}
unsafe impl Sync for OutputStream {}

pub struct ActiveConversation {
    input_streams: Vec<InputStream>,
    output_streams: Vec<OutputStream>,
}

#[derive(Clone)]
pub struct TheMan {
    host: Arc<Host>,
    node: Router,
    gossip: iroh_gossip::net::Gossip,
    protocol: ProtocolTheMan<iroh_blobs::store::mem::Store>,
    local_pool: Arc<iroh_blobs::util::local_pool::LocalPool>,
    message_receiver: tokio::sync::watch::Receiver<Option<Message>>,
    conversations: Arc<Mutex<HashMap<iroh_blobs::Hash, ActiveConversation>>>,
    codec: Arc<dyn TCodecAudio>,

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
    pub async fn new(secret_key: SecretKey) -> Self {
        let endpoint = iroh::Endpoint::builder()
            .discovery_n0()
            .discovery_dht()
            .secret_key(secret_key)
            .bind()
            .await
            .unwrap();

        let gossip = iroh_gossip::net::Gossip::builder()
            .spawn(endpoint.clone())
            .await
            .unwrap();

        let local_pool = iroh_blobs::util::local_pool::LocalPool::default();

        let blobs = iroh_blobs::net_protocol::Blobs::memory().build(local_pool.handle(), &endpoint);

        let protocol =
            protocol::TheMan::spawn(blobs.clone(), endpoint.clone(), local_pool.handle()).await;

        info!("Create node");
        let node = iroh::protocol::Router::builder(endpoint)
            .accept(iroh_gossip::ALPN, gossip.clone())
            .accept(iroh_blobs::ALPN, blobs.clone())
            .accept(protocol::ALPN, protocol.clone())
            .spawn()
            .await
            .unwrap();

        println!(
            "NodeId: {}",
            base64_serialize(&node.endpoint().node_id()).unwrap()
        );

        let message_receiver = protocol.subscribe_messages().await;

        let host = cpal::default_host();
        println!("Audio host: {}", host.id().name());
        let host = Arc::new(host);

        let conversations: Arc<Mutex<HashMap<iroh_blobs::Hash, ActiveConversation>>> =
            Arc::default();
        let codec = Arc::new(CodecAudioOpus::new().expect("Cannot create opus codec"));
        let task;

        {
            let opus = codec.clone();
            let conversations = conversations.clone();
            let mut message_receiver = message_receiver.clone();
            let host = host.clone();
            let protocol = protocol.clone();
            let node_id = node.endpoint().node_id();
            task = tokio::spawn(async move {
                let opus = opus.clone();
                loop {
                    let opus = opus.clone();
                    let mut i = 0;
                    let message = message_receiver
                        .wait_for(|m| {
                            let ret = i != 0 && m.is_some();
                            i += 1;
                            ret
                        })
                        .await;

                    let message = message.unwrap().clone();

                    let Some(message) = message else {
                        continue;
                    };

                    if message.ticket.owner_id == node_id {
                        continue;
                    }

                    if !message.raw.data.starts_with('/') {
                        continue;
                    }

                    info!("Command: {}", message.raw.data);

                    let mut command_iter = message.raw.data.split(' ');
                    let command = command_iter.next().unwrap();

                    match command {
                        "/auto" => {
                            let stream_index = command_iter.next().unwrap();
                            let ticket = command_iter.next().unwrap();
                            let data = protocol
                                .get(base64_deserialize::<Ticket>(ticket).unwrap())
                                .await;

                            let mut conversations = conversations.lock().await;

                            let conversation = conversations
                                .entry(message.raw.conversation.hash())
                                .or_insert(ActiveConversation {
                                    input_streams: vec![],
                                    output_streams: vec![],
                                });

                            if let Some(stream) = conversation
                                .output_streams
                                .get(stream_index.parse::<usize>().unwrap())
                            {
                                stream.sender.send(media_man::Packet { data }).unwrap();
                                continue;
                            }

                            let (sender, receiver) =
                                std::sync::mpsc::channel::<media_man::Packet>();

                            let output_stream;
                            {
                                let opus = opus.clone();
                                let decoder_settings = opus
                                    .default_decoder_settings(SampleFormat::F32, 48000, 1)
                                    .unwrap();
                                let mut decoder = opus.create_decoder(decoder_settings).unwrap();

                                let mut buffer = VecDeque::<f32>::new();

                                let output_device = host
                                    .default_output_device()
                                    .expect("Cannot get default output device");
                                output_stream = output_device
                                    .build_output_stream(
                                        &cpal::StreamConfig {
                                            channels: 1,
                                            sample_rate: cpal::SampleRate(48000),
                                            buffer_size: cpal::BufferSize::Fixed(FRAME_SIZE as u32),
                                        },
                                        move |samples: &mut [f32], info| {
                                            while let Ok(packet) = receiver.try_recv() {
                                                info!("Recv Packet: {}", packet.data.len());
                                                let frames = decoder.decode(packet).unwrap();

                                                buffer.extend(unsafe {
                                                    std::slice::from_raw_parts(
                                                        frames[0].data.as_ptr() as *const f32,
                                                        frames[0].data.len() / size_of::<f32>(),
                                                    )
                                                });
                                            }

                                            for sample in samples {
                                                *sample = buffer.pop_front().unwrap_or(0.);
                                            }
                                        },
                                        |err| error!("{err} from the default output stream"),
                                        None,
                                    )
                                    .expect("Cannot create output stream");
                                output_stream.play().expect("Cannot play output stream");
                            }

                            sender.send(media_man::Packet { data }).unwrap();

                            conversation.output_streams.push(OutputStream {
                                sender,
                                stream: output_stream,
                            });
                        }
                        "/test" => {}
                        _ => {}
                    }
                }
            });
        }

        TheMan {
            node,
            gossip,
            protocol,
            local_pool: Arc::new(local_pool),
            message_receiver,
            host,
            conversations,
            codec,
            task: Arc::new(task),
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
    ) -> Option<ConversationHandle<iroh_blobs::store::mem::Store>> {
        self.protocol.get_conversation(hash).await
    }

    pub async fn raw_conversations(&self) -> Option<Vec<iroh_blobs::Hash>> {
        self.protocol.raw_conversations().await
    }

    pub async fn create(
        &self,
        raw: RawConversation,
    ) -> Option<ConversationHandle<iroh_blobs::store::mem::Store>> {
        self.protocol.create(raw).await
    }

    pub async fn recover(&self, ticket: Ticket) {
        self.protocol.recover(ticket).await
    }

    pub fn node_id(&self) -> NodeId {
        self.node.endpoint().node_id()
    }

    pub async fn add_conversation_default_stream(&self, conversation_id: Hash) {
        let mut conversations = self.conversations.lock().await;

        let conversation = conversations
            .entry(conversation_id)
            .or_insert(ActiveConversation {
                input_streams: vec![],
                output_streams: vec![],
            });

        let input_device = self
            .host
            .default_input_device()
            .expect("Cannot get input device");

        let opus = self.codec.clone();
        let encoder_settings = opus
            .default_encoder_settings(SampleFormat::F32, 48000, 1)
            .unwrap();
        let mut encoder = opus.create_encoder(encoder_settings).unwrap();

        info!("Opus Encoder created");

        let protocol = self.protocol.clone();
        let index = conversation.input_streams.len();

        let tasks: Arc<std::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>> = Arc::default();

        let rt = tokio::runtime::Handle::current();
        let input_stream;
        {
            let tasks = tasks.clone();

            input_stream = input_device
                .build_input_stream(
                    &cpal::StreamConfig {
                        channels: 1,
                        sample_rate: cpal::SampleRate(48000),
                        buffer_size: cpal::BufferSize::Fixed(FRAME_SIZE as u32),
                    },
                    move |samples: &[f32], info| {
                        encoder.encode(&[&FrameAudio::f32_new(samples)]).unwrap();

                        while let Some(packet) = encoder.get_packet() {
                            info!("Send Packet: {}", packet.data.len());

                            let protocol = protocol.clone();

                            tasks.lock().unwrap().push(rt.spawn(async move {
                                let ticket = protocol.store(packet.data).await;
                                protocol
                                    .reply_last(
                                        conversation_id,
                                        format!(
                                            "/auto {index} {}",
                                            base64_serialize(&ticket).unwrap()
                                        ),
                                    )
                                    .await;
                            }));
                        }
                    },
                    |err| error!("{err} from Default audio device"),
                    None,
                )
                .expect("Cannot create input stream");
        }

        input_stream
            .play()
            .expect("Cannot start default input audio stream!");

        conversation.input_streams.push(InputStream {
            stream: input_stream,
            tasks,
        });
    }
}

pub fn base64_serialize<T: Serialize>(value: &T) -> bincode::Result<String> {
    let value = bincode::serialize(value)?;
    Ok(BASE64_URL_SAFE_NO_PAD.encode(value))
}

#[derive(Debug)]
pub enum Base64DecodeError {
    Bincode(bincode::Error),
    Base64(base64::DecodeError),
}

impl From<base64::DecodeError> for Base64DecodeError {
    fn from(value: base64::DecodeError) -> Self {
        Self::Base64(value)
    }
}

impl From<bincode::Error> for Base64DecodeError {
    fn from(value: bincode::Error) -> Self {
        Self::Bincode(value)
    }
}

pub fn base64_deserialize<T: DeserializeOwned>(
    value: impl AsRef<[u8]>,
) -> Result<T, Base64DecodeError> {
    let bytes = BASE64_URL_SAFE_NO_PAD.decode(value)?;
    Ok(bincode::deserialize::<T>(&bytes)?)
}
