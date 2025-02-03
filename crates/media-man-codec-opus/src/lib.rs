use std::{collections::VecDeque, sync::Arc};

use media_man_core::{
    DecodeError, EncodeError, FrameAudio, Packet, SampleFormat, SettingsError, TCodecAudio,
    TDecoderAudio, TEncoderAudio, TSettings, Value, ValueInfo,
};
use opus_sys_kman as sys;

#[derive(Debug)]
pub enum OpusError {
    UnknownError(i32),
    BadArg,
    BufferToSmall,
    InternalError,
    InvalidPacket,
    Uimplemented,
    InvalidState,
    AllocFail,
}

impl From<i32> for OpusError {
    fn from(value: i32) -> Self {
        match value {
            sys::OPUS_BAD_ARG => OpusError::BadArg,
            sys::OPUS_BUFFER_TO_SMALL => OpusError::BufferToSmall,
            sys::OPUS_INTERNAL_ERROR => OpusError::InternalError,
            sys::OPUS_INVALID_PACKET => OpusError::InvalidPacket,
            sys::OPUS_UNIMPLEMENTED => OpusError::Uimplemented,
            sys::OPUS_INVALID_STATE => OpusError::InvalidState,
            _ => OpusError::UnknownError(value),
        }
    }
}

pub struct OpusEncoderSettings {
    sample_rate: u32,
    channels: u8,
    sample_float_format: bool,

    application: i32,

    framesize: u32,
    bit_rate: u32,
}

impl TSettings for OpusEncoderSettings {
    fn get_settings_len(&self) -> usize {
        2
    }

    fn get_setting_info(&self, idx: usize) -> Option<media_man_core::ValueInfo> {
        Some(match idx {
            0 => {
                ValueInfo::new_with("Frame Size", "", 5004u32)
                    + ValueInfo::new_with("2.5ms", "", 5001u32)
                    + ValueInfo::new_with("5ms", "", 5002u32)
                    + ValueInfo::new_with("10ms", "", 5003u32)
                    + ValueInfo::new_with("20ms", "", 5004u32)
                    + ValueInfo::new_with("40ms", "", 5005u32)
                    + ValueInfo::new_with("100ms", "", 5008u32)
            }
            1 => ValueInfo::new_with("Bit Rate", "", 64000u32),
            _ => return None,
        })
    }

    fn set_setting(
        &mut self,
        idx: usize,
        value: media_man_core::Value,
    ) -> Result<(), media_man_core::SettingsError> {
        let Some(info) = self.get_setting_info(idx) else {
            return Err(SettingsError::InvalidSettingIndex);
        };
        if !info.valid(value) {
            return Err(SettingsError::InvalidValue);
        }
        match idx {
            0 => self.framesize = value.try_into().unwrap(),
            1 => self.bit_rate = value.try_into().unwrap(),
            _ => return Err(SettingsError::InvalidSettingIndex),
        }
        Ok(())
    }

    fn get_setting(&mut self, idx: usize) -> Result<Value, media_man_core::SettingsError> {
        Ok(match idx {
            0 => self.framesize.into(),
            1 => self.bit_rate.into(),
            _ => return Err(SettingsError::InvalidSettingIndex),
        })
    }
}

pub struct OpusDecoderSettings {
    sample_rate: u32,
    channels: u8,
    sample_float_format: bool,
}

impl TSettings for OpusDecoderSettings {
    fn get_settings_len(&self) -> usize {
        0
    }

    fn get_setting_info(&self, _idx: usize) -> Option<ValueInfo> {
        None
    }

    fn set_setting(&mut self, _idx: usize, _value: Value) -> Result<(), SettingsError> {
        Err(SettingsError::InvalidSettingIndex)
    }

    fn get_setting(&mut self, _idx: usize) -> Result<Value, SettingsError> {
        Err(SettingsError::InvalidSettingIndex)
    }
}

pub struct EncoderAudioOpus {
    library: Arc<sys::OpusLibSys>,
    encoder: sys::OpusEncoder,
    settings: Box<OpusEncoderSettings>,
    samples: VecDeque<u8>,
    packets: VecDeque<Packet>,
}

