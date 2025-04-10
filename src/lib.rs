use std::{
    collections::{BTreeMap, VecDeque},
    future::Future,
    pin::Pin,
    str::FromStr,
    sync::Arc,
};

use base64::{Engine as _, prelude::BASE64_URL_SAFE_NO_PAD};
use iroh::{NodeId, SecretKey, endpoint::RemoteInfo, protocol::Router};
pub mod protocol;
use iroh_blobs::Hash;
type Store = iroh_blobs::store::fs::Store;
use media_man::{CodecAudioOpus, TCodecAudio};
use protocol::{ConversationHandle, Message, RawConversation, TheMan as ProtocolTheMan, Ticket};
use serde::{Serialize, de::DeserializeOwned};
use tokio::sync::mpsc::{
    UnboundedReceiver as Receiver, UnboundedSender as Sender, unbounded_channel as channel,
};
use tokio::sync::oneshot::{Receiver as OReceiver, Sender as OSender, channel as ochannel};
use tracing::{error, info, warn};

use cpal::traits::{DeviceTrait as _, HostTrait as _};

mod command;
use command::{Command, CommandAuto, CommandData};

pub struct StreamIN {
    name: String,
    task: Pin<Box<dyn Future<Output = ()> + Send>>,
    sender: Sender<f32>,
}

pub struct StreamOUT {
    name: String,
    task: Pin<Box<dyn Future<Output = ()> + Send>>,
    receiver: Receiver<f32>,
}

pub struct UnsafeSendSyncWrapper<T>(pub T);

unsafe impl<T> Send for UnsafeSendSyncWrapper<T> {}
unsafe impl<T> Sync for UnsafeSendSyncWrapper<T> {}

#[derive(Default)]
pub struct ActiveConversation {
    inputs: BTreeMap<u32, usize>,
    next_idx: u32,

    outputs: BTreeMap<NodeId, BTreeMap<u32, (tokio::sync::mpsc::Sender<media_man::Packet>, usize)>>,
}

enum ServiceRequest {
    Inputs(Hash, OSender<Vec<u32>>),
    Outputs(Hash, NodeId, OSender<Vec<u32>>),

    AddInput(Hash, Hash, u16, OSender<u32>),
    DirectAddInput(Hash, OSender<u32>),

    StopInput(Hash, u32),

    GetInputStream(Hash, u32, OSender<usize>),
    GetOutputStream(Hash, NodeId, u32, OSender<usize>),

    InputStreams(OSender<Vec<usize>>),
    OutputStreams(OSender<Vec<usize>>),

    StreamName(usize, OSender<Option<String>>),

    OutputStreamConnections(usize, OSender<Vec<usize>>),
    OutputStreamConnect(usize, usize),
    OutputStreamDisconnect(usize, usize),
}

struct TheManService {
    protocol: ProtocolTheMan<iroh_blobs::store::fs::Store>,
    node_id: NodeId,
    message_receiver: tokio::sync::watch::Receiver<Option<Message>>,
    receiver: tokio::sync::mpsc::Receiver<ServiceRequest>,

    host: cpal::Host,
    audio_codecs: Vec<Box<dyn TCodecAudio>>,

    conversations: BTreeMap<Hash, ActiveConversation>,

    next_id: usize,

    in_streams: BTreeMap<usize, StreamIN>,
    out_streams: BTreeMap<usize, (StreamOUT, Vec<usize>)>,
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

