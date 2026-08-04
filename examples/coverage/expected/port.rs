#[derive(Clone, Copy)]
pub enum BufferInitError {
    OutOfMemory,
}

#[derive(Clone)]
pub struct Buffer {
    pub data: Box<[u8]>,
    pub length: u32,
}

impl Buffer {
    pub fn new(bytes: &[u8]) -> Self {
        Self {
            data: Box::from(bytes),
            length: u32::try_from(bytes.len()).unwrap(),
        }
    }
}

#[derive(Clone, Copy)]
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

#[derive(Clone, Copy)]
pub struct Node<'bump> {
    pub label: &'bump [u8],
    pub children: &'static [u32],
}

#[derive(Clone, Copy)]
pub struct View<'a> {
    pub bytes: &'a [u8],
}

#[derive(Clone, Copy)]
pub struct Cache {
    pub entries: Option<core::ptr::NonNull<[u8]>>,
}

pub fn make_buffer(bytes: &[u8]) -> Result<Buffer, BufferInitError> {
    let _ = bytes;
    todo!()
}

pub fn make_view<'a>(bytes: &'a [u8]) -> View<'a> {
    View {
        bytes,
    }
}

pub fn main() -> Result<(), BufferInitError> {
    todo!()
}
