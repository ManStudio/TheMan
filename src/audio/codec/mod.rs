use the_man::Atom;

pub trait Codec: Sync + Send {
    fn name(&self) -> &str;

    fn settings(&self) -> Vec<String>;
    fn get_setting(&mut self, key: String) -> Option<Atom>;
    fn set_setting(&mut self, key: String, value: Atom);

    fn errors(&mut self) -> Vec<String>;

    fn encode(&mut self, data: &mut Vec<f32>) -> Vec<u8>;
    fn decode(&mut self, data: &mut Vec<u8>) -> Vec<f32>;

    fn c(&self) -> Box<dyn Codec>;
}
