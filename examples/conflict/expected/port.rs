#[derive(Clone, Copy)]
pub struct Cache {
    pub entries: Option<core::ptr::NonNull<[u8]>>,
}
