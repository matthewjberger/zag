//! Whole Zig projects, read through their build scripts and ported end to end.
//!
//! The `examples/` layer holds the frontend to the compiler on programs small
//! enough to carry hand-built fact tables. This layer trades that for size.
//! Nothing here is hand-built: each project is read with the compiler, ported,
//! and compared against the output checked in beside it, so a change to any
//! pass arrives as a diff somebody has to read.
//!
//! These are ordinary programs rather than programs shaped to suit the
//! analysis, which is the point. What they surface is written down in
//! `corpus/README.md`, including the parts that come out wrong.

use std::path::{Path, PathBuf};

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