impl EncoderAudioOpus {
    pub fn new_with(
        library: &Arc<sys::OpusLibSys>,
        settings: Box<OpusEncoderSettings>,
    ) -> Result<Self, OpusError> {
        let mut res = 0i32;
        let mut encoder = unsafe {
            library.opus_encoder_create(
                settings.sample_rate as i32,
                settings.channels as i32,
                settings.application,
                &mut res,
            )
        };

        unsafe {
            library.opus_encoder_set_expect_frame_duration(&mut encoder, settings.framesize as i32);
            library.opus_encoder_set_bitrate(&mut encoder, settings.bit_rate as i32);
        }

        if res == 0 {
            Ok(EncoderAudioOpus {
                library: library.clone(),
                encoder,
                settings,
                samples: VecDeque::default(),
                packets: VecDeque::default(),
            })
        } else {
            Err(res.into())
        }
    }
}

impl TSettings for EncoderAudioOpus {
    fn get_settings_len(&self) -> usize {
        self.settings.get_settings_len()
    }

    fn get_setting_info(&self, idx: usize) -> Option<ValueInfo> {
        self.settings.get_setting_info(idx)
    }

    fn set_setting(&mut self, idx: usize, value: Value) -> Result<(), SettingsError> {
        self.settings.set_setting(idx, value)?;
        unsafe {
            self.library
                .opus_encoder_set_bitrate(&mut self.encoder, self.settings.bit_rate as i32);
            self.library.opus_encoder_set_expect_frame_duration(
                &mut self.encoder,
                self.settings.framesize as i32,
            );
        }
        Ok(())
    }

    fn get_setting(&mut self, idx: usize) -> Result<Value, SettingsError> {
        self.settings.get_setting(idx)
    }
}

impl TEncoderAudio for EncoderAudioOpus {
    fn encode(&mut self, frames: &[&media_man_core::FrameAudio]) -> Result<(), EncodeError> {
        if frames.len() != self.settings.channels as usize {
            return Err(EncodeError::InvalidNumberOfFrames);
        }

        for (i, frame) in frames.iter().enumerate() {
            if self.settings.sample_float_format {
                if frame.sample_format != SampleFormat::F32 {
                    return Err(EncodeError::InvalidSampleFormatFor(i as u8));
                }
            } else if frame.sample_format != SampleFormat::I16 {
                return Err(EncodeError::InvalidSampleFormatFor(i as u8));
            }
        }

        {
            let len = frames[0].data.len();
            for frame in &frames[1..] {
                if len != frame.data.len() {
                    return Err(EncodeError::FramesDontHaveTheSameSize);
                }
            }
        }

        let mut data = [0u8; 64000];

        let framesize = match self.settings.framesize {
            5001u32 => self.settings.sample_rate / 400, // 2.5ms
            5002u32 => self.settings.sample_rate / 200, // 5ms
            5003u32 => self.settings.sample_rate / 100, // 10ms
            5004u32 => self.settings.sample_rate / 50,  // 20ms
            5005u32 => self.settings.sample_rate / 25,  // 40ms
            5008u32 => self.settings.sample_rate / 10,  // 100ms
            _ => {
                unimplemented!()
            }
        };

        unsafe {
            if self.settings.sample_float_format {
                for sample_index in 0..frames[0].data.len() / size_of::<f32>() {
                    for frame in frames {
                        self.samples.extend(std::slice::from_raw_parts(
                            frame.data.as_ptr().add(sample_index * size_of::<f32>()),
                            size_of::<f32>(),
                        ));
                    }
                }
            } else {
                for sample_index in 0..frames[0].data.len() / size_of::<i16>() {
                    for frame in frames {
                        self.samples.extend(std::slice::from_raw_parts(
                            frame.data.as_ptr().add(sample_index * size_of::<i16>()),
                            size_of::<i16>(),
                        ))
                    }
                }
            }
        }

        unsafe {
            if self.settings.sample_float_format {
                let chunk_size =
                    framesize as usize * size_of::<f32>() * self.settings.channels as usize;
                while self.samples.len() >= chunk_size {
                    let buffer = self.samples.drain(..chunk_size).collect::<Vec<u8>>();
                    let res = self.library.opus_encode_float(
                        &mut self.encoder,
                        buffer.as_ptr() as *const _,
                        ((buffer.len() / size_of::<f32>()) / self.settings.channels as usize)
                            as i32,
                        data.as_mut_ptr(),
                        data.len() as i32,
                    );
                    dbg!(res);
                    if res > 0 {
                        self.packets.push_back(Packet {
                            data: data[0..res as usize].to_vec(),
                        });
                    } else {
                        return Err(EncodeError::Custom(format!("{:?}", OpusError::from(res))));
                    }
                }
            } else {
                let chunk_size =
                    framesize as usize * size_of::<i16>() * self.settings.channels as usize;
                while self.samples.len() >= chunk_size {
                    let buffer = self.samples.drain(..chunk_size).collect::<Vec<u8>>();
                    let res = self.library.opus_encode(
                        &mut self.encoder,
                        buffer.as_ptr() as *const _,
                        buffer.len() as i32
                            / self.settings.channels as i32
                            / size_of::<i16>() as i32,
                        data.as_mut_ptr(),
                        data.len() as i32,
                    );
                    if res > 0 {
                        self.packets.push_back(Packet {
                            data: data[0..res as usize].to_vec(),
                        });
                    } else {
                        return Err(EncodeError::Custom(format!("{:?}", OpusError::from(res))));
                    }
                }
            }
        }

        Ok(())
    }

