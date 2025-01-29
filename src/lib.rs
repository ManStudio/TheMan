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
use protocol::{ConversationHandle, Message, RawConversation, TheMan as ProtocolTheMan, Ticket};
use serde::{de::DeserializeOwned, Serialize};
use tokio::sync::{Mutex, RwLock};
use tracing::{error, info};

pub struct InputStream {
    opus: Arc<opus_sys_kman::OpusLibSys>,
    encoder: opus_sys_kman::OpusEncoder,
    stream: Stream,
    tasks: Arc<std::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>>,
}

unsafe impl Send for InputStream {}
unsafe impl Sync for InputStream {}

impl Drop for InputStream {
    fn drop(&mut self) {
        unsafe {
            self.opus.opus_encoder_destroy(&mut self.encoder);
        }
    }
}

pub struct OutputStream {
    opus: Arc<opus_sys_kman::OpusLibSys>,
    sender: std::sync::mpsc::Sender<Vec<u8>>,
    stream: Stream,
    decoder: opus_sys_kman::OpusDecoder,
}

const FRAME_SIZE: usize = 960;

unsafe impl Send for OutputStream {}
unsafe impl Sync for OutputStream {}

impl Drop for OutputStream {
    fn drop(&mut self) {
        unsafe {
            self.opus.opus_decoder_destroy(&mut self.decoder);
        }
    }
}

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
    codec: Arc<opus_sys_kman::OpusLibSys>,

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

        let protocol = protocol::TheMan::spawn(
            gossip.clone(),
            blobs.clone(),
            endpoint.clone(),
            local_pool.handle(),
        )
        .await;

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

        let library = unsafe {
            opus_sys_kman::OpusLibSys::new()
                .expect("Cannot find opus codec or incompatible opus library")
        };

        let conversations: Arc<Mutex<HashMap<iroh_blobs::Hash, ActiveConversation>>> =
            Arc::default();
        let codec = Arc::new(library);
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
                                stream.sender.send(data).unwrap();
                                continue;
                            }

                            let (sender, receiver) = std::sync::mpsc::channel::<Vec<u8>>();

                            let mut decoder;
                            let output_stream;
                            {
                                let opus = opus.clone();
                                unsafe {
                                    let mut ret = 0i32;
                                    decoder = opus.opus_decoder_create(48000, 1, &mut ret);

                                    if ret != 0 {
                                        error!("Cannot create opus decoder");
                                        std::process::exit(1);
                                    }
                                }

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
                                            if let Ok(packet) = receiver.try_recv() {
                                                info!("Recv Packet: {}", packet.len());
                                                let mut tmp = [0f32; FRAME_SIZE];
                                                unsafe {
                                                    let res = opus.opus_decode_float(
                                                        &mut decoder,
                                                        packet.as_ptr(),
                                                        packet.len() as i32,
                                                        tmp.as_mut_ptr(),
                                                        FRAME_SIZE as i32,
                                                        0,
                                                    );

                                                    if res < 0 {
                                                        error!("Cannot decode");
                                                    } else {
                                                        buffer.extend(&tmp[0..res as usize]);
                                                    }
                                                }
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

                            sender.send(data).unwrap();

                            conversation.output_streams.push(OutputStream {
                                opus,
                                sender,
                                stream: output_stream,
                                decoder,
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
        let mut encoder;
        unsafe {
            let mut err = 0i32;
            encoder =
                opus.opus_encoder_create(48000, 1, opus_sys_kman::OPUS_APPLICATION_AUDIO, &mut err);
            if err != 0 {
                error!("Cannot create opus encoder {err}");
                std::process::exit(1);
            }
        }

        info!("Opus Encoder created");

        let protocol = self.protocol.clone();
        let index = conversation.input_streams.len();
        let mut buffer = VecDeque::<f32>::default();
        let mut data = [0u8; 1024];

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
                        buffer.extend(samples);
                        if buffer.len() < FRAME_SIZE {
                            return;
                        }

                        let mut tmp_buffer = Vec::<f32>::with_capacity(FRAME_SIZE);
                        for _ in 0..FRAME_SIZE {
                            tmp_buffer.push(buffer.pop_front().unwrap());
                        }
                        let res = unsafe {
                            opus.opus_encode_float(
                                &mut encoder,
                                tmp_buffer.as_ptr(),
                                FRAME_SIZE as i32,
                                data.as_mut_ptr(),
                                data.len() as i32,
                            )
                        };
                        if res < 0 {
                            error!("Cannot opus encode {res}");
                            return;
                        }

                        let data = data[0..res as usize].to_vec();
                        info!("Send Packet: {}", data.len());

                        let protocol = protocol.clone();

                        tasks.lock().unwrap().push(rt.spawn(async move {
                            let ticket = protocol.store(data).await;
                            protocol
                                .reply_last(
                                    conversation_id,
                                    format!("/auto {index} {}", base64_serialize(&ticket).unwrap()),
                                )
                                .await;
                        }));
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
            opus: self.codec.clone(),
            encoder,
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
