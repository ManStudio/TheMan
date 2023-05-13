use the_man::Atom;

pub mod opus;

pub trait Codec {
    fn name(&self) -> &str;

    fn settings(&self) -> Vec<String>;
    fn get_setting(&self, key: String) -> Option<Atom>;
    fn set_setting(&mut self, key: String, value: Atom);

    fn errors(&mut self) -> Vec<String>;

    fn encode(&mut self, data: Vec<u8>) -> Vec<u8>;
    fn decode(&mut self, data: Vec<u8>) -> Vec<u8>;

    fn c(&self) -> Box<dyn Codec>;
}
