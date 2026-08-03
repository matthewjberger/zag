pub mod entry {
    pub struct Entry {
        pub label: Box<[u8]>,
        pub amount: u32,
    }
}

pub mod store {
    pub fn total(first: &super::entry::Entry, second: &super::entry::Entry) -> u32 {
        first.amount + second.amount
    }
}
