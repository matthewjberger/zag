//! A Zig project, as the crawl found it: the files it is made of, and what the
//! build script says it produces.
//!
//! A Zig file is a struct, so `store.Entry` is a field access on the struct the
//! `store` import gave back. `imports` is what makes that readable as a type in
//! another module rather than as an opaque name.

use crate::program::Program;
use zag_facts::tables::ArtifactKind;

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

/// One thing the build script asks the compiler to produce.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Artifact {
    pub name: String,
    /// The module it is rooted at, by name. Nothing where the build script
    /// named a file the crawl could not open, which is a hole in the build
    /// graph the report has to say out loud.
    pub root: Option<String>,
    pub kind: ArtifactKind,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Project {
    pub modules: Vec<SourceModule>,
    /// Empty for a project read from a root file, because the file it was
    /// pointed at is the whole answer.
    pub artifacts: Vec<Artifact>,
}

/// One file, wrapped as the whole project. What `build` does underneath.
pub fn single(program: Program) -> Project {
    Project {
        modules: vec![SourceModule {
            program,
            ..SourceModule::default()
        }],
        artifacts: Vec::new(),
    }
}
