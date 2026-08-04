//! Whole Zig projects, read through their build scripts and ported end to end.
//!
//! The `examples/` layer holds the frontend to the compiler on programs small
//! enough to carry hand-built fact tables. This layer trades that for size.
//! Nothing here is hand-built: each project is read with the compiler, ported,
//! and compared against the output checked in beside it, so a change to any
//! pass arrives as a diff somebody has to read.
//!
//! These are ordinary programs rather than programs shaped to suit the
//! analysis, which is the point. The port of each one is handed to rustc,
//! because a transpiler whose output does not compile has not finished.

use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the crate lives two directories under the workspace root")
        .to_path_buf()
}

fn names() -> Vec<String> {
    let directory = workspace_root().join("corpus");
    let mut found: Vec<String> = std::fs::read_dir(&directory)
        .unwrap_or_else(|cause| panic!("{}: {cause}", directory.display()))
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    found.sort();
    found
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|cause| panic!("{}: {cause}", path.display()))
        .replace("\r\n", "\n")
}

/// The whole pipeline over one project, or nothing where zig is not on PATH.
/// The crawl runs the compiler, so every case here skips rather than failing on
/// a machine that cannot run it.
fn ported(name: &str) -> Option<zag_emit::Output> {
    let directory = workspace_root().join("corpus").join(name);
    let project = match zag::read_project(&directory) {
        Ok(project) => project,
        Err(complaint) => {
            eprintln!("skipping {name}: {complaint}");
            return None;
        }
    };
    let tables = zag_frontend::build_project(&project, "x86_64-linux");
    assert_eq!(
        zag_facts::validate::validate(&tables),
        Ok(()),
        "{name}: reading the project built tables the validator rejects"
    );
    Some(zag::generate(&tables).unwrap_or_else(|cause| panic!("{name}: {}", zag::describe(&cause))))
}

#[test]
fn there_are_projects_to_read() {
    assert!(names().len() >= 3, "{:?}", names());
}

#[test]
fn every_project_stands_on_its_own() {
    for name in names() {
        let directory = workspace_root().join("corpus").join(&name);
        for required in ["build.zig", "build.zig.zon", "src/main.zig"] {
            assert!(
                directory.join(required).is_file(),
                "{name} is missing {required}, so it cannot be built on its own"
            );
        }
    }
}

/// Every project is named in the README, because a corpus nobody can read the
/// purpose of is a directory of files.
#[test]
fn every_project_is_described() {
    let readme = read(&workspace_root().join("corpus").join("README.md"));
    for name in names() {
        assert!(
            readme.contains(&name),
            "corpus/README.md does not name {name}"
        );
    }
}

#[test]
fn every_project_ports_to_what_is_checked_in() {
    for name in names() {
        let Some(output) = ported(&name) else {
            continue;
        };
        let directory = workspace_root().join("corpus").join(&name).join("expected");
        let source = String::from_utf8(output.source).expect("the port is text");
        let report = String::from_utf8(output.report).expect("the report is text");
        assert_eq!(
            source,
            read(&directory.join("port.rs")),
            "{name}: the port changed, run `just corpus` and read the diff"
        );
        assert_eq!(
            report,
            read(&directory.join("port.report.txt")),
            "{name}: the report changed, run `just corpus` and read the diff"
        );
    }
}

/// Reading the same project twice has to give the same port, or the checked in
/// output above means nothing.
#[test]
fn reading_a_project_twice_ports_the_same_way() {
    for name in names() {
        let (Some(first), Some(second)) = (ported(&name), ported(&name)) else {
            continue;
        };
        assert_eq!(first.source, second.source, "{name}");
        assert_eq!(first.report, second.report, "{name}");
    }
}

/// Metadata is the only thing emitted, so there is no code generation and no
/// linking, but constants are still evaluated, which is what turns the layout
/// assertions the emitter wrote into part of the check.
fn compiles(path: &Path, directory: &Path) -> Result<(), String> {
    std::fs::create_dir_all(directory).map_err(|cause| format!("{cause}"))?;
    let output = Command::new("rustc")
        .arg("--edition")
        .arg("2024")
        .arg("--crate-type")
        .arg("lib")
        .arg("--emit")
        .arg("metadata")
        .arg("--out-dir")
        .arg(directory)
        .arg(path)
        .output()
        .map_err(|cause| format!("running rustc: {cause}"))?;
    if output.status.success() {
        return Ok(());
    }
    Err(String::from_utf8_lossy(&output.stderr).into_owned())
}

/// The whole claim. A port that does not compile is a port nobody can start
/// from, and every refusal the passes make is there so this holds.
#[test]
fn every_port_compiles() {
    let directory = workspace_root().join("target").join("corpus-rustc");
    for name in names() {
        let port = workspace_root()
            .join("corpus")
            .join(&name)
            .join("expected")
            .join("port.rs");
        if let Err(complaint) = compiles(&port, &directory) {
            panic!(
                "{name}: the port does not compile
{complaint}"
            );
        }
    }
}

/// The crate form, which is what somebody actually keeps. A project read
/// through its build script carries an artifact per executable, so this is the
/// only place the binary the layout writes for one is handed to cargo.
#[test]
fn cargo_builds_every_project_laid_out_as_a_crate() {
    if Command::new("cargo").arg("--version").output().is_err() {
        return;
    }
    for name in names() {
        let Some(output) = ported(&name) else {
            continue;
        };
        let project = zag::read_project(&workspace_root().join("corpus").join(&name))
            .expect("the project read a moment ago still reads");
        let tables = zag_frontend::build_project(&project, "x86_64-linux");
        let port = zag_emit::layout::lay_out(&tables, &output, &name);
        assert!(
            !port.binaries.is_empty(),
            "{name}: the build script names an executable and the layout wrote no binary"
        );
        let directory = workspace_root()
            .join("target")
            .join("corpus-crate")
            .join(&name);
        let _ = std::fs::remove_dir_all(&directory);
        for file in &port.files {
            let path = directory.join(&file.path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("the scratch directory is writable");
            }
            std::fs::write(&path, &file.contents).expect("the scratch file is writable");
        }
        let built = Command::new("cargo")
            .arg("build")
            .arg("--manifest-path")
            .arg(directory.join("Cargo.toml"))
            .output()
            .expect("cargo runs");
        assert!(
            built.status.success(),
            "{name} does not build as a crate:
{}",
            String::from_utf8_lossy(&built.stderr)
        );
    }
}

/// What fraction of fields the analysis settles, printed rather than asserted.
/// A threshold here would be a number to game. The point is that a change in
/// coverage is visible while the diff that caused it is on screen.
#[test]
fn how_much_of_the_corpus_the_analysis_settles() {
    let mut decided = 0;
    let mut total = 0;
    for name in names() {
        let directory = workspace_root().join("corpus").join(&name);
        let Ok(project) = zag::read_project(&directory) else {
            continue;
        };
        let tables = zag_frontend::build_project(&project, "x86_64-linux");
        let analysis = zag_analysis::analyze(&tables);
        let settled = analysis
            .ownership
            .class
            .iter()
            .filter(|class| **class != zag_analysis::ownership::OwnershipClass::Unknown)
            .count();
        println!(
            "{name}: {settled}/{} fields settled",
            analysis.ownership.class.len()
        );
        decided += settled;
        total += analysis.ownership.class.len();
    }
    if total > 0 {
        println!("corpus: {decided}/{total} fields settled");
    }
}
