use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::Duration,
};

use crate::{logic::message::AudioMessage, Message};
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    InputCallbackInfo, OutputCallbackInfo, SizedSample, StreamError,
};
use the_man::Atom;
use tokio::sync::mpsc::{Receiver, Sender};

use self::codec::{opus::CodecOpus, Codec};

mod codec;

pub struct Device {
    pub device: cpal::Device,
    pub supported_config: cpal::SupportedStreamConfig,
    pub config: cpal::StreamConfig,
}

impl Device {
    pub fn build_input_stream<T, D, E>(
        &mut self,
        data_callback: D,
        error_callback: E,
        timeout: Option<Duration>,
    ) -> Result<cpal::Stream, cpal::BuildStreamError>
    where
        T: SizedSample,
        D: FnMut(&[T], &InputCallbackInfo) + Send + 'static,
        E: FnMut(StreamError) + Send + 'static,
    {
        self.device
            .build_input_stream(&self.config, data_callback, error_callback, timeout)
    }

    pub fn build_output_stream<T, D, E>(
        &mut self,
        data_callback: D,
        error_callback: E,
        timeout: Option<Duration>,
    ) -> Result<cpal::Stream, cpal::BuildStreamError>
    where
        T: SizedSample,
        D: FnMut(&mut [T], &OutputCallbackInfo) + Send + 'static,
        E: FnMut(StreamError) + Send + 'static,
    {
        self.device
            .build_output_stream(&self.config, data_callback, error_callback, timeout)
    }
}

pub struct Stream {
    pub codec: Box<dyn Codec>,
    pub stream: Option<cpal::Stream>,
    pub volume: f32,
    pub stream_type: StreamType,
    pub id: usize,
    pub buffer: Vec<u8>,
    pub sender: Sender<Message>,
}

unsafe impl Send for Stream {}
unsafe impl Sync for Stream {}

pub enum StreamType {
    Input,
    Output,
}

pub struct Audio {
    pub logic_sender: Sender<Message>,
    pub logic_receiver: Receiver<Message>,

    pub host: Option<cpal::Host>,

    pub output_device: Option<Device>,
    pub input_device: Option<Device>,

    pub codecs: HashMap<String, Box<dyn Codec>>,
    pub streams: Vec<Arc<RwLock<Stream>>>,
}

impl Audio {
    pub fn new(logic_sender: Sender<Message>, logic_receiver: Receiver<Message>) -> Self {
        Self {
            logic_sender,
            logic_receiver,
            host: None,
            output_device: None,
            input_device: None,
            codecs: HashMap::new(),
            streams: Vec::new(),
        }
    }
    pub async fn run(mut self) {
        self.host = Some(cpal::default_host());

        self.try_get_default_devices();

        self.codecs
            .insert("opus".into(), Box::<CodecOpus>::default());

        println!("Audio thread started!");

        let mut read_errors = tokio::time::Instant::now() + std::time::Duration::from_secs(1);

        loop {
            tokio::select! {
                Some(event) = self.logic_receiver.recv() => {
                    if let Message::ShutDown = event{
                        self.shutdown();
                        break
                    }else{
                        self.process_logic(event).await;
                    }
                }
                _ = tokio::time::sleep_until(read_errors) => {
                    for stream in self.streams.iter(){
                        let errors = stream.write().unwrap().codec.errors();
                        let id = stream.read().unwrap().id;
                        for error in errors{
                            println!("Error for: {id}, {error}");
                        }
                    }

                    read_errors = tokio::time::Instant::now() + std::time::Duration::from_secs(1);
                }

            }
        }
    }

    fn shutdown(&mut self) {
        println!("Audio thread cloasing");

        for stream in self.streams.drain(..) {
            println!("Audio closing: {}", stream.read().unwrap().id);
            let _ = stream.write().unwrap().stream.take();
        }

        println!("Audio thread shutdown succesfuly");
    }

