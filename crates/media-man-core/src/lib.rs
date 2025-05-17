use std::{collections::BTreeMap, fmt::Display, sync::Arc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SampleFormat {
    I16,
    F32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Format {
    RGBA,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    I16,
    I32,
    F32,
    U32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    I16(i16),
    F32(f32),
    I32(i32),
    U32(u32),
}

impl Value {
    pub fn ty(&self) -> Type {
        match self {
            Value::I16(_) => Type::I16,
            Value::F32(_) => Type::F32,
            Value::I32(_) => Type::I32,
            Value::U32(_) => Type::U32,
        }
    }
}

impl Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::I16(v) => Display::fmt(v, f),
            Value::F32(v) => Display::fmt(v, f),
            Value::I32(v) => Display::fmt(v, f),
            Value::U32(v) => Display::fmt(v, f),
        }
    }
}

impl From<i16> for Value {
    fn from(value: i16) -> Self {
        Self::I16(value)
    }
}

impl From<f32> for Value {
    fn from(value: f32) -> Self {
        Self::F32(value)
    }
}

impl From<i32> for Value {
    fn from(value: i32) -> Self {
        Self::I32(value)
    }
}

impl From<u32> for Value {
    fn from(value: u32) -> Self {
        Self::U32(value)
    }
}

impl TryFrom<Value> for i16 {
    type Error = Value;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        if let Value::I16(value) = value {
            Ok(value)
        } else {
            Err(value)
        }
    }
}

impl TryFrom<Value> for f32 {
    type Error = Value;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        if let Value::F32(value) = value {
            Ok(value)
        } else {
            Err(value)
        }
    }
}

impl TryFrom<Value> for i32 {
    type Error = Value;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        if let Value::I32(value) = value {
            Ok(value)
        } else {
            Err(value)
        }
    }
}

impl TryFrom<Value> for u32 {
    type Error = Value;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        if let Value::U32(value) = value {
            Ok(value)
        } else {
            Err(value)
        }
    }
}

#[derive(Debug)]
pub struct ValueInfo {
    pub name: String,
    pub description: String,
    pub ty: Type,
    pub default: Option<Value>,
    pub options: Vec<ValueInfo>,
}

impl ValueInfo {
    pub fn new(name: impl Into<String>, description: impl Into<String>, ty: Type) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            ty,
            default: None,
            options: vec![],
        }
    }

    pub fn new_with(
        name: impl Into<String>,
        description: impl Into<String>,
        value: impl Into<Value>,
    ) -> Self {
        let value = value.into();
        Self {
            name: name.into(),
            description: description.into(),
            ty: value.ty(),
            default: Some(value),
            options: vec![],
        }
    }

    pub fn valid(&self, value: Value) -> bool {
        if value.ty() != self.ty {
            return false;
        }

        if self.options.is_empty() {
            return true;
        }

        for option in self.options.iter() {
            if option.default == Some(value) {
                return true;
            }
        }

        false
    }

    pub fn value_from_str(&self, str: &str) -> Option<Value> {
        match self.ty {
            Type::I16 => {
                if let Ok(v) = str.parse::<i16>() {
                    return Some(Value::from(v));
                }
            }
            Type::I32 => {
                if let Ok(v) = str.parse::<i32>() {
                    return Some(Value::from(v));
                }
            }
            Type::F32 => {
                if let Ok(v) = str.parse::<f32>() {
                    return Some(Value::from(v));
                }
            }
            Type::U32 => {
                if let Ok(v) = str.parse::<u32>() {
                    return Some(Value::from(v));
                }
            }
        }

        for option in self.options.iter() {
            if let Some(value) = option.value_from_str(str) {
                return Some(value);
            }
        }

        None
    }
}

impl std::ops::Add<ValueInfo> for ValueInfo {
    type Output = Self;

