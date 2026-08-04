#[derive(Clone, Copy)]
pub enum MakeCacheError {
    OutOfMemory,
}

#[derive(Clone, Copy)]
pub struct Cache {
    pub entries: Option<core::ptr::NonNull<[u8]>>,
}

pub fn make_cache(bytes: &[u8]) -> Result<Cache, MakeCacheError> {
    let _ = bytes;
    todo!()
}

pub fn from_heap(bytes: &[u8]) -> Result<Cache, MakeCacheError> {
    make_cache(bytes)
}

pub fn from_arena(bytes: &[u8]) -> Result<Cache, MakeCacheError> {
    make_cache(bytes)
}

pub fn main() -> Result<(), MakeCacheError> {
    todo!()
}
