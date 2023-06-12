use audiopus::{Application, Channels, SampleRate};
use bytes_kman::TBytes;
use the_man::Atom;

use super::Codec;

pub struct CodecOpus {
    encoder: audiopus::coder::Encoder,
    decoder: audiopus::coder::Decoder,

    sample_rate: SampleRate,
    channels: Channels,
    application: Application,

    errors: Vec<String>,

    output_buffer: Vec<u8>,
    input_buffer: Vec<f32>,
}

unsafe impl Sync for CodecOpus {}
unsafe impl Send for CodecOpus {}

impl CodecOpus {
    pub fn new(sample_rate: SampleRate, channels: Channels, application: Application) -> Self {
        let encoder = audiopus::coder::Encoder::new(sample_rate, channels, application).unwrap();

        let decoder = audiopus::coder::Decoder::new(sample_rate, channels).unwrap();

        Self {
            encoder,
            decoder,
            sample_rate,
            channels,
            application,
            errors: Vec::new(),
            output_buffer: vec![0; 4096],
            input_buffer: vec![0.0; 48000],
        }
    }
    fn init(&mut self, sample_rate: SampleRate, channels: Channels, application: Application) {
        let encoder = audiopus::coder::Encoder::new(sample_rate, channels, application);
        let decoder = audiopus::coder::Decoder::new(sample_rate, channels);

        let encoder = match encoder {
            Ok(e) => e,
            Err(err) => {
                self.errors
                    .push(format!("CodecOpus error when creating Encoder: {err}"));
                return;
            }
        };
        let decoder = match decoder {
            Ok(e) => e,
            Err(err) => {
                self.errors
                    .push(format!("CodecOpus error when creating Decoder: {err}"));
                return;
            }
        };

        self.sample_rate = sample_rate;
        self.channels = channels;
        self.application = application;

        self.encoder = encoder;
        self.decoder = decoder;
    }
}

impl Default for CodecOpus {
    fn default() -> Self {
        let sample_rate = SampleRate::Hz8000;
        let channels = Channels::Mono;
        let application = Application::Audio;
        Self::new(sample_rate, channels, application)
    }
}

impl Codec for CodecOpus {
    fn name(&self) -> &str {
        "opus"
    }

    fn settings(&self) -> Vec<String> {
        vec![
            "sample_rate".into(),
            "channels".into(),
            "application".into(),
        ]
    }

    fn get_setting(&mut self, key: String) -> Option<the_man::Atom> {
        match key.trim() {
            "sample_rate" => Some(the_man::Atom::UnSignedValues {
                value: self.sample_rate as i32 as usize,
                values: vec![8000, 12000, 16000, 24000, 48000],
            }),
            "channels" => Some(the_man::Atom::UnSignedValues {
                value: if self.channels.is_stereo() { 2 } else { 1 },
                values: vec![1, 2],
            }),
            "application" => Some(the_man::Atom::StringValues {
                value: match self.application {
                    Application::Voip => "Voip".into(),
                    Application::Audio => "Audio".into(),
                    Application::LowDelay => "LowDelay".into(),
                },
                values: vec!["Voip".into(), "Audio".into(), "LowDelay".into()],
            }),
            _ => None,
        }
    }

    fn set_setting(&mut self, key: String, value: the_man::Atom) {
        match key.trim() {
            "sample_rate" => {
                if value.valid() {
                    if let Atom::UnSignedValues { value, .. } = value {
                        let sample_rate = match value {
                            8000 => SampleRate::Hz8000,
                            12000 => SampleRate::Hz12000,
                            16000 => SampleRate::Hz16000,
                            24000 => SampleRate::Hz24000,
                            48000 => SampleRate::Hz48000,
                            _ => return,
                        };
                        self.init(sample_rate, self.channels, self.application)
                    }
                }
            }
            "channels" => {
                if value.valid() {
                    if let Atom::UnSignedValues { value, .. } = value {
                        let channels = match value {
                            1 => Channels::Mono,
                            2 => Channels::Stereo,
                            _ => return,
                        };
                        self.init(self.sample_rate, channels, self.application)
                    }
                }
            }
            "application" => {
                if value.valid() {
                    if let Atom::StringValues { value, .. } = value {
                        let application = match value.trim() {
                            "Voip" => Application::Voip,
                            "Audio" => Application::Audio,
                            "LowDelay" => Application::LowDelay,
                            _ => return,
                        };
                        self.init(self.sample_rate, self.channels, application)
                    }
                }
            }
            _ => {}
        }
    }

    fn errors(&mut self) -> Vec<String> {
        std::mem::take(&mut self.errors)
    }

    fn encode(&mut self, data: &mut Vec<f32>) -> Vec<u8> {
        let mut buffer = Vec::new();
        let chunk =
            ((self.sample_rate as i32 as usize / 1000) * self.channels as i32 as usize) * 20;
        let mut size;
        let mut to_remove;
        while {
            size = data.len();
            to_remove = size - (size % chunk);
            to_remove >= chunk
        } {
            let data = data.drain(..to_remove).collect::<Vec<f32>>();
            match self.encoder.encode_float(&data, &mut self.output_buffer) {
                Ok(len) => {
                    buffer.append(&mut self.output_buffer[..len].to_vec().to_bytes());
                }
                Err(err) => self
                    .errors
                    .push(format!("OpusCodec error when encoding: {err}")),
            }
        }
        buffer
    }

    fn decode(&mut self, data: &mut Vec<u8>) -> Vec<f32> {
        let mut buffer = Vec::new();
        while data.len() >= 0usize.size() {
            let Some(data) = Vec::<u8>::from_bytes(data) else {break};
            if data.is_empty() {
                continue;
            }
            match self.decoder.decode_float(
                Some(data.as_slice().try_into().unwrap()),
                self.input_buffer.as_mut_slice().try_into().unwrap(),
                false,
            ) {
                Ok(len) => buffer
                    .append(&mut self.input_buffer[..len * self.channels as i32 as usize].to_vec()),
                Err(err) => self
                    .errors
                    .push(format!("OpusCodec error when decoding: {err}")),
            }
        }

        buffer
    }

    fn c(&self) -> Box<dyn Codec> {
        Box::new(Self::new(self.sample_rate, self.channels, self.application))
    }
}
