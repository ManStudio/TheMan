#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SampleFormat {
    I16,
    F32,
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
}

impl std::ops::Add<ValueInfo> for ValueInfo {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        let Self {
            name,
            description,
            ty,
            default,
            mut options,
        } = self;

        options.push(rhs);

        Self {
            name,
            description,
            ty,
            default,
            options,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SettingsError {
    NotSupportedSampleFormat,
    NotSupportedChannels,
    NotSupportedSampleRate,
    InvalidSettingIndex,
    InvalidValue,
    InvalidSettingsType,
}

pub trait TSettings: std::any::Any + Sync + Send {
    fn get_settings_len(&self) -> usize;
    fn get_setting_info(&self, idx: usize) -> Option<ValueInfo>;
    fn set_setting(&mut self, idx: usize, value: Value) -> Result<(), SettingsError>;
    fn get_setting(&mut self, idx: usize) -> Result<Value, SettingsError>;
}

#[derive(Debug, Clone)]
pub struct FrameAudio {
    pub sample_format: SampleFormat,
    pub data: Vec<u8>,
}

impl FrameAudio {
    pub fn f32_new(data: &[f32]) -> FrameAudio {
        FrameAudio {
            sample_format: SampleFormat::F32,
            data: unsafe {
                std::slice::from_raw_parts(data.as_ptr() as *const u8, std::mem::size_of_val(data))
            }
            .to_vec(),
        }
    }
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
