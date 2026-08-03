pub struct Token<'bump> {
    pub text: &'bump [u8],
    pub length: u32,
}

pub struct Document<'a, 'bump> {
    pub source: &'a [u8],
    pub tokens: &'bump [Token<'bump>],
    pub separators: &'static [u8],
}
