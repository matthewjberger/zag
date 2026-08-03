pub struct Buffer {
    pub data: Box<[u8]>,
    pub length: u32,
}

#[repr(C)]
pub struct Header {
    pub magic: u32,
    pub version: u16,
    pub flags: u16,
}

const _: () = assert!(core::mem::size_of::<Header>() == 8);
const _: () = assert!(core::mem::align_of::<Header>() == 4);
const _: () = assert!(core::mem::offset_of!(Header, magic) == 0);
const _: () = assert!(core::mem::offset_of!(Header, version) == 4);
const _: () = assert!(core::mem::offset_of!(Header, flags) == 6);

pub struct Node<'bump> {
    pub label: &'bump [u8],
    pub children: &'static [u32],
}

pub struct View<'a> {
    pub bytes: &'a [u8],
}

pub struct Cache {
    pub entries: Option<core::ptr::NonNull<[u8]>>,
}
