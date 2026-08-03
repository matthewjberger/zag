//! One Zig file, as the crawl found it.
//!
//! A Zig file is a struct, so `store.Entry` is a field access on the struct the
//! `store` import gave back. `imports` is what makes that readable as a type in
//! another module rather than as an opaque name.

use crate::program::Program;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceModule {
    /// What the module is called in the port. Empty for the root, whose
    /// declarations sit at the top level the way the root file's do in Zig.
    pub name: String,
    pub path: String,
    pub program: Program,
    /// The name each import is bound to, paired with the module it reached.
    pub imports: Vec<(String, String)>,
    /// Imports that reached nothing. Kept so the report can say the program
    /// was read with a hole in it.
    pub unresolved: Vec<String>,
}

/// One file, wrapped as the whole program. What `build` does underneath.
pub fn single(program: Program) -> Vec<SourceModule> {
    vec![SourceModule {
        program,
        ..SourceModule::default()
    }]
}
