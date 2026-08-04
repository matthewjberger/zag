//! Reading a program that is more than one file, and a project that builds
//! more than one program.
//!
//! The crawl runs the compiler, so every case here skips where zig is not on
//! PATH. What it must never do is panic: a missing file, a cycle, an import
//! that names nothing, and a build script that says nothing readable are all
//! answers rather than crashes.

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

fn read(root: &Path) -> Option<zag_frontend::project::Project> {
    match zag::read_project(root) {
        Ok(project) => Some(project),
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
    let Some(project) = read(&root) else { return };
    let modules = &project.modules;
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
    let Some(project) = read(&root) else { return };
    let modules = &project.modules;
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
    let Some(project) = read(&root) else { return };
    let modules = &project.modules;
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
    let Some(project) = read(&root) else { return };
    let modules = &project.modules;
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
    let Some(project) = read(&root) else { return };
    let modules = &project.modules;
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
    let Some(project) = read(&root) else { return };
    let modules = &project.modules;
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
    let Some(project) = read(&root) else { return };
    let tables = zag_frontend::build_project(&project, "x86_64-linux");
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
    let Some(project) = read(&root) else { return };
    let modules = &project.modules;
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

/// A build script that names two executables over one shared module. `run`
/// points at the first, so `zig build run` still works on it.
fn two_program_project(name: &str) -> PathBuf {
    let directory = scratch(name);
    let source = directory.join("src");
    std::fs::create_dir_all(&source).expect("the source directory is writable");
    write(
        &source,
        "shared.zig",
        "pub const Shared = struct { count: u32 };\n",
    );
    write(
        &source,
        "reader.zig",
        "const shared = @import(\"shared.zig\");\npub fn main() void {\n    _ = shared.Shared;\n}\n",
    );
    write(
        &source,
        "writer.zig",
        "const shared = @import(\"shared.zig\");\npub fn main() void {\n    _ = shared.Shared;\n}\n",
    );
    write(
        &directory,
        "build.zig",
        "const std = @import(\"std\");\n\
         pub fn build(b: *std.Build) void {\n\
         \x20   const target = b.standardTargetOptions(.{});\n\
         \x20   const optimize = b.standardOptimizeOption(.{});\n\
         \x20   const reader = b.addExecutable(.{\n\
         \x20       .name = \"reader\",\n\
         \x20       .root_module = b.createModule(.{\n\
         \x20           .root_source_file = b.path(\"src/reader.zig\"),\n\
         \x20           .target = target,\n\
         \x20           .optimize = optimize,\n\
         \x20       }),\n\
         \x20   });\n\
         \x20   b.installArtifact(reader);\n\
         \x20   const writer = b.addExecutable(.{\n\
         \x20       .name = \"writer\",\n\
         \x20       .root_module = b.createModule(.{\n\
         \x20           .root_source_file = b.path(\"src/writer.zig\"),\n\
         \x20           .target = target,\n\
         \x20           .optimize = optimize,\n\
         \x20       }),\n\
         \x20   });\n\
         \x20   b.installArtifact(writer);\n\
         }\n",
    );
    directory
}

#[test]
fn a_build_script_naming_two_executables_gives_two_artifacts_over_shared_modules() {
    let directory = two_program_project("two-programs");
    let Some(project) = read(&directory) else {
        return;
    };
    let names: Vec<&str> = project
        .artifacts
        .iter()
        .map(|artifact| artifact.name.as_str())
        .collect();
    assert_eq!(names, vec!["reader", "writer"]);
    assert_eq!(
        project.artifacts[0].root.as_deref(),
        Some("reader"),
        "{:?}",
        project.artifacts
    );
    assert_eq!(project.artifacts[1].root.as_deref(), Some("writer"));
    // The empty top level, then each file the two roots reach between them,
    // with the shared one read once.
    let modules: Vec<&str> = project
        .modules
        .iter()
        .map(|module| module.name.as_str())
        .collect();
    assert_eq!(modules, vec!["", "reader", "shared", "writer"]);
}

#[test]
fn pointing_at_the_directory_and_at_its_build_script_read_the_same_project() {
    let directory = two_program_project("two-programs-directory");
    let Some(from_directory) = read(&directory) else {
        return;
    };
    let Some(from_script) = read(&directory.join("build.zig")) else {
        return;
    };
    assert_eq!(from_directory, from_script);
}

#[test]
fn each_executable_gets_a_binary_that_calls_what_its_root_ported() {
    let directory = two_program_project("two-programs-port");
    let Some(project) = read(&directory) else {
        return;
    };
    let tables = zag_frontend::build_project(&project, "x86_64-linux");
    assert_eq!(zag_facts::validate::validate(&tables), Ok(()));
    let output = zag::generate(&tables).expect("the tables port");
    let port = zag_emit::layout::lay_out(&tables, &output, "two_programs");
    assert_eq!(port.binaries, vec!["reader", "writer"]);
    let written: Vec<&str> = port.files.iter().map(|file| file.path.as_str()).collect();
    assert!(written.contains(&"src/bin/reader.rs"), "{written:?}");
    assert!(written.contains(&"src/bin/writer.rs"), "{written:?}");
    let reader = port
        .files
        .iter()
        .find(|file| file.path == "src/bin/reader.rs")
        .expect("the reader binary is written");
    let text = String::from_utf8(reader.contents.clone()).expect("the binary is text");
    assert!(text.contains("two_programs::reader::main()"), "{text}");
    let report = String::from_utf8(output.report).expect("the report is text");
    assert!(report.contains("executable reader: reader.zig"), "{report}");
}

#[test]
fn a_directory_with_no_build_script_is_an_error_rather_than_a_panic() {
    let directory = scratch("no-build-script");
    assert!(zag::read_project(&directory).is_err());
}

#[test]
fn a_build_script_that_declares_nothing_readable_is_an_error_rather_than_a_panic() {
    let directory = scratch("empty-build-script");
    write(
        &directory,
        "build.zig",
        "const std = @import(\"std\");\npub fn build(b: *std.Build) void {\n    _ = b;\n}\n",
    );
    assert!(zag::read_project(&directory).is_err());
}

/// The example projects are whole Zig projects with their own build scripts, so
/// pointing zag at one is the case a person actually has.
#[test]
fn an_example_project_reads_through_its_build_script() {
    let directory = workspace_root().join("examples").join("ledger");
    let Some(project) = read(&directory) else {
        return;
    };
    assert_eq!(project.artifacts.len(), 1);
    assert_eq!(project.artifacts[0].name, "ledger");
    assert_eq!(project.artifacts[0].root.as_deref(), Some("main"));
    assert_eq!(
        project.artifacts[0].kind,
        zag_facts::tables::ArtifactKind::Executable
    );
}

#[test]
fn a_field_allocated_in_one_file_and_freed_in_another_still_comes_out_owned() {
    let root = workspace_root()
        .join("examples")
        .join("ledger")
        .join("src")
        .join("main.zig");
    let Some(project) = read(&root) else { return };
    let tables = zag_frontend::build_project(&project, "x86_64-linux");
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
