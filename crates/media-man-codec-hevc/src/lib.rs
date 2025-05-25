use std::{any::Any, collections::VecDeque, sync::Arc};

use media_man_core::{
    DecodeError, Format, FrameVideo, Packet, SettingsError, TCodecVideo, TDecoderVideo,
    TEncoderVideo, TSettings,
};

pub struct CodecVideoHEVC;

#[derive(bincode::Decode, bincode::Encode)]
struct PacketData {
    pts: i64,
    dts: i64,
    data: Arc<[u8]>,
    stream_index: std::ffi::c_int,
    flags: std::ffi::c_int,
    side_datas: Arc<[(std::ffi::c_uint, Arc<[u8]>)]>,
    duration: i64,
    pos: i64,
    time_base_num: std::ffi::c_int,
    time_base_den: std::ffi::c_int,
}

impl PacketData {
    pub fn from_av_packet(packet: &rsmpeg::avcodec::AVPacket) -> Self {
        let mut data = vec![0u8; packet.size as usize];
        data.copy_from_slice(unsafe {
            std::slice::from_raw_parts(packet.data as *const u8, packet.size as usize)
        });

        let mut side_datas =
            Vec::<(std::ffi::c_uint, Arc<[u8]>)>::with_capacity(packet.side_data_elems as usize);
        for i in 0..packet.side_data_elems as usize {
            unsafe {
                let side_data = packet.side_data.add(i).read();
                let mut data = vec![0u8; side_data.size];
                data.copy_from_slice(std::slice::from_raw_parts(
                    side_data.data as *const u8,
                    side_data.size,
                ));

                side_datas.push((side_data.type_, data.into()));
            }
        }

        Self {
            pts: packet.pts,
            dts: packet.dts,
            data: data.into(),
            stream_index: packet.stream_index,
            flags: packet.flags,
            side_datas: side_datas.into(),
            duration: packet.duration,
            pos: packet.pos,
            time_base_num: packet.time_base.num,
            time_base_den: packet.time_base.den,
        }
    }

    pub fn to_av_packet(&self) -> rsmpeg::avcodec::AVPacket {
        let mut packet = rsmpeg::avcodec::AVPacket::new();

        packet.set_pts(self.pts);
        packet.set_dts(self.dts);

        let data = unsafe { rsmpeg::ffi::av_malloc(self.data.len()) };
        unsafe {
            std::slice::from_raw_parts_mut(data as *mut u8, self.data.len())
                .copy_from_slice(&self.data)
        };

        unsafe {
            rsmpeg::ffi::av_packet_from_data(
                packet.as_mut_ptr(),
                data as *mut u8,
                self.data.len() as i32,
            )
        };

        packet.set_stream_index(self.stream_index);
        packet.set_flags(self.flags);

        for side_data in self.side_datas.iter() {
            unsafe {
                let data = rsmpeg::ffi::av_malloc(side_data.1.len());
                std::slice::from_raw_parts_mut(data as *mut u8, side_data.1.len())
                    .copy_from_slice(&side_data.1);
                rsmpeg::ffi::av_packet_side_data_add(
                    &packet.side_data as *const _ as *mut _,
                    &packet.side_data_elems as *const _ as *mut _,
                    side_data.0,
                    data,
                    side_data.1.len(),
                    0,
                );
            }
        }

        packet.set_duration(self.duration);
        packet.set_pos(self.pos);
        unsafe {
            (&packet.time_base as *const _ as *mut rsmpeg::ffi::AVRational).write(
                rsmpeg::ffi::AVRational {
                    num: self.time_base_num,
                    den: self.time_base_den,
                },
            )
        };

        packet
    }
}

struct VideoEncoderSettings {}

impl TSettings for VideoEncoderSettings {
    fn get_settings_len(&self) -> usize {
        0
    }

    fn get_setting_info(&self, id: usize) -> Option<media_man_core::ValueInfo> {
        None
    }

    fn set_setting(
        &mut self,
        id: usize,
        value: media_man_core::Value,
    ) -> Result<(), SettingsError> {
        Err(SettingsError::InvalidSettingIndex)
    }

    fn get_setting(&self, id: usize) -> Result<media_man_core::Value, SettingsError> {
        Err(SettingsError::InvalidSettingIndex)
    }
}

struct VideoEncoder {
    settings: VideoEncoderSettings,
    packets: VecDeque<Packet>,
    context: Option<rsmpeg::avcodec::AVCodecContext>,
}

unsafe impl Sync for VideoEncoder {}

impl TSettings for VideoEncoder {
    fn get_settings_len(&self) -> usize {
        0
    }

    fn get_setting_info(&self, id: usize) -> Option<media_man_core::ValueInfo> {
        None
    }

    fn set_setting(
        &mut self,
        id: usize,
        value: media_man_core::Value,
    ) -> Result<(), SettingsError> {
        Err(SettingsError::InvalidSettingIndex)
    }

    fn get_setting(&self, id: usize) -> Result<media_man_core::Value, SettingsError> {
        Err(SettingsError::InvalidSettingIndex)
    }
}

