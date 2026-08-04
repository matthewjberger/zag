#[derive(Clone, Copy)]
pub enum PacketInitError {
    OutOfMemory,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Header {
    pub magic: u32,
    pub version: u16,
    pub flags: u16,
    pub length: u32,
}

const _: () = assert!(core::mem::size_of::<Header>() == 12);
const _: () = assert!(core::mem::align_of::<Header>() == 4);
const _: () = assert!(core::mem::offset_of!(Header, magic) == 0);
const _: () = assert!(core::mem::offset_of!(Header, version) == 4);
const _: () = assert!(core::mem::offset_of!(Header, flags) == 6);
const _: () = assert!(core::mem::offset_of!(Header, length) == 8);

#[derive(Clone)]
pub struct Packet {
    pub header: Header,
    pub payload: Box<[u8]>,
}

impl Packet {
    pub fn new(version: u16, body: &[u8]) -> Self {
        Self {
            header: Header {
                magic: 0x5A414750,
                version,
                flags: 0,
                length: u32::try_from(body.len()).unwrap(),
            },
            payload: Box::from(body),
        }
    }
}

pub fn main() -> Result<(), PacketInitError> {
    todo!()
}