            next_id: 0,
            in_streams: BTreeMap::default(),
            out_streams: BTreeMap::default(),
        }
    }

    pub fn setup(&mut self) {
        self.add_default_input();
        self.add_default_output();
    }

    pub fn add_default_input(&mut self) {
        if let Some(default_device) = self.host.default_input_device() {
            let (sender, receiver) = channel::<f32>();

            let Ok(stream) = default_device.build_input_stream(
                &cpal::StreamConfig {
                    channels: 1,
                    sample_rate: cpal::SampleRate(48000),
                    buffer_size: cpal::BufferSize::Fixed(960),
                },
                move |samples: &[f32], _info| {
                    for sample in samples {
                        if let Err(err) = sender.send(*sample) {
                            error!("Default Audio Input cannot send: {err}");
                        }
                    }
                },
                |err| {
                    error!("Default Audio Input: {err}");
                },
                None,
            ) else {
                error!("Found Default Audio Device but cannot open audio stream");
                return;
            };

            let stream = UnsafeSendSyncWrapper(stream);

            self.add_output_stream(StreamOUT {
                name: format!(
                    "OS Input: {}",
                    default_device
                        .name()
                        .unwrap_or_else(|err| format!("Unknown {err}"))
                ),
                task: Box::pin(async move {
                    let _stream = stream;
                    std::future::pending().await
                }),
                receiver,
            });
        } else {
            warn!("No input device found!");
        }
    }

    pub fn add_default_output(&mut self) {
        if let Some(default_device) = self.host.default_output_device() {
            let (sender, mut receiver) = channel::<f32>();

            let mut buffer = VecDeque::new();
            let Ok(stream) = default_device.build_output_stream(
                &cpal::StreamConfig {
                    channels: 1,
                    sample_rate: cpal::SampleRate(48000),
                    buffer_size: cpal::BufferSize::Fixed(960),
                },
                move |samples: &mut [f32], _info| {
                    while let Ok(sample) = receiver.try_recv() {
                        buffer.push_back(sample);
                    }

                    for sample in samples {
                        *sample = buffer.pop_front().unwrap_or(0.0);
                    }
                },
                |err| {
                    error!("Default Audio Output: {err}");
                },
                None,
            ) else {
                error!("Found Default Audio Device but cannot open audio stream");
                return;
            };

            let stream = UnsafeSendSyncWrapper(stream);

            self.add_input_stream(StreamIN {
                name: format!(
                    "OS Output: {}",
                    default_device
                        .name()
                        .unwrap_or_else(|err| format!("Unknown {err}"))
                ),
                task: Box::pin(async move {
                    let _stream = stream;
                    std::future::pending().await
                }),
                sender,
            });
        } else {
            warn!("No output device found!");
        }
    }

    pub fn add_input_stream(&mut self, stream: StreamIN) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        _ = self.in_streams.insert(id, stream);
        id
    }

    pub fn add_output_stream(&mut self, stream: StreamOUT) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        _ = self.out_streams.insert(id, (stream, Vec::default()));
        id
    }

    pub async fn run(mut self) {
        let (stream_sender, mut stream_receiver) = channel();

        self.protocol.add_stream_sender(stream_sender).await;

        loop {
            let message_receiver = {
                let mut i = 0;
                self.message_receiver.wait_for(move |v| {
                    let res = v.is_some() && i > 0;
                    i += 1;
                    res
                })
            };

            let mut tasks = Vec::new();

            let mut receivers = Vec::new();

            for (_, (out_stream, inputs)) in unsafe {
                std::mem::transmute::<
                    std::collections::btree_map::IterMut<'_, usize, (StreamOUT, Vec<usize>)>,
                    std::collections::btree_map::IterMut<'static, usize, (StreamOUT, Vec<usize>)>,
                >(self.out_streams.iter_mut())
            } {
                tasks.push(&mut out_stream.task);
                receivers.push(Box::pin(async {
                    (out_stream.receiver.recv().await, inputs)
                }))
            }

            for (_, in_stream) in self.in_streams.iter_mut() {
                tasks.push(&mut in_stream.task);
            }

            tokio::select! {
                _ = async { if tasks.is_empty() {std::future::pending().await} else {futures_util::future::select_all(tasks).await}}  => {
                    panic!("A task that should never finish has finished");
                }
                ((Some(sample), inputs), _, _) = async {if receivers.is_empty() {std::future::pending().await} else{ futures_util::future::select_all(receivers).await}} => {
                    for input in inputs{
                        if let Some(stream) = self.in_streams.get(input){
                            if let Err(err) = stream.sender.send(sample){
                                error!("Cannot send sample to: {} {err}", stream.name);
                            }
                        }
                    }
                }
                Some(message) = async {
                    message_receiver.await
                    .expect("Cannot receive message from the the-man service.")
                    .clone()
                } => {
                    if let Ok(command) = Command::from_str(&message.raw.data) {
                        self.handle_command(message, command).await;
                    }
                }
                Some(request) = self.receiver.recv() => {
                    self.handle_request(request).await;
                }
                Some((node_id, stream_event)) = stream_receiver.recv() => {
                    self.handle_stream_event(node_id, stream_event).await;
                    // info!("Stream Event: {stream_event:?}");
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

                            let (psender, mut preceiver) =
                                tokio::sync::mpsc::channel::<media_man::Packet>(16);

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
                                let (sender, receiver) = channel::<f32>();

                                let idx = *idx;
                                StreamOUT {
                                    name: format!(
                                        "opus decoder and receiver for {}-{}-{idx}",
                                        base64_serialize(&message.raw.conversation.hash()).unwrap(),
                                        base64_serialize(&message.ticket.owner_id).unwrap()
                                    ),
                                    task: Box::pin(async move {
                                        loop {
                                            let Some(packet) = preceiver.recv().await else {
                                                continue;
                                            };

                                            match decoder.decode(packet) {
                                                Ok(mut frames) => {
                                                    for sample in frames.remove(0).to_f32() {
                                                        _ = sender.send(sample);
                                                    }
                                                }
                                                Err(err) => {
                                                    error!("{err:?} when decoding for {idx}");
                                                }
                                            }
                                        }
                                    }),
                                    receiver,
                                }
                            };

                            let id = self.add_output_stream(stream);

                            let conversation = self
                                .conversations
                                .entry(message.raw.conversation.hash())
                                .or_default();
                            let output = conversation
                                .outputs
                                .entry(message.ticket.owner_id)
                                .or_default();

                            output.insert(*idx, (psender, id));
                            break;
                        }
                    }
                    CommandAuto::Play { idx, ticket } => {
                        let Some(conversation) =
                            self.conversations.get_mut(&message.raw.conversation.hash())
                        else {
                            error!("Play before start???");
                            return;
                        };

                        let Some(output) = conversation.outputs.get_mut(&message.ticket.owner_id)
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
                            .0
                            .try_send(media_man::Packet { data })
                            .is_err()
                        {
                            if let Some((_, id)) = output.remove(idx) {
                                self.out_streams.remove(&id);
                            }
                        }
                    }
                    CommandAuto::Stop { idx } => {
                        let Some(conversation) =
                            self.conversations.get_mut(&message.raw.conversation.hash())
                        else {
                            error!("Stop before start???");
                            return;
                        };

                        let Some(output) = conversation.outputs.get_mut(&message.ticket.owner_id)
                        else {
                            error!("Stop before start???");
                            return;
                        };

                        if let Some((_, id)) = output.remove(idx) {
                            self.out_streams.remove(&id);
                        }
                    }
                }
            }
        }
    }

    async fn handle_request(&mut self, request: ServiceRequest) {
        match request {
            ServiceRequest::Inputs(conversation_id, result_sender) => {
                let Some(active) = self.conversations.get(&conversation_id) else {
                    _ = result_sender.send(vec![]);
                    return;
                };

                _ = result_sender.send(active.inputs.keys().cloned().collect::<Vec<_>>());
            }

            ServiceRequest::Outputs(conversation_id, node_id, result_sender) => {
                let Some(active) = self.conversations.get(&conversation_id) else {
                    _ = result_sender.send(vec![]);
                    return;
                };

                let Some(outputs) = active.outputs.get(&node_id) else {
                    _ = result_sender.send(vec![]);
                    return;
                };

                _ = result_sender.send(outputs.keys().cloned().collect::<Vec<_>>());
            }

            ServiceRequest::GetInputStream(conversation_id, idx, result_sender) => {
                let Some(active) = self.conversations.get(&conversation_id) else {
                    _ = result_sender.send(usize::MAX);
                    return;
                };

                let Some(input) = active.inputs.get(&idx) else {
                    _ = result_sender.send(usize::MAX);
                    return;
                };

                _ = result_sender.send(*input);
            }

            ServiceRequest::GetOutputStream(conversation_id, node_id, idx, result_sender) => {
                let Some(active) = self.conversations.get(&conversation_id) else {
                    _ = result_sender.send(usize::MAX);
                    return;
                };

                let Some(node_outputs) = active.outputs.get(&node_id) else {
                    _ = result_sender.send(usize::MAX);
                    return;
                };

                let Some(output) = node_outputs.get(&idx) else {
                    _ = result_sender.send(usize::MAX);
                    return;
                };

                _ = result_sender.send(output.1);
            }

            ServiceRequest::AddInput(conversation_id, last_message, ttl, result_sender) => {
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

                let codec = self
                    .audio_codecs
                    .iter()
                    .find(|codec| codec.name() == "opus")
                    .expect("Cannot get opus codec");
                let encoder_settings = codec
                    .default_encoder_settings(media_man::SampleFormat::F32, 48000, 1)
                    .unwrap();
                let mut encoder = codec.create_encoder(encoder_settings).unwrap();

                let (sender, mut receiver) = channel::<f32>();

                let protocol = self.protocol.clone();

                let active = self.conversations.entry(conversation_id).or_default();
                let idx = active.next_idx;
                active.next_idx += 1;

                conversation
                    .send(
                        format!(
                            "{}",
                            Command::Auto(CommandAuto::Start {
                                idx: 0,
                                codec_name: "opus".into(),
                                codec_settings: BTreeMap::default()
                            })
                        ),
                        0,
                    )
                    .await;

                let stream = StreamIN {
                    name: format!(
                        "opus encoder and sender for: {}-{idx}",
                        base64_serialize(&conversation_id).unwrap()
                    ),
                    task: Box::pin(async move {
                        struct DropConversation(ConversationHandle<Store>, u32, u16);
                        impl std::ops::Deref for DropConversation {
                            type Target = ConversationHandle<Store>;

                            fn deref(&self) -> &Self::Target {
                                &self.0
                            }
                        }

                        impl std::ops::DerefMut for DropConversation {
                            fn deref_mut(&mut self) -> &mut Self::Target {
                                &mut self.0
                            }
                        }

                        impl Drop for DropConversation {
                            fn drop(&mut self) {
                                info!("DropConversation");
                                tokio::task::block_in_place(|| {
                                    tokio::runtime::Handle::current().block_on(async {
                                        self.0
                                            .send(
                                                format!(
                                                    "{}",
                                                    Command::Auto(CommandAuto::Stop {
                                                        idx: self.1
                                                    })
                                                ),
                                                self.2,
                                            )
                                            .await;
                                    });
                                });
                            }
                        }

                        let mut conversation = DropConversation(conversation, idx, ttl);

                        loop {
                            let Some(sample) = receiver.recv().await else {
                                continue;
                            };

                            if let Err(err) =
                                encoder.encode(&[&media_man::FrameAudio::f32_new(vec![sample])])
                            {
                                error!("opus encode: {err:?}");
                                continue;
                            }

                            while let Some(packet) = encoder.get_packet() {
                                let ticket = protocol.store(packet.data, ttl).await;

                                conversation
                                    .send(
                                        format!(
                                            "{}",
                                            Command::Auto(CommandAuto::Play { idx, ticket })
                                        ),
                                        ttl,
                                    )
                                    .await;
                            }
                        }
                    }),
                    sender,
                };

                let id = self.add_input_stream(stream);
                let active = self.conversations.entry(conversation_id).or_default();
                active.inputs.insert(idx, id);
                _ = result_sender.send(idx);
            }

            ServiceRequest::DirectAddInput(conversation_id, result_sender) => {
                let codec = self
                    .audio_codecs
                    .iter()
                    .find(|codec| codec.name() == "opus")
                    .expect("Cannot get opus codec");
                let encoder_settings = codec
                    .default_encoder_settings(media_man::SampleFormat::F32, 48000, 1)
                    .unwrap();
                let mut encoder = codec.create_encoder(encoder_settings).unwrap();

                let (sender, mut receiver) = channel::<f32>();

                let protocol = self.protocol.clone();

                let active = self.conversations.entry(conversation_id).or_default();
                let idx = active.next_idx;
                active.next_idx += 1;

                protocol
                    .send_stream(protocol::StreamEvent::Start {
                        conversation_id,
                        idx,
                        codec: String::from("opus"),
                        settings: String::default(),
                    })
                    .await;

                let stream = StreamIN {
                    name: format!(
                        "opus encoder and sender for: {}-{idx}",
                        base64_serialize(&conversation_id).unwrap()
                    ),
                    task: Box::pin(async move {
                        struct DropConversation(ProtocolTheMan<Store>, Hash, u32);

                        impl Drop for DropConversation {
                            fn drop(&mut self) {
                                tokio::task::block_in_place(|| {
                                    tokio::runtime::Handle::current().block_on(async {
                                        self.0
                                            .send_stream(protocol::StreamEvent::Stop {
                                                conversation_id: self.1,
                                                idx: self.2,
                                            })
                                            .await;
                                    });
                                });
                            }
                        }

                        let conversation = DropConversation(protocol.clone(), conversation_id, idx);

                        loop {
                            let Some(sample) = receiver.recv().await else {
                                continue;
                            };

                            if let Err(err) =
                                encoder.encode(&[&media_man::FrameAudio::f32_new(vec![sample])])
                            {
                                error!("opus encode: {err:?}");
                                continue;
                            }

                            while let Some(packet) = encoder.get_packet() {
                                protocol
                                    .send_stream(protocol::StreamEvent::Play {
                                        conversation_id: conversation.1,
                                        idx: conversation.2,
                                        data: Arc::from(packet.data),
                                    })
                                    .await;
                            }
                        }
                    }),
                    sender,
                };

                let id = self.add_input_stream(stream);
                let active = self.conversations.entry(conversation_id).or_default();
                active.inputs.insert(idx, id);
                if result_sender.send(idx).is_err() {
                    warn!("Cannot respond");
                }
            }

            ServiceRequest::StopInput(conversation_id, idx) => {
                let active = self.conversations.entry(conversation_id).or_default();
                let Some(stream) = active.inputs.remove(&idx) else {
                    error!("There is no input stream to remove");
                    return;
                };

                _ = self.in_streams.remove(&stream);
            }

            ServiceRequest::InputStreams(result_sender) => {
                _ = result_sender.send(self.in_streams.keys().cloned().collect::<Vec<_>>());
            }

            ServiceRequest::OutputStreams(result_sender) => {
                _ = result_sender.send(self.out_streams.keys().cloned().collect::<Vec<_>>());
            }

            ServiceRequest::StreamName(id, result_sender) => {
                if let Some(stream) = self.in_streams.get(&id) {
                    _ = result_sender.send(Some(stream.name.clone()));
                } else if let Some((stream, _)) = self.out_streams.get(&id) {
                    _ = result_sender.send(Some(stream.name.clone()));
                } else {
                    _ = result_sender.send(None);
                }
            }

            ServiceRequest::OutputStreamConnections(id, result_sender) => {
                if let Some((_, connections)) = self.out_streams.get(&id) {
                    _ = result_sender.send(connections.clone());
                } else {
                    _ = result_sender.send(vec![]);
                }
            }

            ServiceRequest::OutputStreamConnect(id, to_id) => {
                let Some((_, connections)) = self.out_streams.get_mut(&id) else {
                    return;
                };
                if !connections.contains(&to_id) {
                    connections.push(to_id);
                }
            }

            ServiceRequest::OutputStreamDisconnect(id, from_id) => {
                let Some((_, connections)) = self.out_streams.get_mut(&id) else {
                    return;
                };

                if let Some(to_remove) = connections.iter().position(|id| *id == from_id) {
                    connections.swap_remove(to_remove);
                }
            }
        }
    }

    async fn handle_stream_event(&mut self, node_id: NodeId, stream_event: protocol::StreamEvent) {
        match stream_event {
            protocol::StreamEvent::Start {
                conversation_id,
                idx,
                codec: codec_name,
                settings,
            } => {
                for audio_codec in self.audio_codecs.iter() {
                    if audio_codec.name() != *codec_name {
                        continue;
                    }

                    let (psender, mut preceiver) =
                        tokio::sync::mpsc::channel::<media_man::Packet>(16);

                    let stream = {
                        let decoder_settings = audio_codec
                            .default_decoder_settings(media_man::SampleFormat::F32, 48000, 1)
                            .unwrap();
                        let mut decoder = audio_codec.create_decoder(decoder_settings).unwrap();
                        let (sender, receiver) = channel::<f32>();

                        StreamOUT {
                            name: format!(
                                "opus decoder and direct receiver for {}-{}-{idx}",
                                base64_serialize(&conversation_id).unwrap(),
                                base64_serialize(&node_id).unwrap()
                            ),
                            task: Box::pin(async move {
                                loop {
                                    let Some(packet) = preceiver.recv().await else {
                                        continue;
                                    };

                                    match decoder.decode(packet) {
                                        Ok(mut frames) => {
                                            for sample in frames.remove(0).to_f32() {
                                                _ = sender.send(sample);
                                            }
                                        }
                                        Err(err) => {
                                            error!("{err:?} when decoding for {idx}");
                                        }
                                    }
                                }
                            }),
                            receiver,
                        }
                    };

                    let id = self.add_output_stream(stream);

                    let conversation = self.conversations.entry(conversation_id).or_default();
                    let output = conversation.outputs.entry(node_id).or_default();

                    output.insert(idx, (psender, id));
                    break;
                }
            }
            protocol::StreamEvent::Play {
                conversation_id,
                idx,
                data,
            } => {
                let Some(conversation) = self.conversations.get_mut(&conversation_id) else {
                    error!("Play before start???");
                    return;
                };

                let Some(output) = conversation.outputs.get_mut(&node_id) else {
                    error!("Play before start???");
                    return;
                };

                let Some(output_stream) = output.get_mut(&idx) else {
                    error!("Play before start???");
                    return;
                };

                if output_stream
                    .0
                    .try_send(media_man::Packet {
                        data: data.to_vec(),
                    })
                    .is_err()
                {
                    if let Some((_, id)) = output.remove(&idx) {
                        self.out_streams.remove(&id);
                    }
                }
            }
            protocol::StreamEvent::Stop {
                conversation_id,
                idx,
            } => {
                let Some(conversation) = self.conversations.get_mut(&conversation_id) else {
                    error!("Stop before start???");
                    return;
                };

                let Some(output) = conversation.outputs.get_mut(&node_id) else {
                    error!("Stop before start???");
                    return;
                };

                if let Some((_, id)) = output.remove(&idx) {
                    self.out_streams.remove(&id);
                }
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
                let mut service = TheManService::new(protocol, node_id, message_receiver, receiver);
                service.setup();
                service.run().await;
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
        self.protocol.send_message(raw, 0).await
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

    pub async fn conversation_inputs(&self, conversation_id: Hash) -> Vec<u32> {
        let (sender, receiver) = ochannel();
        self.sender
            .send(ServiceRequest::Inputs(conversation_id, sender))
            .await
            .unwrap();
        receiver.await.unwrap()
    }

    pub async fn conversation_outputs(&self, conversation_id: Hash, node_id: NodeId) -> Vec<u32> {
        let (sender, receiver) = ochannel();
        self.sender
            .send(ServiceRequest::Outputs(conversation_id, node_id, sender))
            .await
            .unwrap();
        receiver.await.unwrap()
    }

    pub async fn conversation_create_input(
        &self,
        conversation_id: Hash,
        reply_to_message: Hash,
        ttl: u16,
    ) -> u32 {
        let (sender, receiver) = ochannel();

        self.sender
            .send(ServiceRequest::AddInput(
                conversation_id,
                reply_to_message,
                ttl,
                sender,
            ))
            .await
            .unwrap();

        receiver.await.unwrap()
    }

    pub async fn conversation_direct_create_input(&self, conversation_id: Hash) -> u32 {
        let (sender, receiver) = ochannel();

        self.sender
            .send(ServiceRequest::DirectAddInput(conversation_id, sender))
            .await
            .unwrap();

        receiver.await.unwrap()
    }

    pub async fn conversation_stop_input(&self, conversation_id: Hash, idx: u32) {
        self.sender
            .send(ServiceRequest::StopInput(conversation_id, idx))
            .await
            .unwrap();
    }

    pub async fn conversation_input_stream(&self, conversation_id: Hash, idx: u32) -> usize {
        let (sender, receiver) = ochannel();
        self.sender
            .send(ServiceRequest::GetInputStream(conversation_id, idx, sender))
            .await
            .unwrap();
        receiver.await.unwrap()
    }

    pub async fn conversation_output_stream(
        &self,
        conversation_id: Hash,
        node_id: NodeId,
        idx: u32,
    ) -> usize {
        let (sender, receiver) = ochannel();
        self.sender
            .send(ServiceRequest::GetOutputStream(
                conversation_id,
                node_id,
                idx,
                sender,
            ))
            .await
            .unwrap();
        receiver.await.unwrap()
    }

    pub async fn input_streams(&self) -> Vec<usize> {
        let (sender, receiver) = ochannel();
        self.sender
            .send(ServiceRequest::InputStreams(sender))
            .await
            .unwrap();
        receiver.await.unwrap()
    }

    pub async fn output_streams(&self) -> Vec<usize> {
        let (sender, receiver) = ochannel();
        self.sender
            .send(ServiceRequest::OutputStreams(sender))
            .await
            .unwrap();
        receiver.await.unwrap()
    }

    pub async fn stream_name(&self, id: usize) -> Option<String> {
        let (sender, receiver) = ochannel();
        self.sender
            .send(ServiceRequest::StreamName(id, sender))
            .await
            .unwrap();
        receiver.await.unwrap()
    }

    pub async fn output_stream_connections(&self, id: usize) -> Vec<usize> {
        let (sender, receiver) = ochannel();
        self.sender
            .send(ServiceRequest::OutputStreamConnections(id, sender))
            .await
            .unwrap();
        receiver.await.unwrap()
    }

    pub async fn output_stream_connect(&self, id: usize, to_id: usize) {
        self.sender
            .send(ServiceRequest::OutputStreamConnect(id, to_id))
            .await
            .unwrap();
    }

    pub async fn output_stream_disconnect(&self, id: usize, from_id: usize) {
        self.sender
            .send(ServiceRequest::OutputStreamDisconnect(id, from_id))
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

    pub async fn message_set_ttl(&self, hash: Hash, ttl: u16) {
        self.protocol.message_set_ttl(hash, ttl).await;
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
