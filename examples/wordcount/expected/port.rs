#[derive(Clone)]
pub struct Counts {
    pub text: Box<[u8]>,
    pub name: Box<[u8]>,
    pub words: u32,
    pub lines: u32,
}
