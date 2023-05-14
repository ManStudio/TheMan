use std::{collections::HashMap, time::Duration};

use crate::{logic::message::AudioMessage, Message};
use cpal::{
    traits::{DeviceTrait, HostTrait},
    InputCallbackInfo, OutputCallbackInfo, SizedSample, StreamError,
};
use tokio::sync::mpsc::{Receiver, Sender};

use self::codec::Codec;

mod codec;

pub struct Device {
    pub device: cpal::Device,
    pub config: cpal::SupportedStreamConfig,
}

impl Device {
    pub fn build_input_stream<T, D, E>(
        &mut self,
        mut data_callback: D,
        error_callback: E,
        timeout: Option<Duration>,
    ) -> Result<cpal::Stream, cpal::BuildStreamError>
    where
        T: SizedSample,
        D: FnMut(&[T], &InputCallbackInfo) + Send + 'static,
        E: FnMut(StreamError) + Send + 'static,
    {
        self.device.build_input_stream(
            &self.config.clone().into(),
            data_callback,
            error_callback,
            timeout,
        )
    }

    pub fn build_output_stream<T, D, E>(
        &mut self,
        mut data_callback: D,
        error_callback: E,
        timeout: Option<Duration>,
    ) -> Result<cpal::Stream, cpal::BuildStreamError>
    where
        T: SizedSample,
        D: FnMut(&mut [T], &OutputCallbackInfo) + Send + 'static,
        E: FnMut(StreamError) + Send + 'static,
    {
        self.device.build_output_stream(
            &self.config.clone().into(),
            data_callback,
            error_callback,
            timeout,
        )
    }
}

pub struct Stream {
    pub codec: Box<dyn Codec>,
    pub input: Option<cpal::Stream>,
    pub volume: f32,
    pub stream_type: StreamType,
    pub id: usize,
}

pub enum StreamType {
    Input,
    Output,
}

pub struct Audio {
    pub logic_sender: Sender<Message>,
    pub logic_reciver: Receiver<Message>,
    pub gui_sender: Sender<Message>,
    pub gui_reciver: Receiver<Message>,
    pub egui_ctx: eframe::egui::Context,

    pub host: Option<cpal::Host>,

    pub output_device: Option<Device>,
    pub input_device: Option<Device>,

    pub codecs: HashMap<String, Box<dyn Codec>>,
    pub streams: Vec<Stream>,
}

impl Audio {
    pub async fn run(mut self) {
        self.host = Some(cpal::default_host());
        self.try_get_default_devices();

        loop {
            tokio::select! {
                Some(event) = self.logic_reciver.recv() => {
                    if let Message::ShutDown = event{
                        self.shutdown();
                        break
                    }else{
                        self.process_logic(event);
                    }
                }
                Some(event) = self.gui_reciver.recv() => {
                    self.process_gui(event);
                }

            }
        }
    }

    fn shutdown(&mut self) {}

    fn process_logic(&mut self, event: Message) {
        match event {
            Message::Audio(AudioMessage::CreateInputChannel { id, codec }) => {
                let mut error = String::new();
                if let Some(input_device) = &mut self.input_device {
                } else {
                    error.push_str("No input device!\n");
                }
                self.logic_sender
                    .try_send(Message::Audio(AudioMessage::ResCreateInputChannel(
                        id, error,
                    )));
            }
            _ => {}
        }
    }

    fn process_gui(&mut self, event: Message) {
        match event {
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
                .map(|(device, config)| Device { device, config });
            self.input_device = host
                .default_input_device()
                .and_then(|device| {
                    device
                        .default_input_config()
                        .map(|config| (device, config))
                        .ok()
                })
                .map(|(device, config)| Device { device, config });
        }
    }
}
