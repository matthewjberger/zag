pub mod document {
    pub struct Entry {
        pub section: Option<core::ptr::NonNull<[u8]>>,
        pub key: Option<core::ptr::NonNull<[u8]>>,
        pub value: Option<core::ptr::NonNull<[u8]>>,
    }

    pub struct Document {
        pub text: Box<[u8]>,
        pub entries: Option<core::ptr::NonNull<[Entry]>>,
    }

    impl Document {
        pub fn deinit(&mut self) {
            todo!()
        }
        pub fn lookup(&self, section: &[u8], key: &[u8]) -> Option<&[u8]> {
            let _ = section;
            let _ = key;
            todo!()
        }
        pub fn count(&self) -> u32 {
            todo!()
        }
        pub fn section_size(&self, section: &[u8]) -> u32 {
            let _ = section;
            todo!()
        }
    }

    pub enum Error {
        MissingEquals,
        UnterminatedSection,
        EmptyKey,
    }
}

pub mod parser {
    pub fn trim(text: &[u8]) -> &[u8] {
        let _ = text;
        todo!()
    }

    pub fn is_comment(line: &[u8]) -> bool {
        let _ = line;
        todo!()
    }

    pub fn section_name(line: &[u8]) -> Result<&[u8], super::document::Error> {
        let _ = line;
        todo!()
    }
}
