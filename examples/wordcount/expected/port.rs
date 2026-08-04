#[derive(Clone, Copy)]
pub enum CountsInitError {
    OutOfMemory,
}

#[derive(Clone)]
pub struct Counts {
    pub text: Box<[u8]>,
    pub name: Box<[u8]>,
    pub words: u32,
    pub lines: u32,
}

impl Counts {
    pub fn init(name: &[u8], input: &[u8]) -> Result<Counts, CountsInitError> {
        let _ = name;
        let _ = input;
        todo!()
    }
}

pub fn main() -> Result<(), CountsInitError> {
    todo!()
}
