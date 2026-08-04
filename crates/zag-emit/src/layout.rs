//! Lays the port out as a crate rather than as one file.
//!
//! A Zig file is a Rust module and a Zig package is a Rust crate, so a program
//! of several files is one crate of several modules, and a program that
//! depends on a package the crawl could not read needs a crate for it too.
//! Those are the ones the report calls unresolved imports: named rather than
//! path imports, which is what a package looks like from inside the source.
//!
//! Where more than one crate is needed the top level is a workspace, because
//! that is what Cargo calls a set of crates that build together.
//!
//! Several Zig executables over one set of files are not several crates. They
//! share every module they import, which is one Cargo package with a binary
//! each, and a workspace would mean copying the shared modules into every one
//! of them or inventing a crate the Zig never had.

use crate::Output;
use zag_facts::tables::{ArtifactKind, Tables, artifact_count, string_bytes};

/// One file the port is made of, at a path relative to wherever it is written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct File {
    pub path: String,
    pub contents: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Port {
    pub files: Vec<File>,
    /// The crates the port is made of, in the order they are written. More
    /// than one means the top level is a workspace.
    pub crates: Vec<String>,
    /// The binaries the port builds, one per executable the build script asked
    /// for, in the order they are written.
    pub binaries: Vec<String>,
}

fn text(contents: &str) -> Vec<u8> {
    contents.as_bytes().to_vec()
}

/// A crate name Cargo accepts, from whatever the Zig called it.
fn crate_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|letter| {
            if letter.is_ascii_alphanumeric() {
                letter.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() || cleaned.starts_with(|letter: char| letter.is_ascii_digit()) {
        return format!("crate_{cleaned}");
    }
    cleaned
}

/// The packages the program imports that the crawl could not read. Each one is
/// a crate the port needs and does not have, which is a fact the reader has to
/// act on rather than discover from a compile error.
fn missing_packages(tables: &Tables) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for row in 0..tables.unresolved_imports.owner.len() {
        let Some(name) = tables.unresolved_imports.name.get(row) else {
            continue;
        };
        let name = String::from_utf8_lossy(string_bytes(&tables.strings, *name)).into_owned();
        // A path import that failed is a missing file, not a missing package.
        if name.ends_with(".zig") {
            continue;
        }
        let name = crate_name(&name);
        if !found.contains(&name) {
            found.push(name);
        }
    }
    found.sort();
    found
}

fn manifest(name: &str, dependencies: &[String], standalone: bool) -> String {
    let mut out = String::new();
    if standalone {
        // Without this a crate written inside another crate's directory is
        // taken for one of its members, which is not what a port is.
        out.push_str("[workspace]\n\n");
    }
    out.push_str(&format!(
        "[package]\n\
         name = \"{name}\"\n\
         version = \"0.1.0\"\n\
         edition = \"2024\"\n\n"
    ));
    if !dependencies.is_empty() {
        out.push_str("[dependencies]\n");
        for dependency in dependencies {
            out.push_str(&format!(
                "{dependency} = {{ path = \"../{dependency}\" }}\n"
            ));
        }
        out.push('\n');
    }
    // The port is index based and has nothing to gain from raw pointers, the
    // same reason the tool that wrote it forbids them.
    out.push_str("[lints.rust]\nunsafe_code = \"forbid\"\n");
    out
}

/// Splits the rendered source back into one file per module.
///
/// The renderer writes the whole program with each module inside a `pub mod`,
/// which is the form that compiles as one file. A crate says the same thing
/// with a file per module and a declaration in the root, so the braces come
/// off and one level of indentation with them.
fn split_modules(source: &str) -> (String, Vec<(String, String)>) {
    let mut root = String::new();
    let mut modules: Vec<(String, String)> = Vec::new();
    let mut lines = source.lines().peekable();
    while let Some(line) = lines.next() {
        let Some(name) = line
            .strip_prefix("pub mod ")
            .and_then(|rest| rest.strip_suffix(" {"))
        else {
            root.push_str(line);
            root.push('\n');
            continue;
        };
        let mut body = String::new();
        for inner in lines.by_ref() {
            if inner == "}" {
                break;
            }
            body.push_str(inner.strip_prefix("    ").unwrap_or(inner));
            body.push('\n');
        }
        modules.push((name.to_string(), body));
    }
    (root, modules)
}

