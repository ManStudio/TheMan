pub mod network;

#[derive(Debug, Clone)]
pub enum Atom {
    SignedValues {
        value: isize,
        values: Vec<isize>,
    },
    UnSignedValues {
        value: usize,
        values: Vec<usize>,
    },
    StringValues {
        value: String,
        values: Vec<String>,
    },
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
            Atom::SignedValues { value, values } => values.contains(value),
            Atom::UnSignedValues { value, values } => values.contains(value),
            Atom::StringValues { value, values } => values.contains(value),
        }
    }
}
