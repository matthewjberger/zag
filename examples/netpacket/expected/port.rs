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

pub struct Packet {
    pub header: Header,
    pub payload: Box<[u8]>,
}