/// The executables the build script asked for, paired with the module each one
/// is rooted at. An artifact whose root the crawl could not open keeps its row
/// with no module, because a binary the port owes and cannot write is a fact
/// the reader has to act on.
fn executables(tables: &Tables) -> Vec<(String, Option<String>)> {
    let mut found: Vec<(String, Option<String>)> = Vec::new();
    for row in 0..artifact_count(&tables.artifacts) {
        if tables.artifacts.kind.get(row) != Some(&ArtifactKind::Executable) {
            continue;
        }
        let Some(name) = tables.artifacts.name.get(row) else {
            continue;
        };
        let name = crate_name(&String::from_utf8_lossy(string_bytes(
            &tables.strings,
            *name,
        )));
        if found.iter().any(|(taken, _)| *taken == name) {
            continue;
        }
        let module = tables
            .artifacts
            .root
            .get(row)
            .and_then(|root| tables.modules.name.get(root.0 as usize))
            .map(|name| String::from_utf8_lossy(string_bytes(&tables.strings, *name)).into_owned())
            .filter(|name| !name.is_empty());
        found.push((name, module));
    }
    found
}

/// A binary that runs what the Zig root of the artifact ran.
///
/// The port keeps a Zig `main` as an ordinary function, so the binary is a call
/// to it. `let _ =` rather than a bare call, because a `main` that could fail
/// in Zig comes across returning a `Result` and its value is not this stub's to
/// decide about. Where the module has no `main` the port could write, the
/// binary says so and fails when run rather than at compile time, which is the
/// same bargain the unwritten function bodies take.
fn binary(package: &str, artifact: &str, module: Option<&str>, body: Option<&str>) -> String {
    let Some(module) = module else {
        return format!(
            "//! `{artifact}` in the Zig build graph. The crawl could not open the file it\n\
             //! is rooted at, so there is nothing here to call.\n\
             \n\
             fn main() {{\n    todo!(\"{artifact} has no ported root\")\n}}\n"
        );
    };
    let declares_main = body.is_some_and(|body| {
        body.lines()
            .any(|line| line.trim_start().starts_with("pub fn main("))
    });
    if !declares_main {
        return format!(
            "//! `{artifact}` in the Zig build graph, rooted at the `{module}` module,\n\
             //! which ported no `main` for this to call.\n\
             \n\
             fn main() {{\n    todo!(\"the {artifact} root ported no main\")\n}}\n"
        );
    }
    format!(
        "//! `{artifact}` in the Zig build graph, rooted at the `{module}` module.\n\
         \n\
         fn main() {{\n    let _ = {package}::{module}::main();\n}}\n"
    )
}

/// The port as a set of files. One crate where the program needs one, and a
/// workspace where it names packages the crawl could not read.
pub fn lay_out(tables: &Tables, output: &Output, name: &str) -> Port {
    let name = crate_name(name);
    let missing = missing_packages(tables);
    let source = String::from_utf8_lossy(&output.source).into_owned();
    let (root, modules) = split_modules(&source);

    let mut files = Vec::new();
    let inside = |path: &str| {
        if missing.is_empty() {
            path.to_string()
        } else {
            format!("{name}/{path}")
        }
    };

    let mut lib = String::new();
    for (module, _) in &modules {
        lib.push_str(&format!("pub mod {module};\n"));
    }
    if !modules.is_empty() && !root.trim().is_empty() {
        lib.push('\n');
    }
    lib.push_str(root.trim_start_matches('\n'));
    files.push(File {
        path: inside("src/lib.rs"),
        contents: text(&lib),
    });
    for (module, body) in &modules {
        files.push(File {
            path: inside(&format!("src/{module}.rs")),
            contents: text(body),
        });
    }
    let mut binaries = Vec::new();
    for (artifact, module) in executables(tables) {
        let body = module.as_ref().and_then(|wanted| {
            modules
                .iter()
                .find(|(name, _)| name == wanted)
                .map(|(_, body)| body.as_str())
        });
        files.push(File {
            path: inside(&format!("src/bin/{artifact}.rs")),
            contents: text(&binary(&name, &artifact, module.as_deref(), body)),
        });
        binaries.push(artifact);
    }
    files.push(File {
        path: inside("Cargo.toml"),
        contents: text(&manifest(&name, &missing, missing.is_empty())),
    });

    let mut crates = vec![name.clone()];
    for package in &missing {
        crates.push(package.clone());
        files.push(File {
            path: format!("{package}/Cargo.toml"),
            contents: text(&manifest(package, &[], false)),
        });
        files.push(File {
            path: format!("{package}/src/lib.rs"),
            contents: text(&format!(
                "//! The Zig imported `{package}` and the crawl could not read it, so this\n\
                 //! is the crate the port needs and does not have. Fill it in, or point\n\
                 //! Cargo at the real one and delete this.\n"
            )),
        });
    }

    if crates.len() > 1 {
        let members: Vec<String> = crates
            .iter()
            .map(|name| format!("    \"{name}\",\n"))
            .collect();
        files.push(File {
            path: "Cargo.toml".to_string(),
            contents: text(&format!(
                "[workspace]\nresolver = \"3\"\nmembers = [\n{}]\n",
                members.concat()
            )),
        });
    }

    files.sort_by(|left, right| left.path.cmp(&right.path));
    Port {
        files,
        crates,
        binaries,
    }
}
