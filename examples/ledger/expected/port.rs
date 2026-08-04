#[derive(Clone, Copy)]
pub enum MainError {
    OutOfMemory,
}

pub fn main() -> Result<(), MainError> {
    todo!()
}

pub mod entry {
    #[derive(Clone)]
    pub struct Entry {
        pub label: Box<[u8]>,
        pub amount: u32,
    }
}

pub mod store {
    pub fn open(label: &[u8], amount: u32) -> Result<super::entry::Entry, super::MainError> {
        let _ = label;
        let _ = amount;
        todo!()
    }

    pub fn total(first: &super::entry::Entry, second: &super::entry::Entry) -> u32 {
        first.amount + second.amount
    }

    pub fn largest(entries: &[super::entry::Entry]) -> u32 {
        let mut highest: u32 = 0;
        for item in entries.iter() {
            highest = highest.max(item.amount);
        }
        highest
    }

    pub fn combined(first: &super::entry::Entry, second: &super::entry::Entry) -> u32 {
        total(first, second) + 1
    }
}
