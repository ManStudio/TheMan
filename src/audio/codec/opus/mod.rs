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
    pub bitrate: i32,
}

impl Clone for CodecOpus {
    fn clone(&self) -> Self {
        let mut s = Self {
            errors: self.errors.clone(),
            decoder: Decoder::new(self.sample_rate, self.channels).unwrap(),
            encoder: Encoder::new(self.sample_rate, self.channels, self.application).unwrap(),
            channels: self.channels,
            application: self.application,
            sample_rate: self.sample_rate,
            input_buffer: self.input_buffer.clone(),
            output_buffer: self.output_buffer.clone(),
            bitrate: self.bitrate,
        };

        s.setup_buffers();
        s.setup();

        s
    }
}

unsafe impl Send for CodecOpus {}
unsafe impl Sync for CodecOpus {}

impl Default for CodecOpus {
    fn default() -> Self {
        println!("Opus version: {}", opus::version());
        let mut s = Self {
            errors: Vec::new(),
            decoder: Decoder::new(8000, opus::Channels::Mono).unwrap(),
            encoder: Encoder::new(8000, opus::Channels::Mono, opus::Application::Voip).unwrap(),
            channels: opus::Channels::Mono,
            application: opus::Application::Voip,
            sample_rate: 8000,
            input_buffer: Vec::new(),
            output_buffer: Vec::new(),
            bitrate: 128000,
        };

        s.setup_buffers();
        s.setup();

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
        println!("Len: {}", len);
        self.input_buffer.resize(len, 0.0);
        self.output_buffer.resize(len, 0);
    }

    fn setup(&mut self) {
        println!("Channels: {:?}", self.channels);
        self.encoder
            .set_bitrate(opus::Bitrate::Bits(self.bitrate))
            .unwrap();
        self.encoder.set_inband_fec(true);
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
                        Ok(mut encoder) => self.encoder = encoder,
                        Err(err) => self.errors.push(format!("OpusEncoder: Error: {err}")),
                    }

                    self.sample_rate = value as u32;
                    self.setup_buffers();
                    self.setup();
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
                    let new_encoder = Encoder::new(self.sample_rate, channels, self.application);

                    match new_decoder {
                        Ok(decoder) => self.decoder = decoder,
                        Err(err) => self.errors.push(format!("OpusDecoder: Error: {err}")),
                    }
                    match new_encoder {
                        Ok(mut encoder) => self.encoder = encoder,
                        Err(err) => self.errors.push(format!("OpusEncoder: Error: {err}")),
                    }

                    self.channels = channels;
                    self.setup_buffers();
                    self.setup();
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

    fn encode(&mut self, data: &mut Vec<f32>) -> Vec<u8> {
        println!("Input: {}", data.len());
        let mut buffer = Vec::new();
        let chunk = (self.sample_rate as usize
            * match self.channels {
                opus::Channels::Mono => 1,
                opus::Channels::Stereo => 2,
            }
            / 1000)
            * 20 //ms
        ;
        let mut size;
        let mut to_remove;
        while {
            size = data.len();
            to_remove = size - (size % chunk);
            to_remove >= chunk
        } {
            println!(
                "Process: {to_remove}, Samplerate: {}, Channels: {:?}",
                self.sample_rate, self.channels
            );
            let data = data.drain(..to_remove).collect::<Vec<f32>>();
            match self.encoder.encode_float(&data, &mut self.output_buffer) {
                Ok(len) => buffer.append(&mut self.output_buffer[0..len].to_vec().to_bytes()),
                Err(err) => {
                    self.errors
                        .push(format!("OpusEncoder encoding error: {err:?}"));
                    return buffer;
                }
            }
        }
        buffer
    }

    fn decode(&mut self, data: &mut Vec<u8>) -> Vec<f32> {
        let mut buffer = Vec::new();
        while data.len() >= 0usize.size() {
            let data = Vec::<u8>::from_bytes(&mut data.drain(..)).unwrap();
            if data.is_empty() {
                continue;
            }
            match self
                .decoder
                .decode_float(&data, &mut self.input_buffer, false)
            {
                Ok(len) => buffer.append(&mut self.input_buffer[0..len].to_vec()),
                Err(err) => {
                    self.errors
                        .push(format!("OpusDecoder decoding error: {err:?}"));
                }
            }
        }

        println!(
            "Decoded: {}, SampleRate: {}, Channels: {:?}",
            buffer.len(),
            self.sample_rate,
            self.channels
        );

        buffer
    }

    fn c(&self) -> Box<dyn Codec> {
        // Box::<Self>::default()
        Box::new(self.clone())
    }
}

#[test]
fn opus() {
    let input_buffer: Vec<f32> = vec![0.0; 1920 * 2]; // Add more samples as needed

    // Opus encoder parameters
    let sample_rate = 48000;

    // Calculate the number of frames in the buffer

    // Create an Opus encoder with the specified parameters
    let mut encoder = Encoder::new(
        sample_rate,
        opus::Channels::Stereo,
        opus::Application::Audio,
    )
    .unwrap();

    // Calculate the maximum size of the output buffer
    let max_output_len = 4000; // Adjust the size as needed

    // Create an output buffer to store the encoded Opus data
    let mut output_buffer = vec![0u8; max_output_len];

    // Encode the input audio buffer
    let encoded_len = encoder
        .encode_float(&input_buffer, &mut output_buffer)
        .unwrap();

    // Print the length of the encoded Opus data
    println!("Encoded length: {}", encoded_len);
}

#[test]
fn tt() {
    let mut buffer = vec![0.0; 6000];
    let multiple = 920;

    let current_size = buffer.len();
    let elements_to_extract = current_size - (current_size % multiple);
    panic!("ele: {}", elements_to_extract);

    let extracted_slice = buffer.split_off(elements_to_extract);

    panic!("Drained slice size: {}", extracted_slice.len());
}
