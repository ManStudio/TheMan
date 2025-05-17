use std::{collections::BTreeMap, future::Future, pin::Pin, str::FromStr, sync::Arc};

use base64::{Engine as _, prelude::BASE64_URL_SAFE_NO_PAD};
use iroh::{NodeId, SecretKey, endpoint::RemoteInfo, protocol::Router};
pub mod protocol;
use iroh_blobs::Hash;
type Store = iroh_blobs::store::fs::Store;
use media_man::{CodecAudioOpus, CodecAudioRaw, CodecVideoRaw, TCodecAudio, TCodecVideo};
use platform::PlatformEvent;
use protocol::{ConversationHandle, Message, RawConversation, TheMan as ProtocolTheMan, Ticket};
use serde::{Serialize, de::DeserializeOwned};
use tokio::sync::Mutex;
use tokio::sync::mpsc::{
    UnboundedReceiver as Receiver, UnboundedSender as Sender, unbounded_channel as channel,
};
use tokio::sync::oneshot::{Receiver as OReceiver, Sender as OSender, channel as ochannel};
use tracing::{error, info, warn};

mod command;
use command::{Command, CommandAuto, CommandData};

pub enum StreamIN {
    Audio {
        name: String,
        task: tokio::task::JoinHandle<()>,
        sender: platform::Sender<f32>,
    },
    Video {
        name: String,
        task: tokio::task::JoinHandle<()>,
        sender: platform::Sender<(u32, u32, Arc<[u8]>)>,
    },
}

impl StreamIN {
    pub fn name(&self) -> &String {
        match self {
            StreamIN::Audio { name, .. } => name,
            StreamIN::Video { name, .. } => name,
        }
    }
}

pub enum StreamOUT {
    Audio {
        name: String,
        task: tokio::task::JoinHandle<()>,
        receiver: platform::Receiver<f32>,
    },

    Video {
        name: String,
        task: tokio::task::JoinHandle<()>,
        receiver: platform::Receiver<(u32, u32, Arc<[u8]>)>,
        last_frame: Option<(u32, u32, Arc<[u8]>)>,
    },
}

impl StreamOUT {
    pub fn name(&self) -> &String {
        match self {
            StreamOUT::Audio { name, .. } => name,
            StreamOUT::Video { name, .. } => name,
        }
    }
}

#[derive(Default)]
pub struct ActiveConversation {
    inputs: BTreeMap<u32, usize>,
    next_idx: u32,

    outputs: BTreeMap<NodeId, BTreeMap<u32, (Sender<media_man::Packet>, usize)>>,
}

enum ServiceRequest {
    Inputs(Hash, OSender<Vec<u32>>),
    Outputs(Hash, NodeId, OSender<Vec<u32>>),

    AddAudioInput(Hash, Hash, u16, OSender<u32>),
    DirectAddAudioInput(Hash, OSender<u32>),
    DirectAddVideoInput(Hash, OSender<u32>),
    StopInput(Hash, u32),

    GetInputStream(Hash, u32, OSender<usize>),
    GetOutputStream(Hash, NodeId, u32, OSender<usize>),

    InputStreams(OSender<Vec<usize>>),
    OutputStreams(OSender<Vec<usize>>),

    StreamName(usize, OSender<Option<String>>),
    StreamLastVideoFrame(usize, OSender<Option<(u32, u32, Arc<[u8]>)>>),

    OutputStreamConnections(usize, OSender<Vec<usize>>),
    OutputStreamConnect(usize, usize),
    OutputStreamDisconnect(usize, usize),

    ScreenShare,
}

struct TheManService {
    protocol: ProtocolTheMan<iroh_blobs::store::fs::Store>,
    node_id: NodeId,
    message_receiver: tokio::sync::watch::Receiver<Option<Message>>,
    receiver: tokio::sync::mpsc::Receiver<ServiceRequest>,

