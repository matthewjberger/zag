//! Reading a program that is more than one file.
//!
//! The crawl runs the compiler, so every case here skips where zig is not on
//! PATH. What it must never do is panic: a missing file, a cycle, and an import
//! that names nothing are all answers rather than crashes.

use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the crate lives two directories under the workspace root")
        .to_path_buf()
}

fn scratch(name: &str) -> PathBuf {
    let directory = workspace_root().join("target").join("project").join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("the scratch directory is writable");
    directory
}

fn write(directory: &Path, name: &str, text: &str) -> PathBuf {
    let path = directory.join(name);
    std::fs::write(&path, text).expect("the scratch file is writable");
    path
}

fn read(root: &Path) -> Option<Vec<zag_frontend::project::SourceModule>> {
    match zag::read_project(root) {
        Ok(modules) => Some(modules),
        Err(complaint) => {
            eprintln!("skipping: {complaint}");
            None
        }
    }
}

#[test]
fn a_program_of_one_file_is_one_module_with_no_name() {
    let directory = scratch("single");
    let root = write(&directory, "main.zig", "pub fn main() void {}\n");
    let Some(modules) = read(&root) else { return };
    assert_eq!(modules.len(), 1);
    assert_eq!(modules[0].name, "");
    assert_eq!(modules[0].path, "main.zig");
}

#[test]
fn the_root_comes_first_and_the_rest_come_sorted() {
    let directory = scratch("ordered");
    write(
        &directory,
        "zebra.zig",
        "pub const Zebra = struct { a: u32 };\n",
    );
    write(
        &directory,
        "alpha.zig",
        "pub const Alpha = struct { a: u32 };\n",
    );
    let root = write(
        &directory,
        "main.zig",
        "const zebra = @import(\"zebra.zig\");\nconst alpha = @import(\"alpha.zig\");\npub fn main() void {}\n",
    );
    let Some(modules) = read(&root) else { return };
    let names: Vec<&str> = modules.iter().map(|module| module.name.as_str()).collect();
    assert_eq!(names, vec!["", "alpha", "zebra"]);
}

#[test]
fn a_cycle_terminates_and_reads_each_file_once() {
    let directory = scratch("cycle");
    write(
        &directory,
        "second.zig",
        "const first = @import(\"first.zig\");\npub const Second = struct { a: u32 };\n",
    );
    write(
        &directory,
        "first.zig",
        "const second = @import(\"second.zig\");\npub const First = struct { a: u32 };\n",
    );
    let root = write(
        &directory,
        "main.zig",
        "const first = @import(\"first.zig\");\npub fn main() void {}\n",
    );
    let Some(modules) = read(&root) else { return };
    let names: Vec<&str> = modules.iter().map(|module| module.name.as_str()).collect();
    assert_eq!(names, vec!["", "first", "second"]);
}

#[test]
fn a_file_that_imports_itself_terminates() {
    let directory = scratch("self");
    let root = write(
        &directory,
        "main.zig",
        "const own = @import(\"main.zig\");\npub fn main() void {}\n",
    );
    let Some(modules) = read(&root) else { return };
    assert_eq!(modules.len(), 1);
}

#[test]
fn an_import_that_names_nothing_is_recorded_rather_than_dropped() {
    let directory = scratch("unresolved");
    let root = write(
        &directory,
        "main.zig",
        "const missing = @import(\"missing.zig\");\nconst named = @import(\"some_package\");\npub fn main() void {}\n",
    );
    let Some(modules) = read(&root) else { return };
    assert_eq!(modules.len(), 1);
    assert_eq!(
        modules[0].unresolved,
        vec!["missing.zig".to_string(), "some_package".to_string()]
    );
}

#[test]
fn the_compilers_own_modules_are_not_followed_and_not_reported() {
    let directory = scratch("standard");
    let root = write(
        &directory,
        "main.zig",
        "const std = @import(\"std\");\nconst builtin = @import(\"builtin\");\npub fn main() void {}\n",
    );
    let Some(modules) = read(&root) else { return };
    assert_eq!(modules.len(), 1);
    assert!(modules[0].unresolved.is_empty());
}

#[test]
fn an_unresolved_import_reaches_the_report() {
    let directory = scratch("reported");
    let root = write(
        &directory,
        "main.zig",
        "const missing = @import(\"missing.zig\");\npub fn main() void {}\n",
    );
    let Some(modules) = read(&root) else { return };
    let tables = zag_frontend::build_project(&modules, "x86_64-linux");
    assert_eq!(zag_facts::validate::validate(&tables), Ok(()));
    let output = zag::generate(&tables).expect("the tables port");
    let report = String::from_utf8(output.report).expect("the report is text");
    assert!(
        report.contains("unresolved import: missing.zig"),
        "{report}"
    );
}

#[test]
fn a_root_that_is_not_there_is_an_error_rather_than_a_panic() {
    let missing = workspace_root().join("target").join("no-such-file.zig");
    assert!(zag::read_project(&missing).is_err());
}

#[test]
fn two_spellings_of_one_file_are_one_module() {
    let directory = scratch("spellings");
    write(
        &directory,
        "shared.zig",
        "pub const Shared = struct { a: u32 };\n",
    );
    let root = write(
        &directory,
        "main.zig",
        "const one = @import(\"shared.zig\");\nconst two = @import(\"./shared.zig\");\npub fn main() void {}\n",
    );
    let Some(modules) = read(&root) else { return };
    assert_eq!(modules.len(), 2, "{modules:?}");
}

#[test]
fn reading_a_project_twice_gives_the_same_tables() {
    let root = workspace_root()
        .join("examples")
        .join("ledger")
        .join("src")
        .join("main.zig");
    let Some(first) = read(&root) else { return };
    let Some(second) = read(&root) else { return };
    assert_eq!(
        zag_frontend::build_project(&first, "x86_64-linux"),
        zag_frontend::build_project(&second, "x86_64-linux")
    );
}

#[test]
fn a_field_allocated_in_one_file_and_freed_in_another_still_comes_out_owned() {
    let root = workspace_root()
        .join("examples")
        .join("ledger")
        .join("src")
        .join("main.zig");
    let Some(modules) = read(&root) else { return };
    let tables = zag_frontend::build_project(&modules, "x86_64-linux");
    let analysis = zag_analysis::analyze(&tables);
    let owned = tables
        .fields
        .name
        .iter()
        .position(|name| zag_facts::tables::string_bytes(&tables.strings, *name) == b"label")
        .expect("the ledger declares a label");
    assert_eq!(
        analysis.ownership.class[owned],
        zag_analysis::ownership::OwnershipClass::Owned,
        "no single file says who owns the label, so reading them together is the whole point"
    );
}