    fn add(mut self, rhs: Self) -> Self {
        self.options.push(rhs);
        self
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SettingsError {
    NotSupportedSampleFormat,
    NotSupportedChannels,
    NotSupportedSampleRate,
    NotSupportedFormat,
    InvalidSettingIndex,
    InvalidValue,
    InvalidSettingsType,
}

pub trait TSettings: std::any::Any + Sync + Send {
    fn get_settings_len(&self) -> usize;
    fn get_setting_info(&self, id: usize) -> Option<ValueInfo>;
    fn set_setting(&mut self, id: usize, value: Value) -> Result<(), SettingsError>;
    fn get_setting(&self, id: usize) -> Result<Value, SettingsError>;
}

pub trait TSettingsExt {
    fn set(&mut self, name: &str, value: Value) -> Result<(), SettingsError>;
    fn get(&self, name: &str) -> Result<Value, SettingsError>;

    fn export(&self) -> BTreeMap<String, String>;
    fn import(&mut self, map: BTreeMap<String, String>);
}

impl TSettingsExt for Box<dyn TSettings> {
    fn set(&mut self, name: &str, value: Value) -> Result<(), SettingsError> {
        for id in 0..self.get_settings_len() {
            let Some(info) = self.get_setting_info(id) else {
                continue;
            };

            if info.name == name {
                return self.set_setting(id, value);
            }
        }

        Err(SettingsError::InvalidSettingIndex)
    }

    fn get(&self, name: &str) -> Result<Value, SettingsError> {
        for id in 0..self.get_settings_len() {
            let Some(info) = self.get_setting_info(id) else {
                continue;
            };

            if info.name == name {
                return self.get_setting(id);
            }
        }

        Err(SettingsError::InvalidSettingIndex)
    }

    fn export(&self) -> BTreeMap<String, String> {
        let mut out = BTreeMap::default();

        for id in 0..self.get_settings_len() {
            let Some(info) = self.get_setting_info(id) else {
                continue;
            };

            let Ok(value) = self.get_setting(id) else {
                continue;
            };

            out.insert(info.name, value.to_string());
        }

        out
    }

    fn import(&mut self, map: BTreeMap<String, String>) {
        for id in 0..self.get_settings_len() {
            let Some(info) = self.get_setting_info(id) else {
                continue;
            };

            let Some(string_value) = map.get(&info.name) else {
                continue;
            };

            if let Some(value) = info.value_from_str(string_value) {
                _ = self.set_setting(id, value);
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct FrameAudio {
    pub sample_format: SampleFormat,
    pub data: Vec<u8>,
}

impl FrameAudio {
    #[allow(clippy::manual_slice_size_calculation)]
    pub fn f32_new(data: Vec<f32>) -> FrameAudio {
        FrameAudio {
            sample_format: SampleFormat::F32,
            data: unsafe {
                let capacity = data.capacity();
                let data = data.leak();
                Vec::from_raw_parts(
                    data.as_mut_ptr() as *mut u8,
                    data.len() * size_of::<f32>(),
                    capacity * size_of::<f32>(),
                )
            },
        }
    }

    pub fn to_f32(self) -> Vec<f32> {
        assert_eq!(self.sample_format, SampleFormat::F32);

        unsafe {
            let capacity = self.data.capacity();
            let data = self.data.leak();
            Vec::from_raw_parts(
                data.as_mut_ptr() as *mut f32,
                data.len() / size_of::<f32>(),
                capacity / size_of::<f32>(),
            )
        }
    }

    pub fn as_f32(&self) -> &[f32] {
        assert_eq!(self.sample_format, SampleFormat::F32);

        unsafe {
            std::slice::from_raw_parts(
                std::mem::transmute::<*const u8, *const f32>(self.data.as_ptr()),
                self.data.len() / size_of::<f32>(),
            )
        }
    }

    pub fn as_f32_mut(&mut self) -> &mut [f32] {
        assert_eq!(self.sample_format, SampleFormat::F32);

        unsafe {
            std::slice::from_raw_parts_mut(
                std::mem::transmute::<*mut u8, *mut f32>(self.data.as_mut_ptr()),
                self.data.len() / size_of::<f32>(),
            )
        }
    }

    pub fn to_i16(self) -> Vec<i16> {
        assert_eq!(self.sample_format, SampleFormat::I16);

        unsafe {
            let capacity = self.data.capacity();
            let data = self.data.leak();
            Vec::from_raw_parts(
                data.as_mut_ptr() as *mut i16,
                data.len() / size_of::<i16>(),
                capacity / size_of::<i16>(),
            )
        }
    }

    pub fn as_i16(&self) -> &[i16] {
        assert_eq!(self.sample_format, SampleFormat::I16);

        unsafe {
            std::slice::from_raw_parts(
                std::mem::transmute::<*const u8, *const i16>(self.data.as_ptr()),
                self.data.len() / size_of::<i16>(),
            )
        }
    }

    pub fn as_i16_mut(&mut self) -> &mut [i16] {
        assert_eq!(self.sample_format, SampleFormat::I16);

        unsafe {
            std::slice::from_raw_parts_mut(
                std::mem::transmute::<*mut u8, *mut i16>(self.data.as_mut_ptr()),
                self.data.len() / size_of::<i16>(),
            )
        }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct FrameVideo {
    pub width: u32,
    pub height: u32,
    pub format: Format,
    pub data: Arc<[u8]>,
}

#[derive(Debug, Clone)]
pub struct Packet {
    pub data: Vec<u8>,
}

impl Packet {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }
}

#[derive(Debug, Clone)]
pub enum EncodeError {
    InvalidFrame(u8),
    FramesDontHaveTheSameSize,
    InvalidNumberOfFrames,
    InvalidSampleFormatFor(u8),
    Custom(String),
}

#[derive(Debug, Clone)]
pub enum DecodeError {
    InvalidPacket,
    Custom(String),
}

pub trait TEncoderAudio: TSettings {
    fn encode(&mut self, frames: &[&FrameAudio]) -> Result<(), EncodeError>;
    fn get_packet(&mut self) -> Option<Packet>;
}

pub trait TDecoderAudio: TSettings {
    fn decode(&mut self, packet: Packet) -> Result<Vec<FrameAudio>, DecodeError>;
}

pub trait TCodecAudio: Send + Sync {
    fn name(&self) -> String;
    fn description(&self) -> String;

    fn default_encoder_settings(
        &self,
        sample_format: SampleFormat,
        sample_rate: u32,
        channels: u8,
    ) -> Result<Box<dyn TSettings>, SettingsError>;
    fn create_encoder(
        &self,
        settings: Box<dyn TSettings>,
    ) -> Result<Box<dyn TEncoderAudio>, SettingsError>;

    fn default_decoder_settings(
        &self,
        sample_format: SampleFormat,
        sample_rate: u32,
        channels: u8,
    ) -> Result<Box<dyn TSettings>, SettingsError>;
    fn create_decoder(
        &self,
        settings: Box<dyn TSettings>,
    ) -> Result<Box<dyn TDecoderAudio>, SettingsError>;
}

pub trait TEncoderVideo: TSettings {
    fn encode(&mut self, frame: &FrameVideo) -> Result<(), EncodeError>;
    fn get_packet(&mut self) -> Option<Packet>;
}

pub trait TDecoderVideo: TSettings {
    fn decode(&mut self, packet: Packet) -> Result<FrameVideo, DecodeError>;
}

pub trait TCodecVideo: Send + Sync {
    fn name(&self) -> String;
    fn description(&self) -> String;

    fn default_encoder_settings(&self, format: Format)
        -> Result<Box<dyn TSettings>, SettingsError>;
    fn create_encoder(
        &self,
        settings: Box<dyn TSettings>,
    ) -> Result<Box<dyn TEncoderVideo>, SettingsError>;

    fn default_decoder_settings(&self, format: Format)
        -> Result<Box<dyn TSettings>, SettingsError>;
    fn create_decoder(
        &self,
        settings: Box<dyn TSettings>,
    ) -> Result<Box<dyn TDecoderVideo>, SettingsError>;
}