    fn get_packet(&mut self) -> Option<Packet> {
        self.packets.pop_front()
    }
}

impl Drop for EncoderAudioOpus {
    fn drop(&mut self) {
        unsafe { self.library.opus_encoder_destroy(&mut self.encoder) };
    }
}

pub struct DecoderAudioOpus {
    library: Arc<sys::OpusLibSys>,
    decoder: sys::OpusDecoder,
    settings: Box<OpusDecoderSettings>,
}

impl DecoderAudioOpus {
    pub fn new_with(
        library: Arc<sys::OpusLibSys>,
        settings: Box<OpusDecoderSettings>,
    ) -> Result<Self, OpusError> {
        let mut ret = 0i32;
        let decoder;
        unsafe {
            decoder = library.opus_decoder_create(
                settings.sample_rate as i32,
                settings.channels as i32,
                &mut ret,
            );
        }

        if ret != 0 {
            return Err(OpusError::from(ret));
        }

        Ok(Self {
            library,
            decoder,
            settings,
        })
    }
}

impl TSettings for DecoderAudioOpus {
    fn get_settings_len(&self) -> usize {
        0
    }

    fn get_setting_info(&self, _idx: usize) -> Option<ValueInfo> {
        None
    }

    fn set_setting(&mut self, _idx: usize, _value: Value) -> Result<(), SettingsError> {
        Err(SettingsError::InvalidSettingIndex)
    }

    fn get_setting(&mut self, _idx: usize) -> Result<Value, SettingsError> {
        Err(SettingsError::InvalidSettingIndex)
    }
}

impl TDecoderAudio for DecoderAudioOpus {
    fn decode(
        &mut self,
        packet: Packet,
    ) -> Result<Vec<media_man_core::FrameAudio>, media_man_core::DecodeError> {
        if self.settings.sample_float_format {
            let mut pcm = [0f32; 4800 * 2];
            let frame_size;
            unsafe {
                frame_size = self.library.opus_decode_float(
                    &mut self.decoder,
                    packet.data.as_ptr(),
                    packet.data.len() as i32,
                    pcm.as_mut_ptr(),
                    ((pcm.len() / 2) / size_of::<f32>()) as i32,
                    0,
                );
            }

            if frame_size < 0 {
                return Err(DecodeError::Custom(format!(
                    "{:?}",
                    OpusError::from(frame_size)
                )));
            }

            let mut frames = Vec::with_capacity(self.settings.channels as usize);
            for _ in 0..self.settings.channels as usize {
                frames.push(FrameAudio {
                    sample_format: SampleFormat::F32,
                    data: vec![0u8; frame_size as usize * size_of::<f32>()],
                });
            }

            for i in 0..frame_size as usize {
                (0..self.settings.channels as usize).for_each(|ii| unsafe {
                    (frames[ii].data.as_mut_ptr() as *mut f32).add(i).write(
                        pcm.as_ptr()
                            .add((i * self.settings.channels as usize) + ii)
                            .read(),
                    )
                });
            }

            Ok(frames)
        } else {
            let mut pcm = [0i16; 4800 * 2];
            let frame_size;
            unsafe {
                frame_size = self.library.opus_decode(
                    &mut self.decoder,
                    packet.data.as_ptr(),
                    packet.data.len() as i32,
                    pcm.as_mut_ptr(),
                    (pcm.len() / 2) as i32,
                    0,
                );
            }

            if frame_size < 0 {
                return Err(DecodeError::Custom(format!(
                    "{:?}",
                    OpusError::from(frame_size)
                )));
            }

            let mut frames = Vec::with_capacity(self.settings.channels as usize);
            for _ in 0..self.settings.channels as usize {
                frames.push(FrameAudio {
                    sample_format: SampleFormat::I16,
                    data: vec![0u8; frame_size as usize * size_of::<i16>()],
                });
            }

            for i in 0..frame_size as usize {
                (0..self.settings.channels as usize).for_each(|ii| unsafe {
                    (frames[ii].data.as_mut_ptr() as *mut i16)
                        .add(i)
                        .write(pcm.as_ptr().add((i * 2) + ii).read())
                });
            }

            Ok(frames)
        }
    }
}

