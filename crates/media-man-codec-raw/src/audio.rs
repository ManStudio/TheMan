#![allow(clippy::needless_range_loop)]

use std::{any::Any, collections::VecDeque};

use media_man_core::{
    DecodeError, EncodeError, FrameAudio, Packet, SampleFormat, SettingsError, TCodecAudio,
    TDecoderAudio, TEncoderAudio, TSettings, Value, ValueInfo,
};

pub struct CodecAudioRaw;

#[derive(bincode::Decode, bincode::Encode)]
enum RawAudioFrame {
    I16 {
        channels: u8,
        sample_rate: u32,
        data: Vec<i16>,
    },
    F32 {
        channels: u8,
        sample_rate: u32,
        data: Vec<f32>,
    },
}

struct AudioEncoderSettings {
    channels: u8,
    sample_rate: u32,
    sample_format: SampleFormat,
}

impl TSettings for AudioEncoderSettings {
    fn get_settings_len(&self) -> usize {
        0
    }

    fn get_setting_info(&self, _id: usize) -> Option<ValueInfo> {
        None
    }

    fn set_setting(&mut self, _id: usize, _value: Value) -> Result<(), SettingsError> {
        Err(SettingsError::InvalidSettingIndex)
    }

    fn get_setting(&self, _id: usize) -> Result<Value, SettingsError> {
        Err(SettingsError::InvalidSettingIndex)
    }
}

struct AudioEncoder {
    settings: AudioEncoderSettings,
    packets: VecDeque<Packet>,
}

impl TSettings for AudioEncoder {
    fn get_settings_len(&self) -> usize {
        self.settings.get_settings_len()
    }

    fn get_setting_info(&self, id: usize) -> Option<ValueInfo> {
        self.settings.get_setting_info(id)
    }

    fn set_setting(&mut self, id: usize, value: Value) -> Result<(), SettingsError> {
        self.settings.set_setting(id, value)
    }

    fn get_setting(&self, id: usize) -> Result<Value, SettingsError> {
        self.settings.get_setting(id)
    }
}

impl TEncoderAudio for AudioEncoder {
    fn encode(&mut self, frames: &[&FrameAudio]) -> Result<(), EncodeError> {
        if frames.len() != self.settings.channels as usize {
            return Err(EncodeError::InvalidNumberOfFrames);
        }

        {
            let mut frame_size: Option<usize> = None;
            for (i, frame) in frames.iter().enumerate() {
                if frame.sample_format != self.settings.sample_format {
                    return Err(EncodeError::InvalidSampleFormatFor(i as u8));
                }

                if let Some(frame_size) = frame_size {
                    if frame_size != frame.len() {
                        return Err(EncodeError::FramesDontHaveTheSameSize);
                    }
                } else {
                    frame_size = Some(frame.len());
                }
            }
        }

        let audio_frame = match self.settings.sample_format {
            SampleFormat::I16 => {
                let mut data = vec![0i16; frames[0].as_i16().len() * frames.len()];

                for (i, frame) in frames.iter().enumerate() {
                    for (ii, sample) in frame.as_i16().iter().enumerate() {
                        data[i + ii * frames.len()] = *sample;
                    }
                }

                RawAudioFrame::I16 {
                    channels: self.settings.channels,
                    sample_rate: self.settings.sample_rate,
                    data,
                }
            }
            SampleFormat::F32 => {
                let mut data = vec![0f32; frames[0].as_f32().len() * frames.len()];

                for (i, frame) in frames.iter().enumerate() {
                    for (ii, sample) in frame.as_f32().iter().enumerate() {
                        data[i + ii * frames.len()] = *sample;
                    }
                }

                RawAudioFrame::F32 {
                    channels: self.settings.channels,
                    sample_rate: self.settings.sample_rate,
                    data,
                }
            }
            _ => {
                return Err(EncodeError::Custom(format!(
                    "Unimplemented sample format: {:?}",
                    self.settings.sample_format
                )));
            }
        };

        match bincode::encode_to_vec(audio_frame, bincode::config::standard()) {
            Ok(data) => {
                self.packets.push_back(Packet { data });
            }
            Err(err) => {
                return Err(EncodeError::Custom(format!("Cannot encode: {err}")));
            }
        }

        Ok(())
    }

    fn get_packet(&mut self) -> Option<Packet> {
        self.packets.pop_front()
    }
}

struct AudioDecoderSettings {
    channels: u8,
    sample_rate: u32,
    sample_format: SampleFormat,
}

impl TSettings for AudioDecoderSettings {
    fn get_settings_len(&self) -> usize {
        0
    }

    fn get_setting_info(&self, _id: usize) -> Option<ValueInfo> {
        None
    }

    fn set_setting(&mut self, _id: usize, _value: Value) -> Result<(), SettingsError> {
        Err(SettingsError::InvalidSettingIndex)
    }

    fn get_setting(&self, _id: usize) -> Result<Value, SettingsError> {
        Err(SettingsError::InvalidSettingIndex)
    }
}

