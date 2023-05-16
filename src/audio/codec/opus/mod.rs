use std::collections::HashMap;

use bytes_kman::TBytes;
use opus::{Decoder, Encoder};
use the_man::Atom;

use super::Codec;

pub struct CodecOpus {
    errors: Vec<String>,
    pub decoder: Decoder,
    pub encoder: Encoder,
    pub channels: opus::Channels,
    pub application: opus::Application,
    pub sample_rate: u32,
    pub input_buffer: Vec<f32>,
    pub output_buffer: Vec<u8>,
}

impl Clone for CodecOpus {
    fn clone(&self) -> Self {
        Self {
            errors: self.errors.clone(),
            decoder: Decoder::new(self.sample_rate, self.channels).unwrap(),
            encoder: Encoder::new(self.sample_rate, self.channels, self.application).unwrap(),
            channels: self.channels,
            application: self.application,
            sample_rate: self.sample_rate,
            input_buffer: self.input_buffer.clone(),
            output_buffer: self.output_buffer.clone(),
        }
    }
}

unsafe impl Send for CodecOpus {}
unsafe impl Sync for CodecOpus {}

impl Default for CodecOpus {
    fn default() -> Self {
        println!("Opus version: {}", opus::version());
        let mut s = Self {
            errors: Vec::new(),
            decoder: Decoder::new(8000, opus::Channels::Stereo).unwrap(),
            encoder: Encoder::new(8000, opus::Channels::Stereo, opus::Application::Voip).unwrap(),
            channels: opus::Channels::Stereo,
            application: opus::Application::Voip,
            sample_rate: 8000,
            input_buffer: Vec::new(),
            output_buffer: Vec::new(),
        };

        s.setup_buffers();

        s
    }
}

impl CodecOpus {
    fn setup_buffers(&mut self) {
        let channels = match self.channels {
            opus::Channels::Mono => 1usize,
            opus::Channels::Stereo => 2usize,
        };

        let len = self.sample_rate as usize * channels;
        self.input_buffer.resize(len, 0.0);
        self.output_buffer.resize(len, 0);
    }
}

impl Codec for CodecOpus {
    fn name(&self) -> &str {
        "Opus"
    }

    fn settings(&self) -> Vec<String> {
        vec![]
    }

    fn get_setting(&mut self, key: String) -> Option<the_man::Atom> {
        match key.trim() {
            "sample_rate" => Some(the_man::Atom::UnSigned {
                value: self.sample_rate as usize,
                range: u32::MIN as usize..u32::MAX as usize,
            }),
            "channels" => Some(the_man::Atom::UnSigned {
                value: match self.channels {
                    opus::Channels::Mono => 1,
                    opus::Channels::Stereo => 2,
                },
                range: 1..3,
            }),
            _ => None,
        }
    }

    fn set_setting(&mut self, key: String, value: the_man::Atom) {
        if !value.valid() {
            eprintln!("OpusCodec set_setting invalid atom: {key}, {value:?}");
            return;
        }
        match key.trim() {
            "sample_rate" => {
                if let the_man::Atom::UnSigned { value, .. } = value {
                    let new_decoder = Decoder::new(value as u32, self.channels.clone());
                    let new_encoder = Encoder::new(
                        value as u32,
                        self.channels.clone(),
                        self.application.clone(),
                    );

                    match new_decoder {
                        Ok(decoder) => self.decoder = decoder,
                        Err(err) => self.errors.push(format!("OpusDecoder: Error: {err}")),
                    }
                    match new_encoder {
                        Ok(encoder) => self.encoder = encoder,
                        Err(err) => self.errors.push(format!("OpusEncoder: Error: {err}")),
                    }

                    self.sample_rate = value as u32;
                    self.setup_buffers();
                }
            }
            "channels" => {
                if let the_man::Atom::UnSigned { value, .. } = value {
                    let channels = match value {
                        1 => opus::Channels::Mono,
                        2 => opus::Channels::Stereo,
                        _ => {
                            panic!("OpusCodec Invalid channels: {value}")
                        }
                    };
                    let new_decoder = Decoder::new(self.sample_rate, channels);
                    let new_encoder =
                        Encoder::new(self.sample_rate, channels.clone(), self.application);

                    match new_decoder {
                        Ok(decoder) => self.decoder = decoder,
                        Err(err) => self.errors.push(format!("OpusDecoder: Error: {err}")),
                    }
                    match new_encoder {
                        Ok(encoder) => self.encoder = encoder,
                        Err(err) => self.errors.push(format!("OpusEncoder: Error: {err}")),
                    }

                    self.channels = channels;
                    self.setup_buffers();
                }
            }
            _ => {
                eprintln!("Opus invalid setting key: {key}")
            }
        }
    }

    fn errors(&mut self) -> Vec<String> {
        self.errors.drain(..).collect::<Vec<String>>()
    }

    fn encode(&mut self, data: Vec<f32>) -> Vec<u8> {
        match self.encoder.encode_float(&data, &mut self.output_buffer) {
            Ok(len) => return self.output_buffer[0..len].to_vec().to_bytes(),
            Err(err) => {
                self.errors
                    .push(format!("OpusEncoder encoding error: {err:?}"));
                return Vec::new();
            }
        }
    }

    fn decode(&mut self, data: &mut dyn Iterator<Item = u8>) -> Vec<f32> {
        let data = Vec::<u8>::from_bytes(data).unwrap_or_default();
        match self
            .decoder
            .decode_float(&data, &mut self.input_buffer, true)
        {
            Ok(len) => return self.input_buffer[0..len].to_vec(),
            Err(err) => {
                self.errors
                    .push(format!("OpusDecoder decoding error: {err:?}"));
                return Vec::new();
            }
        }
    }

    fn c(&self) -> Box<dyn Codec> {
        // Box::<Self>::default()
        Box::new(self.clone())
    }
}