impl Drop for DecoderAudioOpus {
    fn drop(&mut self) {
        unsafe { self.library.opus_decoder_destroy(&mut self.decoder) };
    }
}

pub struct CodecAudioOpus {
    library: Arc<sys::OpusLibSys>,
}

impl CodecAudioOpus {
    pub fn new() -> Option<Self> {
        let lib = unsafe { sys::OpusLibSys::new() }?;

        Some(Self {
            library: Arc::new(lib),
        })
    }
}

impl TCodecAudio for CodecAudioOpus {
    fn name(&self) -> String {
        "opus".to_owned()
    }

    fn description(&self) -> String {
        "opus codec".to_owned()
    }

    fn default_encoder_settings(
        &self,
        sample_format: media_man_core::SampleFormat,
        sample_rate: u32,
        channels: u8,
    ) -> Result<Box<dyn TSettings>, SettingsError> {
        match sample_format {
            SampleFormat::I16 | SampleFormat::F32 => {}
            _ => return Err(SettingsError::NotSupportedSampleFormat),
        }

        if !matches!(channels, 1 | 2) {
            return Err(SettingsError::NotSupportedChannels);
        }

        Ok(Box::new(OpusEncoderSettings {
            bit_rate: 64000,
            framesize: 5004,
            sample_rate,
            channels,
            sample_float_format: sample_format == SampleFormat::F32,
            application: sys::OPUS_APPLICATION_AUDIO,
        }))
    }

    fn create_encoder(
        &self,
        settings: Box<dyn TSettings>,
    ) -> Result<Box<dyn TEncoderAudio>, SettingsError> {
        let settings = (unsafe {
            std::mem::transmute::<Box<dyn TSettings>, Box<dyn std::any::Any>>(settings)
        })
        .downcast::<OpusEncoderSettings>()
        .map_err(|_| SettingsError::InvalidSettingsType)?;

        Ok(Box::new(
            EncoderAudioOpus::new_with(&self.library, settings).unwrap(),
        ))
    }

    fn default_decoder_settings(
        &self,
        sample_format: media_man_core::SampleFormat,
        sample_rate: u32,
        channels: u8,
    ) -> Result<Box<dyn TSettings>, SettingsError> {
        match sample_format {
            SampleFormat::I16 | SampleFormat::F32 => {}
            _ => return Err(SettingsError::NotSupportedSampleFormat),
        }

        if !matches!(channels, 1 | 2) {
            return Err(SettingsError::NotSupportedChannels);
        }

        Ok(Box::new(OpusDecoderSettings {
            sample_rate,
            channels,
            sample_float_format: sample_format == SampleFormat::F32,
        }))
    }

    fn create_decoder(
        &self,
        settings: Box<dyn TSettings>,
    ) -> Result<Box<dyn TDecoderAudio>, SettingsError> {
        let settings = (unsafe {
            std::mem::transmute::<Box<dyn TSettings>, Box<dyn std::any::Any>>(settings)
        })
        .downcast::<OpusDecoderSettings>()
        .map_err(|_| SettingsError::InvalidSettingsType)?;

        Ok(Box::new(
            DecoderAudioOpus::new_with(self.library.clone(), settings).unwrap(),
        ))
    }
}