impl TEncoderVideo for VideoEncoder {
    fn encode(
        &mut self,
        frame: &media_man_core::FrameVideo,
    ) -> Result<(), media_man_core::EncodeError> {
        if self.context.is_none() {
            // let codec =
            //     rsmpeg::avcodec::AVCodec::find_encoder(rsmpeg::ffi::AV_CODEC_ID_HEVC).unwrap();
            let codec = rsmpeg::avcodec::AVCodec::find_encoder_by_name(c"hevc_nvenc").unwrap();
            eprintln!("Name: {:?}", codec.name());
            eprintln!("Long Name: {:?}", codec.long_name());

            let mut ctx = rsmpeg::avcodec::AVCodecContext::new(&codec);
            ctx.set_bit_rate(400000);
            ctx.set_width(1920);
            ctx.set_height(1080);
            ctx.set_pix_fmt(rsmpeg::ffi::AV_PIX_FMT_YUV444P);
            ctx.set_max_b_frames(3);
            ctx.set_time_base(rsmpeg::ffi::AVRational { num: 1, den: 60 });
            ctx.set_framerate(rsmpeg::ffi::AVRational { num: 60, den: 1 });
            ctx.set_gop_size(60 * 2);
            if ctx.open(None).is_ok() {
                self.context = Some(ctx);
            }
        }

        let Some(context) = &mut self.context else {
            return Err(media_man_core::EncodeError::Custom(
                "Cannot create context".into(),
            ));
        };

        let mut ff_frame = rsmpeg::avutil::AVFrame::new();
        ff_frame.set_format(rsmpeg::ffi::AV_PIX_FMT_YUV444P);
        ff_frame.set_width(frame.width as i32);
        ff_frame.set_height(frame.height as i32);
        ff_frame.alloc_buffer().unwrap();

        unsafe {
            // TODO This should be reused
            let sws_ctx = rsmpeg::ffi::sws_getContext(
                frame.width as i32,
                frame.height as i32,
                rsmpeg::ffi::AV_PIX_FMT_RGBA,
                frame.width as i32,
                frame.height as i32,
                rsmpeg::ffi::AV_PIX_FMT_YUV444P,
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            rsmpeg::ffi::sws_scale(
                sws_ctx,
                &[frame.data.as_ptr()] as *const _,
                &((frame.width * 4) as i32),
                0,
                frame.height as i32,
                ff_frame.data_mut() as *const _,
                ff_frame.linesize_mut() as *const _,
            );

            rsmpeg::ffi::sws_freeContext(sws_ctx);
        }

        if let Ok(packet) = context.receive_packet() {
            let data = bincode::encode_to_vec(
                PacketData::from_av_packet(&packet),
                bincode::config::standard(),
            )
            .unwrap();
            self.packets.push_back(Packet { data })
        }

        if let Err(err) = context.send_frame(Some(&ff_frame)) {
            eprintln!("Send frame: {err}")
        }

        if let Ok(packet) = context.receive_packet() {
            let data = bincode::encode_to_vec(
                PacketData::from_av_packet(&packet),
                bincode::config::standard(),
            )
            .unwrap();
            self.packets.push_back(Packet { data })
        }

        Ok(())
    }

    fn get_packet(&mut self) -> Option<media_man_core::Packet> {
        if let Some(context) = &mut self.context {
            if let Ok(packet) = context.receive_packet() {
                let data = bincode::encode_to_vec(
                    PacketData::from_av_packet(&packet),
                    bincode::config::standard(),
                )
                .unwrap();
                self.packets.push_back(Packet { data })
            }
        }

        self.packets.pop_front()
    }
}

struct VideoDecoderSettings {}

impl TSettings for VideoDecoderSettings {
    fn get_settings_len(&self) -> usize {
        0
    }

    fn get_setting_info(&self, id: usize) -> Option<media_man_core::ValueInfo> {
        None
    }

    fn set_setting(
        &mut self,
        id: usize,
        value: media_man_core::Value,
    ) -> Result<(), SettingsError> {
        Err(SettingsError::InvalidSettingIndex)
    }

    fn get_setting(&self, id: usize) -> Result<media_man_core::Value, SettingsError> {
        Err(SettingsError::InvalidSettingIndex)
    }
}

struct VideoDecoder {
    settings: VideoDecoderSettings,
    context: rsmpeg::avcodec::AVCodecContext,
    frames: VecDeque<FrameVideo>,
}

unsafe impl Sync for VideoDecoder {}

impl TSettings for VideoDecoder {
    fn get_settings_len(&self) -> usize {
        0
    }

    fn get_setting_info(&self, id: usize) -> Option<media_man_core::ValueInfo> {
        None
    }

    fn set_setting(
        &mut self,
        id: usize,
        value: media_man_core::Value,
    ) -> Result<(), SettingsError> {
        Err(SettingsError::InvalidSettingIndex)
    }

