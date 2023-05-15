use std::collections::HashMap;

use bytes_kman::TBytes;
use the_man::Atom;

use super::Codec;

pub struct CodecOpus {
    pub settings: HashMap<String, Atom>,
    errors: Vec<String>,
}

impl Default for CodecOpus {
    fn default() -> Self {
        Self {
            settings: HashMap::new(),
            errors: Vec::new(),
        }
    }
}

impl Codec for CodecOpus {
    fn name(&self) -> &str {
        "Opus"
    }

    fn settings(&self) -> Vec<String> {
        self.settings.keys().cloned().collect::<Vec<String>>()
    }

    fn get_setting(&self, key: String) -> Option<the_man::Atom> {
        self.settings.get(&key).cloned()
    }

    fn set_setting(&mut self, key: String, value: the_man::Atom) {
        self.settings.insert(key, value);
    }

    fn errors(&mut self) -> Vec<String> {
        self.errors.drain(..).collect::<Vec<String>>()
    }

    fn encode(&self, data: Vec<f32>) -> Vec<u8> {
        data.to_bytes()
    }

    fn decode(&self, data: &mut dyn Iterator<Item = u8>) -> Vec<f32> {
        Vec::<f32>::from_bytes(data).unwrap_or_default()
    }

    fn c(&self) -> Box<dyn Codec> {
        Box::<Self>::default()
    }
}
