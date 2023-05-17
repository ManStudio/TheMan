pub mod network;

#[derive(Debug, Clone)]
pub enum Atom {
    Signed {
        value: isize,
        range: std::ops::Range<isize>,
    },
    UnSigned {
        value: usize,
        range: std::ops::Range<usize>,
    },
    Float {
        value: f64,
        range: std::ops::Range<f64>,
    },
    Text(String),
}

impl Atom {
    pub fn valid(&self) -> bool {
        match self {
            Atom::Signed { value, range } => range.contains(value),
            Atom::UnSigned { value, range } => range.contains(value),
            Atom::Float { value, range } => range.contains(value),
            Atom::Text(_) => true,
        }
    }
}