    async fn process_logic(&mut self, event: Message) {
        match event {
            Message::Audio(AudioMessage::CreateInputChannel { id, codec }) => {
                let mut error = String::new();
                if let Some(input_device) = &mut self.input_device {
                    if let Some(codec) = self.codecs.get(&codec) {
                        let mut codec = codec.c();
                        {
                            let mut channels = codec
                                .get_setting("channels".into())
                                .expect("Doze not have channels");
                            if let Atom::UnSigned { value, .. } = &mut channels {
                                *value = input_device.config.channels as usize;
                            }
                            codec.set_setting("channels".into(), channels);

                            let mut sample_rate = codec
                                .get_setting("sample_rate".into())
                                .expect("Doze not have sample_rate");
                            if let Atom::UnSigned { value, .. } = &mut sample_rate {
                                *value = input_device.config.sample_rate.0 as usize;
                            }
                            codec.set_setting("sample_rate".into(), sample_rate);
                        }
                        let stream = Arc::new(RwLock::new(Stream {
                            codec,
                            stream: None,
                            volume: 1.0,
                            stream_type: StreamType::Input,
                            id,
                            buffer: Vec::new(),
                            sender: self.logic_sender.clone(),
                        }));
                        let str = stream.clone();
                        let cpal_stream = input_device
                            .build_input_stream(
                                move |input: &[f32], _| {
                                    let volume = str.read().unwrap().volume;
                                    let data = str.write().unwrap().codec.encode(
                                        input.iter().map(|d| d * volume).collect::<Vec<f32>>(),
                                    );
                                    let _ = str.read().unwrap().sender.try_send(Message::Audio(
                                        AudioMessage::InputData { id, data },
                                    ));
                                },
                                |_| panic!("Input stream error!"),
                                None,
                            )
                            .unwrap();
                        cpal_stream.play();
                        stream.write().unwrap().stream = Some(cpal_stream);
                        self.streams.push(stream);
                    } else {
                        error.push_str("Invalid codec!\n");
                    }
                } else {
                    error.push_str("No input device!\n");
                }
                let _ = self.logic_sender.try_send(Message::Audio(
                    AudioMessage::ResCreateInputChannel(id, error),
                ));
            }
            Message::Audio(AudioMessage::CreateOutputChannel { id, codec }) => {
                let mut error = String::new();
                if let Some(output_device) = &mut self.output_device {
                    if let Some(codec) = self.codecs.get(&codec) {
                        let mut codec = codec.c();
                        {
                            let mut channels = codec
                                .get_setting("channels".into())
                                .expect("Doze not have channels");
                            if let Atom::UnSigned { value, .. } = &mut channels {
                                *value = output_device.config.channels as usize;
                            }
                            codec.set_setting("channels".into(), channels);

                            let mut sample_rate = codec
                                .get_setting("sample_rate".into())
                                .expect("Doze not have sample_rate");
                            if let Atom::UnSigned { value, .. } = &mut sample_rate {
                                *value = output_device.config.sample_rate.0 as usize;
                            }
                            codec.set_setting("sample_rate".into(), sample_rate);
                        }
                        let stream = Arc::new(RwLock::new(Stream {
                            codec,
                            stream: None,
                            volume: 1.0,
                            stream_type: StreamType::Output,
                            id,
                            buffer: Vec::new(),
                            sender: self.logic_sender.clone(),
                        }));
                        let str = stream.clone();
                        let cpal_stream = output_device
                            .build_output_stream(
                                move |output: &mut [f32], _| {
                                    let volume = str.read().unwrap().volume;
                                    let mut codec = str.read().unwrap().codec.c();
                                    let mut buffer = {
                                        let mut stre = str.write().unwrap();
                                        let mut iter = stre.buffer.drain(..);
                                        codec.decode(&mut iter)
                                    };
                                    buffer.resize(output.len(), 0.0);
                                    output.copy_from_slice(
                                        &buffer.drain(..).map(|e| e * volume).collect::<Vec<f32>>(),
                                    );
                                    str.write().unwrap().codec = codec;
                                },
                                |_| panic!("Output stream error!"),
                                None,
                            )
                            .unwrap();

                        cpal_stream.play();
                        stream.write().unwrap().stream = Some(cpal_stream);
                        self.streams.push(stream);
                    } else {
                        error.push_str("Invalid codec!\n");
                    }
                } else {
                    error.push_str("No output device!\n");
                }
                let _ = self.logic_sender.try_send(Message::Audio(
                    AudioMessage::ResCreateOutputChannel(id, error),
                ));
            }
            Message::Audio(AudioMessage::OutputData { id, mut data }) => {
                if let Some(stream) = self.streams.get(id) {
                    stream.write().unwrap().buffer.append(&mut data);
                } else {
                    eprintln!("Invalid stream: {id}")
                }
            }
            _ => {}
        }
    }

    pub fn try_get_default_devices(&mut self) {
        if let Some(host) = &mut self.host {
            self.output_device = host
                .default_output_device()
                .and_then(|device| {
                    device
                        .default_output_config()
                        .map(|config| (device, config))
                        .ok()
                })
                .map(|(device, config)| Device {
                    device,
                    config: cpal::StreamConfig {
                        channels: config.channels(),
                        sample_rate: cpal::SampleRate(48000),
                        buffer_size: config.config().buffer_size,
                    },
                    supported_config: config,
                });
            self.input_device = host
                .default_input_device()
                .and_then(|device| {
                    device
                        .default_input_config()
                        .map(|config| (device, config))
                        .ok()
                })
                .map(|(device, config)| Device {
                    device,
                    config: cpal::StreamConfig {
                        channels: config.channels(),
                        sample_rate: cpal::SampleRate(48000),
                        buffer_size: config.config().buffer_size,
                    },
                    supported_config: config,
                });
        }
    }
}
