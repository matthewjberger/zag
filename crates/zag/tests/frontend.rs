//! The frontend reads the example programs with the compiler and builds fact
//! tables from what it finds. The hand-built tables were verified against
//! those same programs field by field, so they are the oracle here: a frontend
//! that reproduces them is reading the program the way a person did.
//!
//! When the frontend covers everything the hand-built tables carry, the
//! hand-built tables stop having a reason to exist.

use std::path::{Path, PathBuf};
use zag_facts::examples::{NAMES, tables_for};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the crate lives two directories under the workspace root")
        .to_path_buf()
}

fn source_of(name: &str) -> PathBuf {
    workspace_root()
        .join("examples")
        .join(name)
        .join("src")
        .join("main.zig")
}

fn read(name: &str) -> Option<zag_facts::tables::Tables> {
    let project = match zag::read_project(&source_of(name)) {
        Ok(project) => project,
        Err(complaint) => {
            // Reading needs zig, and the parser half of it needs a linker.
            eprintln!("skipping {name}: {complaint}");
            return None;
        }
    };
    Some(zag_frontend::build_project(&project, "x86_64-linux"))
}

fn runnable_names() -> Vec<&'static str> {
    NAMES.to_vec()
}

#[test]
fn the_frontend_builds_tables_the_validator_accepts() {
    for name in runnable_names() {
        let Some(tables) = read(name) else { continue };
        assert_eq!(
            zag_facts::validate::validate(&tables),
            Ok(()),
            "{name}: the frontend built tables the validator rejects"
        );
    }
}

#[test]
fn the_frontend_ports_every_example_the_way_the_hand_built_tables_do() {
    for name in runnable_names() {
        let Some(tables) = read(name) else { continue };
        let from_source = zag::generate(&tables)
            .unwrap_or_else(|cause| panic!("{name}: {}", zag::describe(&cause)));
        let by_hand = zag::generate(&tables_for(name).expect("registered"))
            .unwrap_or_else(|cause| panic!("{name}: {}", zag::describe(&cause)));
        assert_eq!(
            String::from_utf8_lossy(&from_source.source),
            String::from_utf8_lossy(&by_hand.source),
            "{name}: reading the program gives a different port than the tables written for it"
        );
    }
}

/// The report is the deliverable for everything the port cannot write, so the
/// hand-built tables have to produce the same one. This is stricter than the
/// source comparison above: it covers the source locations, the reasons, and
/// the allocator conflicts, none of which reach the emitted Rust.
#[test]
fn the_frontend_reports_what_the_hand_built_tables_report() {
    for name in runnable_names() {
        let Some(tables) = read(name) else { continue };
        let from_source = zag::generate(&tables)
            .unwrap_or_else(|cause| panic!("{name}: {}", zag::describe(&cause)));
        let by_hand = zag::generate(&tables_for(name).expect("registered"))
            .unwrap_or_else(|cause| panic!("{name}: {}", zag::describe(&cause)));
        assert_eq!(
            String::from_utf8_lossy(&from_source.report),
            String::from_utf8_lossy(&by_hand.report),
            "{name}: reading the program gives a different report than the tables written for it"
        );
    }
}

/// A location is only useful if it points at the right line, so this reads the
/// Zig back and checks the line the report names really is the declaration.
#[test]
fn the_line_a_report_names_is_the_line_the_function_is_declared_on() {
    for name in runnable_names() {
        let Some(tables) = read(name) else { continue };
        let root = workspace_root().join("examples").join(name).join("src");
        for index in 0..zag_facts::tables::function_count(&tables.functions) {
            let line = tables.functions.line[index];
            assert_ne!(line, 0, "{name}: a function came back with no line");
            let module = tables.functions.module[index].0 as usize;
            let path =
                zag_facts::tables::string_bytes(&tables.strings, tables.modules.path[module]);
            let source = read_source(&root.join(String::from_utf8_lossy(path).as_ref()));
            let declared = source
                .lines()
                .nth(line as usize - 1)
                .unwrap_or_default()
                .trim();
            let function =
                zag_facts::tables::string_bytes(&tables.strings, tables.functions.name[index]);
            let function = String::from_utf8_lossy(function);
            assert!(
                declared.contains(&format!("fn {function}")),
                "{name}: {function} is reported at line {line}, which reads {declared:?}"
            );
        }
    }
}

fn read_source(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|cause| panic!("{}: {cause}", path.display()))
}

#[test]
fn the_frontend_reaches_the_same_ownership_decisions() {
    for name in runnable_names() {
        let Some(tables) = read(name) else { continue };
        let read_analysis = zag_analysis::analyze(&tables);
        let hand = tables_for(name).expect("registered");
        let hand_analysis = zag_analysis::analyze(&hand);
        assert_eq!(
            read_analysis.ownership.class, hand_analysis.ownership.class,
            "{name}: the classes differ"
        );
        assert_eq!(
            read_analysis.ownership.confidence, hand_analysis.ownership.confidence,
            "{name}: the confidences differ"
        );
        assert!(read_analysis.provenance.converged);
    }
}

#[test]
fn the_frontend_reads_the_layout_the_compiler_resolved() {
    let Some(tables) = read("netpacket") else {
        return;
    };
    let header = tables
        .structs
        .name
        .iter()
        .position(|name| zag_facts::tables::string_bytes(&tables.strings, *name) == b"Header")
        .expect("netpacket declares a header");
    assert_eq!(tables.structs.size[header], 12);
    assert_eq!(tables.structs.alignment[header], 4);
    assert_ne!(
        tables.structs.flags[header] & zag_facts::tables::STRUCT_FLAG_EXTERN,
        0,
        "the header keeps its C layout"
    );
}

#[test]
fn a_reordered_layout_comes_back_reordered() {
    let Some(tables) = read("netpacket") else {
        return;
    };
    // Zig puts the packet's slice first in memory and its header second, which
    // is the offset a person would have guessed wrong.
    let offsets: Vec<(String, u32)> = (0..tables.fields.owner.len())
        .filter(|row| {
            let owner = tables.fields.owner[*row].0 as usize;
            zag_facts::tables::string_bytes(&tables.strings, tables.structs.name[owner])
                == b"Packet"
        })
        .map(|row| {
            (
                String::from_utf8_lossy(zag_facts::tables::string_bytes(
                    &tables.strings,
                    tables.fields.name[row],
                ))
                .into_owned(),
                tables.fields.offset[row],
            )
        })
        .collect();
    assert_eq!(
        offsets,
        vec![("header".to_string(), 16), ("payload".to_string(), 0)]
    );
}
