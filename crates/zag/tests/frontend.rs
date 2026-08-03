//! The frontend reads the example programs with the compiler and builds fact
//! tables from what it finds. The hand-built tables were verified against
//! those same programs field by field, so they are the oracle here: a frontend
//! that reproduces them is reading the program the way a person did.
//!
//! When the frontend covers everything the hand-built tables carry, the
//! hand-built tables stop having a reason to exist.

use std::path::{Path, PathBuf};
use zag_facts::examples::{NAMES, is_synthetic, tables_for};

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
    let modules = match zag::read_project(&source_of(name)) {
        Ok(modules) => modules,
        Err(complaint) => {
            // Reading needs zig, and the parser half of it needs a linker.
            eprintln!("skipping {name}: {complaint}");
            return None;
        }
    };
    Some(zag_frontend::build_project(&modules, "x86_64-linux"))
}

fn runnable_names() -> Vec<&'static str> {
    NAMES
        .into_iter()
        .filter(|name| !is_synthetic(name))
        .collect()
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