    platform: Box<dyn platform::TPlatform + Send + Sync + 'static>,
    platform_receiver: platform::Receiver<PlatformEvent>,
    audio_codecs: Vec<Box<dyn TCodecAudio>>,
    video_codecs: Vec<Box<dyn TCodecVideo>>,

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
        let mut audio_codecs = Vec::<Box<dyn TCodecAudio>>::default();
        audio_codecs.push(Box::new(CodecAudioRaw));
        if let Some(opus) = CodecAudioOpus::new() {
            audio_codecs.push(Box::new(opus));
        } else {
            error!("Cannot load opus!");
        }

        let mut video_codecs = Vec::<Box<dyn TCodecVideo>>::default();
        video_codecs.push(Box::new(CodecVideoRaw));

        let (platform_receiver, platform) =
            platform::init_platform().expect("Cannot create Platform");

        Self {
            protocol,
            node_id,
            message_receiver,
            receiver,

            platform,
            platform_receiver,
            audio_codecs,
            video_codecs,

            conversations: BTreeMap::default(),

            next_id: 0,
            in_streams: BTreeMap::default(),
            out_streams: BTreeMap::default(),
        }
    }

    fn setup(&mut self) {
        self.platform.init();
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

            let mut audio_receivers = Vec::new();
            let mut video_receivers = Vec::new();

            for (id, (out_stream, inputs)) in unsafe {
                std::mem::transmute::<
                    std::collections::btree_map::IterMut<'_, usize, (StreamOUT, Vec<usize>)>,
                    std::collections::btree_map::IterMut<'static, usize, (StreamOUT, Vec<usize>)>,
                >(self.out_streams.iter_mut())
            } {
                match out_stream {
                    StreamOUT::Audio { task, receiver, .. } => {
                        tasks.push(task);
                        audio_receivers.push(Box::pin(async { (receiver.recv().await, inputs) }));
                    }
                    StreamOUT::Video { task, receiver, .. } => {
                        tasks.push(task);
                        video_receivers
                            .push(Box::pin(async { (receiver.recv().await, inputs, *id) }));
                    }
                }
            }

            for (_, in_stream) in self.in_streams.iter_mut() {
                match in_stream {
                    StreamIN::Audio { task, .. } => tasks.push(task),
                    StreamIN::Video { task, .. } => tasks.push(task),
                }
            }

            tokio::select! {
                _ = async { if tasks.is_empty() {std::future::pending().await} else {futures_util::future::select_all(tasks).await}}  => {
                    panic!("A task that should never finish has finished");
                }
                o_platform_event = self.platform_receiver.recv() => {
                    if let Some(platform_event) = o_platform_event{
                        self.handle_platform(platform_event).await;
                    }
                }

                ((Some(sample), inputs), _, _) = async {if audio_receivers.is_empty() {std::future::pending().await} else{ futures_util::future::select_all(audio_receivers).await}} => {
                    for input in inputs{
                        if let Some(stream) = self.in_streams.get(input){
                            match stream{
                                StreamIN::Audio { name, task, sender } => {
                                     sender.send(sample);
                                },
                                StreamIN::Video {..} => {
                                    warn!("Video Input connected to Audio Output");
                                }
                            }
                        }
                    }
                }
                ((o_frame, inputs, id), _, _) = async {if video_receivers.is_empty() {std::future::pending().await} else{ futures_util::future::select_all(video_receivers).await}} => {
                    if let Some(frame) = o_frame{
                        for input in inputs{
                            if let Some(stream) = self.in_streams.get(input){
                                match stream{
                                    StreamIN::Audio { ..} => {
                                        warn!("Audio Input connected to Video Output");
                                    },
                                    StreamIN::Video {name, sender, ..} => {
                                        sender.send(frame.clone());
                                    }
                                }
                            }
                        }

                        if let Some(stream) = self.out_streams.get_mut(&id){
                            match &mut stream.0{
                                StreamOUT::Video {last_frame, ..} => {
                                    *last_frame = Some(frame);
                                },
                                _ => {
                                    error!("Is not video out stream");
                                }
                            }
                        }else{
                            error!("Invalid stream index");
                        }
                    }else{
                        error!("Stream died: {id}");
                        self.out_streams.remove(&id);
                    }
                }
                event = async {
                    message_receiver.await.map(|res|res.clone())
                } => {
                    if let Ok(message) = event{
                        if let Some(message) = message{
                            if let Ok(command) = Command::from_str(&message.raw.data) {
                                self.handle_command(message, command).await;
                            }
                        }
                    }else{
                        static LAST_TIME: Mutex<Option<std::time::Instant>> = Mutex::const_new(None);
                        let mut last_time = LAST_TIME.lock().await;
                        let mut show = false;
                        if let Some(instant) = &*last_time{
                            if instant.elapsed() > std::time::Duration::from_secs(1){
                                show = true;
                            }
                        }else{
                            show = true;
                        }
                        if show{
                            error!("Cannot receive message from the the-man service.");
                            *last_time = Some(std::time::Instant::now());
                        }
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

                            let (psender, mut preceiver) = channel::<media_man::Packet>();

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
                                let (sender, receiver) = platform::channel::<f32>();

                                let idx = *idx;
                                StreamOUT::Audio {
                                    name: format!(
                                        "opus decoder and receiver for {}-{}-{idx}",
                                        base64_serialize(&message.raw.conversation.hash()).unwrap(),
                                        base64_serialize(&message.ticket.owner_id).unwrap()
                                    ),
                                    task: tokio::spawn(async move {
                                        loop {
                                            let Some(packet) = preceiver.recv().await else {
                                                continue;
                                            };

                                            match decoder.decode(packet) {
                                                Ok(frames) => {
                                                    for sample in frames[0].as_f32() {
                                                        sender.send(*sample);
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
                        static LAST_TIME: Mutex<Option<std::time::Instant>> =
                            Mutex::const_new(None);
                        let mut last_time = LAST_TIME.lock().await;
                        let mut show = false;
                        if let Some(instant) = &*last_time {
                            if instant.elapsed() > std::time::Duration::from_secs(1) {
                                show = true;
                            }
                        } else {
                            show = true;
                        }

                        let Some(conversation) =
                            self.conversations.get_mut(&message.raw.conversation.hash())
                        else {
                            if show {
                                error!("Play before start???");
                                *last_time = Some(std::time::Instant::now());
                            }
                            return;
                        };

                        let Some(output) = conversation.outputs.get_mut(&message.ticket.owner_id)
                        else {
                            if show {
                                error!("Play before start???");
                                *last_time = Some(std::time::Instant::now());
                            }
                            return;
                        };

                        let Some(output_stream) = output.get_mut(idx) else {
                            if show {
                                error!("Play before start???");
                                *last_time = Some(std::time::Instant::now());
                            }
                            return;
                        };

                        let data = self.protocol.get(ticket.clone()).await;

                        if let Err(err) = output_stream.0.send(media_man::Packet { data }) {
                            error!("{err} when sending packet {idx}");
                            if let Some((_, id)) = output.remove(idx) {
                                if let Some(stream) = self.out_streams.remove(&id) {}
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

            ServiceRequest::AddAudioInput(conversation_id, last_message, ttl, result_sender) => {
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

                let (sender, mut receiver) = platform::channel::<f32>();

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

                let stream = StreamIN::Audio {
                    name: format!(
                        "opus encoder and sender for: {}-{idx}",
                        base64_serialize(&conversation_id).unwrap()
                    ),
                    task: tokio::spawn(async move {
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

            ServiceRequest::DirectAddAudioInput(conversation_id, result_sender) => {
                let codec = self
                    .audio_codecs
                    .iter()
                    .find(|codec| codec.name() == "opus")
                    .expect("Cannot get opus codec");
                let encoder_settings = codec
                    .default_encoder_settings(media_man::SampleFormat::F32, 48000, 1)
                    .unwrap();
                let mut encoder = codec.create_encoder(encoder_settings).unwrap();

                let (sender, mut receiver) = platform::channel::<f32>();

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

                let stream = StreamIN::Audio {
                    name: format!(
                        "opus encoder and sender for: {}-{idx}",
                        base64_serialize(&conversation_id).unwrap()
                    ),
                    task: tokio::spawn(async move {
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

            ServiceRequest::DirectAddVideoInput(conversation_id, result_sender) => {
                let codec = self
                    .video_codecs
                    .iter()
                    .find(|codec| codec.name() == "video-raw")
                    .expect("Cannot get video-raw codec");
                let encoder_settings = codec
                    .default_encoder_settings(media_man::Format::RGBA)
                    .unwrap();
                let mut encoder = codec.create_encoder(encoder_settings).unwrap();

                let (sender, mut receiver) = platform::channel::<(u32, u32, Arc<[u8]>)>();

                let protocol = self.protocol.clone();

                let active = self.conversations.entry(conversation_id).or_default();
                let idx = active.next_idx;
                active.next_idx += 1;

                protocol
                    .send_stream(protocol::StreamEvent::Start {
                        conversation_id,
                        idx,
                        codec: String::from("video-raw"),
                        settings: String::default(),
                    })
                    .await;

                let stream = StreamIN::Video {
                    name: format!(
                        "video-raw and sender for: {}-{idx}",
                        base64_serialize(&conversation_id).unwrap()
                    ),
                    task: tokio::spawn(async move {
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
                            let Some(frame) = receiver.recv().await else {
                                continue;
                            };

                            if let Err(err) = encoder.encode(&media_man::FrameVideo {
                                width: frame.0,
                                height: frame.1,
                                format: media_man::Format::RGBA,
                                data: frame.2,
                            }) {
                                error!("video Encode error: {err:?}");
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
                    _ = result_sender.send(Some(stream.name().clone()));
                } else if let Some((stream, _)) = self.out_streams.get(&id) {
                    _ = result_sender.send(Some(stream.name().clone()));
                } else {
                    _ = result_sender.send(None);
                }
            }
            ServiceRequest::StreamLastVideoFrame(id, result_sender) => {
                if let Some(stream) = self.out_streams.get(&id) {
                    if let StreamOUT::Video { last_frame, .. } = &stream.0 {
                        _ = result_sender.send(last_frame.clone());
                    } else {
                        _ = result_sender.send(None);
                    }
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

            ServiceRequest::ScreenShare => {
                self.platform.start_screen_share();
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

                    let (psender, mut preceiver) = channel::<media_man::Packet>();

                    let stream = {
                        let decoder_settings = audio_codec
                            .default_decoder_settings(media_man::SampleFormat::F32, 48000, 1)
                            .unwrap();
                        let mut decoder = audio_codec.create_decoder(decoder_settings).unwrap();
                        let (sender, receiver) = platform::channel::<f32>();

                        StreamOUT::Audio {
                            name: format!(
                                "opus decoder and direct receiver for {}-{}-{idx}",
                                base64_serialize(&conversation_id).unwrap(),
                                base64_serialize(&node_id).unwrap()
                            ),
                            task: tokio::spawn(async move {
                                loop {
                                    let Some(packet) = preceiver.recv().await else {
                                        continue;
                                    };

                                    match decoder.decode(packet) {
                                        Ok(frames) => {
                                            for sample in frames[0].as_f32() {
                                                sender.send(*sample);
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

                for video_codec in self.video_codecs.iter() {
                    if video_codec.name() != *codec_name {
                        continue;
                    }

                    let (psender, mut preceiver) = channel::<media_man::Packet>();

                    let stream = {
                        let decoder_settings = video_codec
                            .default_decoder_settings(media_man::Format::RGBA)
                            .unwrap();
                        let mut decoder = video_codec.create_decoder(decoder_settings).unwrap();
                        let (sender, receiver) = platform::channel::<(u32, u32, Arc<[u8]>)>();

                        StreamOUT::Video {
                            name: format!(
                                "video-raw and direct receiver for {}-{}-{idx}",
                                base64_serialize(&conversation_id).unwrap(),
                                base64_serialize(&node_id).unwrap()
                            ),
                            task: tokio::spawn(async move {
                                loop {
                                    let Some(packet) = preceiver.recv().await else {
                                        continue;
                                    };

                                    match decoder.decode(packet) {
                                        Ok(frame) => {
                                            sender.send((frame.width, frame.height, frame.data));
                                        }
                                        Err(err) => {
                                            error!("Cannot decode: {err:?}");
                                        }
                                    }
                                }
                            }),
                            receiver,
                            last_frame: None,
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
                static LAST_TIME: Mutex<Option<std::time::Instant>> = Mutex::const_new(None);
                let mut last_time = LAST_TIME.lock().await;
                let mut show = false;
                if let Some(instant) = &*last_time {
                    if instant.elapsed() > std::time::Duration::from_secs(1) {
                        show = true;
                    }
                } else {
                    show = true;
                }
                let Some(conversation) = self.conversations.get_mut(&conversation_id) else {
                    if show {
                        error!("Play before start???");
                        *last_time = Some(std::time::Instant::now());
                    }
                    return;
                };

                let Some(output) = conversation.outputs.get_mut(&node_id) else {
                    if show {
                        error!("Play before start???");
                        *last_time = Some(std::time::Instant::now());
                    }
                    return;
                };

                let Some(output_stream) = output.get_mut(&idx) else {
                    if show {
                        error!("Play before start???");
                        *last_time = Some(std::time::Instant::now());
                    }
                    return;
                };

                if let Err(err) = output_stream.0.send(media_man::Packet {
                    data: data.to_vec(),
                }) {
                    error!("{err} when sending packet {idx}");
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

    async fn handle_platform(&mut self, event: PlatformEvent) {
        match event {
            PlatformEvent::NewAudioOutput(id, sender, name) => {
                info!("NewAudioOutput with id: {id} name: {name}");
                self.add_input_stream(StreamIN::Audio {
                    name,
                    task: tokio::spawn(std::future::pending()),
                    sender,
                });
            }
            PlatformEvent::RemovedAudioOutput(id) => todo!(),
            PlatformEvent::NewAudioInput(id, receiver, name) => {
                info!("NewAudioInput with id: {id} name: {name}");
                self.add_output_stream(StreamOUT::Audio {
                    name,
                    task: tokio::spawn(std::future::pending()),
                    receiver,
                });
            }
            PlatformEvent::RemovedAudioInput(id) => todo!(),
            PlatformEvent::NewVideoInput(id, receiver, name) => {
                info!("NewVideoInput with id: {id} name: {name}");
                self.add_output_stream(StreamOUT::Video {
                    name,
                    task: tokio::spawn(std::future::pending()),
                    receiver,
                    last_frame: None,
                });
            }
            PlatformEvent::RemovedVideoInput(id) => todo!(),
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
            .send(ServiceRequest::AddAudioInput(
                conversation_id,
                reply_to_message,
                ttl,
                sender,
            ))
            .await
            .unwrap();

        receiver.await.unwrap()
    }

    pub async fn conversation_direct_create_audio_input(&self, conversation_id: Hash) -> u32 {
        let (sender, receiver) = ochannel();

        self.sender
            .send(ServiceRequest::DirectAddAudioInput(conversation_id, sender))
            .await
            .unwrap();

        receiver.await.unwrap()
    }

    pub async fn conversation_direct_create_video_input(&self, conversation_id: Hash) -> u32 {
        let (sender, receiver) = ochannel();

        self.sender
            .send(ServiceRequest::DirectAddVideoInput(conversation_id, sender))
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

    pub async fn stream_last_video_frame(&self, id: usize) -> Option<(u32, u32, Arc<[u8]>)> {
        let (sender, receiver) = ochannel();
        self.sender
            .send(ServiceRequest::StreamLastVideoFrame(id, sender))
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

    pub async fn screen_share(&self) {
        _ = self.sender.send(ServiceRequest::ScreenShare).await;
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
