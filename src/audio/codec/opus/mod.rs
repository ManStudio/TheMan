use bytes_kman::TBytes;
use opus_kman::{Application, Decoder, Encoder, SampleRate, TDecoder, TEncoder};

use super::Codec;

pub struct CodecOpus {
    errors: Vec<String>,
    pub decoder: Decoder,
    pub encoder: Encoder,
    pub channels: u32,
    pub application: opus_kman::Application,
    pub input_buffer: Vec<f32>,
    pub output_buffer: Vec<u8>,
    pub bitrate: i32,
}

impl Clone for CodecOpus {
    fn clone(&self) -> Self {
        let mut s = Self {
            errors: self.errors.clone(),
            decoder: Decoder::new(SampleRate::Hz48000, self.channels).unwrap(),
            encoder: Encoder::new(SampleRate::Hz48000, self.channels, self.application.clone())
                .unwrap(),
            channels: self.channels,
            application: self.application.clone(),
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
        let mut s = Self {
            errors: Vec::new(),
            decoder: Decoder::new(SampleRate::Hz48000, 1).unwrap(),
            encoder: Encoder::new(SampleRate::Hz48000, 1, Application::VOIP).unwrap(),
            channels: 1,
            application: Application::VOIP,
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
        let len = 48000 * self.channels as usize;
        self.input_buffer.resize(len, 0.0);
        self.output_buffer.resize(len, 0);
    }

    fn setup(&mut self) {
        // self.encoder
        //     .set_bitrate(opus::Bitrate::Bits(self.bitrate))
        //     .unwrap();
        // let _ = self.encoder.set_inband_fec(true);
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
                value: 48000,
                range: 48000 as usize..48001 as usize,
            }),
            "channels" => Some(the_man::Atom::UnSigned {
                value: self.channels as usize,
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
                if let the_man::Atom::UnSigned { .. } = value {
                    let new_decoder = Decoder::new(SampleRate::Hz48000, self.channels.clone());
                    let new_encoder = Encoder::new(
                        SampleRate::Hz48000,
                        self.channels.clone(),
                        self.application.clone(),
                    );

                    match new_decoder {
                        Ok(decoder) => self.decoder = decoder,
                        Err(err) => self.errors.push(format!("OpusDecoder: Error: {err:?}")),
                    }
                    match new_encoder {
                        Ok(encoder) => self.encoder = encoder,
                        Err(err) => self.errors.push(format!("OpusEncoder: Error: {err:?}")),
                    }

                    self.setup_buffers();
                    self.setup();
                }
            }
            "channels" => {
                if let the_man::Atom::UnSigned { value, .. } = value {
                    let new_decoder = Decoder::new(SampleRate::Hz48000, value as u32);
                    let new_encoder =
                        Encoder::new(SampleRate::Hz48000, value as u32, self.application.clone());

                    match new_decoder {
                        Ok(decoder) => self.decoder = decoder,
                        Err(err) => self.errors.push(format!("OpusDecoder: Error: {err:?}")),
                    }
                    match new_encoder {
                        Ok(encoder) => self.encoder = encoder,
                        Err(err) => self.errors.push(format!("OpusEncoder: Error: {err:?}")),
                    }

                    self.channels = value as u32;
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
        let mut buffer = Vec::new();
        let chunk = (48 as usize
            * self.channels as usize
            )
            * 20 //ms
        ;
        let mut size;
        let mut to_remove;
        while {
            size = data.len();
            to_remove = size - (size % chunk);
            to_remove >= chunk
        } {
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
            let data = Vec::<u8>::from_bytes(data).unwrap();
            if data.is_empty() {
                continue;
            }
            match self
                .decoder
                .decode_float(&data, &mut self.input_buffer, false)
            {
                // TODO: I don't know why this works!
                Ok(len) => {
                    buffer.append(&mut self.input_buffer[0..len * self.channels as usize].to_vec())
                }
                Err(err) => {
                    self.errors
                        .push(format!("OpusDecoder decoding error: {err:?}"));
                }
            }
        }

        buffer
    }

    fn c(&self) -> Box<dyn Codec> {
        // Box::<Self>::default()
        Box::new(self.clone())
    }
}
