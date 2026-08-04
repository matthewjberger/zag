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

    pub fn largest(entries: &[super::entry::Entry]) -> u32 {
        let mut highest = 0;
        for item in entries {
            highest = highest.max(item.amount);
        }
        highest
    }

    pub fn combined(first: &super::entry::Entry, second: &super::entry::Entry) -> u32 {
        total(first, second) + 1
    }
}
