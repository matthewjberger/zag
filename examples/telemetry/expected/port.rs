#[derive(Clone)]
pub struct Frame {
    pub channels: [u32; 4],
    pub label: Option<&'static [u8]>,
    pub source: Box<[u8]>,
}

impl Frame {
    pub fn new(channels: [u32; 4], source: &[u8]) -> Self {
        Self {
            channels,
            label: None,
            source: Box::from(source),
        }
    }
}
