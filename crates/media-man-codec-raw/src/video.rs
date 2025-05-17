use std::{any::Any, collections::VecDeque, sync::Arc};

use media_man_core::{
    DecodeError, Format, FrameVideo, Packet, SettingsError, TCodecVideo, TDecoderVideo,
    TEncoderVideo, TSettings, Value, ValueInfo,
};

#[derive(bincode::Encode, bincode::Decode)]
pub struct RawVideoFrame {
    width: u32,
    height: u32,
    data: Arc<[u8]>,
}

pub struct CodecVideoRaw;

struct VideoEncoderSettings;

impl TSettings for VideoEncoderSettings {
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

struct VideoDecoderSettings;

impl TSettings for VideoDecoderSettings {
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

#[derive(Default)]
pub struct VideoEncoder {
    packets: VecDeque<Packet>,
}

impl TSettings for VideoEncoder {
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

impl TEncoderVideo for VideoEncoder {
    fn encode(&mut self, frame: &FrameVideo) -> Result<(), media_man_core::EncodeError> {
        match bincode::encode_to_vec(
            RawVideoFrame {
                width: frame.width,
                height: frame.height,
                data: frame.data.clone(),
            },
            bincode::config::standard(),
        ) {
            Ok(packet) => {
                self.packets.push_back(Packet { data: packet });
                Ok(())
            }
            Err(err) => Err(media_man_core::EncodeError::Custom(format!(
                "Cannot encode: {err}"
            ))),
        }
    }

    fn get_packet(&mut self) -> Option<Packet> {
        self.packets.pop_front()
    }
}

struct VideoDecoder;

impl TSettings for VideoDecoder {
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

impl TDecoderVideo for VideoDecoder {
    fn decode(&mut self, packet: Packet) -> Result<FrameVideo, DecodeError> {
        match bincode::decode_from_slice::<RawVideoFrame, _>(
            &packet.data,
            bincode::config::standard(),
        ) {
            Ok((frame, _)) => Ok(FrameVideo {
                width: frame.width,
                height: frame.height,
                format: Format::RGBA,
                data: frame.data,
            }),
            Err(err) => Err(DecodeError::Custom(format!("Cannot decode: {err}"))),
        }
    }
}

impl TCodecVideo for CodecVideoRaw {
    fn name(&self) -> String {
        String::from("video-raw")
    }

    fn description(&self) -> String {
        String::from("Raw video codec")
    }

    fn default_encoder_settings(
        &self,
        format: media_man_core::Format,
    ) -> Result<Box<dyn media_man_core::TSettings>, SettingsError> {
        if format != Format::RGBA {
            return Err(SettingsError::NotSupportedFormat);
        }

        Ok(Box::new(VideoEncoderSettings))
    }

    fn create_encoder(
        &self,
        settings: Box<dyn media_man_core::TSettings>,
    ) -> Result<Box<dyn media_man_core::TEncoderVideo>, SettingsError> {
        match (settings as Box<dyn Any>).downcast::<VideoEncoderSettings>() {
            Ok(_) => Ok(Box::new(VideoEncoder::default())),
            Err(_) => Err(SettingsError::InvalidSettingsType),
        }
    }

    fn default_decoder_settings(
        &self,
        format: media_man_core::Format,
    ) -> Result<Box<dyn media_man_core::TSettings>, SettingsError> {
        if format != Format::RGBA {
            return Err(SettingsError::NotSupportedFormat);
        }

        Ok(Box::new(VideoDecoderSettings))
    }

    fn create_decoder(
        &self,
        settings: Box<dyn media_man_core::TSettings>,
    ) -> Result<Box<dyn media_man_core::TDecoderVideo>, SettingsError> {
        match (settings as Box<dyn Any>).downcast::<VideoDecoderSettings>() {
            Ok(_) => Ok(Box::new(VideoDecoder)),
            Err(_) => Err(SettingsError::InvalidSettingsType),
        }
    }
}