    fn get_setting(&self, id: usize) -> Result<media_man_core::Value, SettingsError> {
        Err(SettingsError::InvalidSettingIndex)
    }
}

impl TDecoderVideo for VideoDecoder {
    fn decode(&mut self, packet: Packet) -> Result<(), DecodeError> {
        let (packet_data, _) =
            bincode::decode_from_slice::<PacketData, _>(&packet.data, bincode::config::standard())
                .unwrap();
        let av_packet = packet_data.to_av_packet();

        if let Err(err) = self.context.send_packet(Some(&av_packet)) {
            eprintln!("Cannot send packet: {err}");
        }

        if let Ok(frame) = self.context.receive_frame() {
            let sws_ctx = unsafe {
                rsmpeg::ffi::sws_getContext(
                    frame.width,
                    frame.height,
                    rsmpeg::ffi::AV_PIX_FMT_YUV444P,
                    frame.width,
                    frame.height,
                    rsmpeg::ffi::AV_PIX_FMT_RGBA,
                    0,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                )
            };

            let mut data = vec![0u8; (frame.width as usize) * (frame.height as usize) * 4];

            unsafe {
                rsmpeg::ffi::sws_scale(
                    sws_ctx,
                    &frame.data as *const _ as *const _,
                    &frame.linesize as *const _,
                    0,
                    frame.height,
                    &[data.as_mut_ptr()] as *const _,
                    &[frame.width * 4] as *const _,
                );

                rsmpeg::ffi::sws_freeContext(sws_ctx);
            }

            self.frames.push_back(FrameVideo {
                width: frame.width as u32,
                height: frame.height as u32,
                format: Format::RGBA,
                data: data.into(),
            })
        }

        Ok(())
    }

    fn get_frame(&mut self) -> Option<FrameVideo> {
        self.frames.pop_front()
    }
}

impl TCodecVideo for CodecVideoHEVC {
    fn name(&self) -> String {
        String::from("hevc")
    }

    fn description(&self) -> String {
        String::from("HVEC using ffmpeg x256")
    }

    fn default_encoder_settings(
        &self,
        format: Format,
    ) -> Result<Box<dyn TSettings>, SettingsError> {
        if format != Format::RGBA {
            return Err(SettingsError::NotSupportedFormat);
        };

        Ok(Box::new(VideoEncoderSettings {}))
    }

    fn create_encoder(
        &self,
        settings: Box<dyn TSettings>,
    ) -> Result<Box<dyn TEncoderVideo>, SettingsError> {
        let Ok(settings) = (settings as Box<dyn Any>).downcast::<VideoEncoderSettings>() else {
            return Err(SettingsError::InvalidSettingsType);
        };

        Ok(Box::new(VideoEncoder {
            settings: *settings,
            packets: VecDeque::default(),
            context: None,
        }))
    }

    fn default_decoder_settings(
        &self,
        format: Format,
    ) -> Result<Box<dyn TSettings>, SettingsError> {
        if format != Format::RGBA {
            return Err(SettingsError::NotSupportedFormat);
        };

        Ok(Box::new(VideoDecoderSettings {}))
    }

    fn create_decoder(
        &self,
        settings: Box<dyn TSettings>,
    ) -> Result<Box<dyn media_man_core::TDecoderVideo>, SettingsError> {
        let Ok(settings) = (settings as Box<dyn Any>).downcast::<VideoDecoderSettings>() else {
            return Err(SettingsError::InvalidSettingsType);
        };

        // let codec = rsmpeg::avcodec::AVCodec::find_decoder(rsmpeg::ffi::AV_CODEC_ID_HEVC).unwrap();
        let codec = rsmpeg::avcodec::AVCodec::find_decoder_by_name(c"hevc_cuvid").unwrap();
        eprintln!("Decoder Name: {:?}", codec.name());
        eprintln!("Decoder Long Name: {:?}", codec.long_name());

        let mut ctx = rsmpeg::avcodec::AVCodecContext::new(&codec);
        ctx.set_width(1920);
        ctx.set_height(1080);
        ctx.set_pix_fmt(rsmpeg::ffi::AV_PIX_FMT_YUV444P);

        ctx.open(None).unwrap();

        Ok(Box::new(VideoDecoder {
            settings: *settings,
            context: ctx,
            frames: VecDeque::default(),
        }))
    }
}

#[test]
fn testing() {
    let codec = CodecVideoHEVC;
    let encoder_settings = codec.default_encoder_settings(Format::RGBA).unwrap();
    let mut encoder = codec.create_encoder(encoder_settings).unwrap();

    let decoder_settings = codec.default_decoder_settings(Format::RGBA).unwrap();
    let mut decoder = codec.create_decoder(decoder_settings).unwrap();

    for _ in 0..60 {
        encoder
            .encode(&FrameVideo {
                width: 1920,
                height: 1080,
                format: Format::RGBA,
                data: vec![0x10u8; 1920 * 1080 * 4].into(),
            })
            .unwrap();
        eprintln!("Encode");

        if let Some(packet) = encoder.get_packet() {
            eprintln!("Encoded: {}", packet.data.len());

            decoder.decode(packet).unwrap();
            if let Some(frame) = decoder.get_frame() {
                eprintln!(
                    "Decoded: {}x{}: {}",
                    frame.width,
                    frame.height,
                    frame.data.len()
                );
            }
        }
    }

    panic!()
}
