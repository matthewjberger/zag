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

use crate::Output;
use zag_facts::tables::{Tables, string_bytes};

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
    Port { files, crates }
}