struct AudioDecoder {
    settings: AudioDecoderSettings,
}

impl TSettings for AudioDecoder {
    fn get_settings_len(&self) -> usize {
        self.settings.get_settings_len()
    }

    fn get_setting_info(&self, id: usize) -> Option<ValueInfo> {
        self.settings.get_setting_info(id)
    }

    fn set_setting(&mut self, id: usize, value: Value) -> Result<(), SettingsError> {
        self.settings.set_setting(id, value)
    }

    fn get_setting(&self, id: usize) -> Result<Value, SettingsError> {
        self.settings.get_setting(id)
    }
}

impl TDecoderAudio for AudioDecoder {
    fn decode(&mut self, packet: Packet) -> Result<Vec<FrameAudio>, DecodeError> {
        let Ok((audio_frame, _)) = bincode::decode_from_slice::<RawAudioFrame, _>(
            &packet.data,
            bincode::config::standard(),
        ) else {
            return Err(DecodeError::InvalidPacket);
        };

        match audio_frame {
            RawAudioFrame::I16 {
                channels,
                sample_rate,
                data,
            } => {
                if self.settings.sample_format != SampleFormat::I16 {
                    return Err(DecodeError::Custom(String::from(
                        "SampleFormat conversion not implemented",
                    )));
                }

                if self.settings.sample_rate != sample_rate {
                    return Err(DecodeError::Custom(String::from(
                        "SampleRate conversion not implemented",
                    )));
                }

                if self.settings.channels != channels {
                    return Err(DecodeError::Custom(String::from(
                        "Channels conversion not implemented",
                    )));
                }

                let mut frames = vec![
                    FrameAudio {
                        sample_format: SampleFormat::I16,
                        data: vec![0u8; data.len() * size_of::<i16>()]
                    };
                    channels as usize
                ];

                for i in 0..channels as usize {
                    for ii in 0..data.len() / channels as usize {
                        let index = i + ii * channels as usize;
                        frames[i].as_i16_mut()[ii] = data[index];
                    }
                }

                Ok(frames)
            }
            RawAudioFrame::F32 {
                channels,
                sample_rate,
                data,
            } => {
                if self.settings.sample_format != SampleFormat::F32 {
                    return Err(DecodeError::Custom(String::from(
                        "SampleFormat conversion not implemented",
                    )));
                }

                if self.settings.sample_rate != sample_rate {
                    return Err(DecodeError::Custom(String::from(
                        "SampleRate conversion not implemented",
                    )));
                }

                if self.settings.channels != channels {
                    return Err(DecodeError::Custom(String::from(
                        "Channels conversion not implemented",
                    )));
                }

                let mut frames = vec![
                    FrameAudio {
                        sample_format: SampleFormat::F32,
                        data: vec![0u8; data.len() * size_of::<f32>()]
                    };
                    channels as usize
                ];

                for i in 0..channels as usize {
                    for ii in 0..data.len() / channels as usize {
                        let index = i + ii * channels as usize;
                        frames[i].as_f32_mut()[ii] = data[index];
                    }
                }

                Ok(frames)
            }
        }
    }
}

impl TCodecAudio for CodecAudioRaw {
    fn name(&self) -> String {
        String::from("audio-raw")
    }

    fn description(&self) -> String {
        String::from("Raw audio, no compression")
    }

    fn default_encoder_settings(
        &self,
        sample_format: SampleFormat,
        sample_rate: u32,
        channels: u8,
    ) -> Result<Box<dyn TSettings>, SettingsError> {
        Ok(Box::new(AudioEncoderSettings {
            channels,
            sample_rate,
            sample_format,
        }))
    }

    fn create_encoder(
        &self,
        settings: Box<dyn TSettings>,
    ) -> Result<Box<dyn media_man_core::TEncoderAudio>, SettingsError> {
        let settings = match (settings as Box<dyn Any>).downcast::<AudioEncoderSettings>() {
            Ok(v) => *v,
            Err(_) => {
                return Err(SettingsError::InvalidSettingsType);
            }
        };

        Ok(Box::new(AudioEncoder {
            settings,
            packets: VecDeque::default(),
        }))
    }

    fn default_decoder_settings(
        &self,
        sample_format: SampleFormat,
        sample_rate: u32,
        channels: u8,
    ) -> Result<Box<dyn TSettings>, SettingsError> {
        Ok(Box::new(AudioDecoderSettings {
            channels,
            sample_rate,
            sample_format,
        }))
    }

    fn create_decoder(
        &self,
        settings: Box<dyn TSettings>,
    ) -> Result<Box<dyn media_man_core::TDecoderAudio>, SettingsError> {
        let settings = match (settings as Box<dyn Any>).downcast::<AudioDecoderSettings>() {
            Ok(v) => *v,
            Err(_) => {
                return Err(SettingsError::InvalidSettingsType);
            }
        };

        Ok(Box::new(AudioDecoder { settings }))
    }
}
