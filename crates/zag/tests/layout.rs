//! The port as a crate rather than as one file.
//!
//! One file is what the checked in output compares against, and a crate is
//! what somebody actually keeps. Both say the same thing, so these check that
//! the crate form carries every declaration the single file does and that
//! `cargo` accepts what comes out.

use std::path::{Path, PathBuf};
use std::process::Command;
use zag_emit::layout::{File, lay_out};
use zag_facts::examples::{NAMES, tables_for};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the crate lives two directories under the workspace root")
        .to_path_buf()
}

fn laid_out(name: &str) -> Vec<File> {
    let tables = tables_for(name).expect("registered");
    let output = zag::generate(&tables).expect("the tables port");
    lay_out(&tables, &output, name).files
}

fn contents(files: &[File], path: &str) -> String {
    files
        .iter()
        .find(|file| file.path == path)
        .map(|file| String::from_utf8_lossy(&file.contents).into_owned())
        .unwrap_or_else(|| panic!("no {path} among {:?}", paths(files)))
}

fn paths(files: &[File]) -> Vec<&str> {
    files.iter().map(|file| file.path.as_str()).collect()
}

#[test]
fn every_example_lays_out_as_a_crate() {
    for name in NAMES {
        let files = laid_out(name);
        assert!(
            paths(&files).contains(&"Cargo.toml"),
            "{name} has no manifest"
        );
        assert!(
            paths(&files).contains(&"src/lib.rs"),
            "{name} has no crate root"
        );
    }
}

#[test]
fn a_program_of_one_file_is_one_crate_of_one_module() {
    let files = laid_out("netpacket");
    assert_eq!(paths(&files), vec!["Cargo.toml", "src/lib.rs"]);
    assert!(contents(&files, "src/lib.rs").contains("pub struct Header"));
}

/// A Zig file is a Rust module, and a module in a crate is a file of its own
/// rather than a block inside another one.
#[test]
fn a_program_of_several_files_gets_a_file_per_module() {
    let files = laid_out("ledger");
    assert_eq!(
        paths(&files),
        vec!["Cargo.toml", "src/entry.rs", "src/lib.rs", "src/store.rs"]
    );
    let root = contents(&files, "src/lib.rs");
    assert!(root.contains("pub mod entry;"), "{root}");
    assert!(root.contains("pub mod store;"), "{root}");
    let entry = contents(&files, "src/entry.rs");
    assert!(entry.starts_with("pub struct Entry {"), "{entry}");
    assert!(!entry.contains("pub mod"), "{entry}");
}

#[test]
fn the_declarations_the_single_file_carries_are_all_in_the_crate() {
    for name in NAMES {
        let tables = tables_for(name).expect("registered");
        let output = zag::generate(&tables).expect("the tables port");
        let single = String::from_utf8_lossy(&output.source).into_owned();
        let files = laid_out(name);
        let together: String = files
            .iter()
            .filter(|file| file.path.ends_with(".rs"))
            .map(|file| String::from_utf8_lossy(&file.contents).into_owned())
            .collect();
        for declared in single
            .lines()
            .filter(|line| line.starts_with("pub struct ") || line.starts_with("pub enum "))
        {
            assert!(
                together.contains(declared.trim()),
                "{name}: the crate is missing {declared:?}"
            );
        }
    }
}

/// A manifest a port writes has to stand on its own, because it is written
/// wherever the reader points it and that may be inside another crate.
#[test]
fn a_lone_crate_is_not_taken_for_a_member_of_whatever_surrounds_it() {
    let manifest = contents(&laid_out("ledger"), "Cargo.toml");
    assert!(manifest.starts_with("[workspace]"), "{manifest}");
    assert!(manifest.contains("unsafe_code = \"forbid\""), "{manifest}");
}

/// A Zig package the crawl could not read is a crate the port needs and does
/// not have, so the port becomes a workspace with a place for it.
#[test]
fn a_package_the_crawl_could_not_read_makes_the_port_a_workspace() {
    let mut tables = tables_for("netpacket").expect("registered");
    zag_facts::build::push_unresolved_import(
        &mut tables,
        zag_facts::tables::ROOT_MODULE,
        b"some_package",
    );
    let output = zag::generate(&tables).expect("the tables port");
    let port = lay_out(&tables, &output, "netpacket");
    assert_eq!(port.crates, vec!["netpacket", "some_package"]);
    let workspace = contents(&port.files, "Cargo.toml");
    assert!(workspace.starts_with("[workspace]"), "{workspace}");
    assert!(workspace.contains("\"netpacket\","), "{workspace}");
    assert!(workspace.contains("\"some_package\","), "{workspace}");
    let member = contents(&port.files, "netpacket/Cargo.toml");
    assert!(
        member.contains("some_package = { path = \"../some_package\" }"),
        "{member}"
    );
    assert!(
        !member.starts_with("[workspace]"),
        "a member is not its own workspace: {member}"
    );
}

/// A missing file is not a missing package, so it adds no crate.
#[test]
fn a_path_import_that_reached_nothing_adds_no_crate() {
    let mut tables = tables_for("netpacket").expect("registered");
    zag_facts::build::push_unresolved_import(
        &mut tables,
        zag_facts::tables::ROOT_MODULE,
        b"missing.zig",
    );
    let output = zag::generate(&tables).expect("the tables port");
    assert_eq!(
        lay_out(&tables, &output, "netpacket").crates,
        vec!["netpacket"]
    );
}

#[test]
fn cargo_accepts_every_example_laid_out_as_a_crate() {
    let root = workspace_root();
    if Command::new("cargo").arg("--version").output().is_err() {
        return;
    }
    for name in NAMES {
        let directory = root.join("target").join("layout").join(name);
        let _ = std::fs::remove_dir_all(&directory);
        for file in laid_out(name) {
            let path = directory.join(&file.path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("the scratch directory is writable");
            }
            std::fs::write(&path, &file.contents).expect("the scratch file is writable");
        }
        let output = Command::new("cargo")
            .arg("build")
            .arg("--manifest-path")
            .arg(directory.join("Cargo.toml"))
            .output()
            .expect("cargo runs");
        assert!(
            output.status.success(),
            "{name} does not build as a crate:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
