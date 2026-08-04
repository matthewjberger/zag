//! Checks the dataflow in the hand-built fact tables against the dataflow the
//! compiler's parser finds in the program they describe. Reflection covers the
//! declarations and layout, and this covers which function calls which, which
//! call allocates or frees, and what each struct literal puts in a field.
//!
//! `crates/zag/tools/extract` reads syntax rather than semantics, so the checks below are
//! necessary conditions rather than a re-derivation. A table claiming an
//! allocation in a function that never allocates fails here. A table claiming
//! the wrong allocator does not, and that is what the Sema frontend is for.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use zag_facts::examples::{NAMES, tables_for};
use zag_facts::tables::{
    AssignmentSource, MemoryOperationKind, PlaceKind, Tables, function_count, string_bytes,
};

const ALLOCATING: [&str; 6] = [
    "dupe",
    "dupeZ",
    "alloc",
    "allocSentinel",
    "create",
    "realloc",
];
const FREEING: [&str; 2] = ["free", "destroy"];
const RESIZING: [&str; 3] = ["realloc", "reallocAdvanced", "resize"];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the crate lives two directories under the workspace root")
        .to_path_buf()
}

#[derive(Debug, Default)]
struct Extraction {
    functions: BTreeSet<String>,
    calls: BTreeSet<(String, String)>,
    /// Every field a free call names, as the function doing the freeing, what
    /// it was reached through, and the field. Kept per occurrence rather than
    /// as a set, because a function freeing two fields is exactly the case
    /// worth checking.
    freed: Vec<(String, String, String)>,
    initialisers: Vec<(String, String, String)>,
    parameters: BTreeSet<(String, String)>,
}

fn last_segment(text: &str) -> &str {
    text.rsplit('.').next().unwrap_or(text)
}

fn value_of<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let start = line.find(&format!("{key}="))? + key.len() + 1;
    let rest = &line[start..];
    match key {
        "value" | "type" => Some(rest),
        _ => Some(rest.split_whitespace().next().unwrap_or(rest)),
    }
}

fn parse(text: &str) -> Extraction {
    let mut extraction = Extraction::default();
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let (Some(kind), Some(subject)) = (parts.next(), parts.next()) else {
            continue;
        };
        match kind {
            "function" => {
                extraction.functions.insert(subject.to_string());
            }
            "parameter" => {
                let owner = subject.split('.').next().unwrap_or(subject);
                if let Some(name) = value_of(line, "name") {
                    extraction
                        .parameters
                        .insert((owner.to_string(), name.to_string()));
                }
            }
            "call" => {
                if let Some(callee) = value_of(line, "callee") {
                    extraction
                        .calls
                        .insert((subject.to_string(), callee.to_string()));
                }
            }
            // `argument <fn>|<callee>|<index> text=<x>.<field>`. Only the
            // first argument of a freeing call says what was freed.
            "argument" => {
                let mut pieces = subject.split('|');
                let (Some(owner), Some(callee), Some("0")) =
                    (pieces.next(), pieces.next(), pieces.next())
                else {
                    continue;
                };
                if !FREEING.contains(&last_segment(callee)) {
                    continue;
                }
                if let Some(text) = value_of(line, "text")
                    && let Some((holder, field)) = text.trim().rsplit_once('.')
                {
                    extraction.freed.push((
                        owner.to_string(),
                        holder.to_string(),
                        field.to_string(),
                    ));
                }
            }
            "initialiser" => {
                if let (Some(field), Some(value)) =
                    (value_of(line, "field"), value_of(line, "value"))
                {
                    extraction.initialisers.push((
                        subject.to_string(),
                        field.to_string(),
                        value.to_string(),
                    ));
                }
            }
            _ => {}
        }
    }
    extraction
}

/// Every Zig file the example is made of, sorted so a run is reproducible.
fn sources_of(root: &Path, name: &str) -> Vec<PathBuf> {
    let directory = root.join("examples").join(name).join("src");
    let mut found: Vec<PathBuf> = std::fs::read_dir(&directory)
        .unwrap_or_else(|cause| panic!("{}: {cause}", directory.display()))
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "zig"))
        .collect();
    found.sort();
    found
}

fn extract(name: &str) -> Option<Extraction> {
    let root = workspace_root();
    let tool = root
        .join("crates")
        .join("zag")
        .join("tools")
        .join("extract")
        .join("main.zig");
    let mut text = String::new();
    for source in sources_of(&root, name) {
        let output = Command::new("zig")
            .arg("run")
            .arg(&tool)
            .arg("--")
            .arg(&source)
            .current_dir(&root)
            .output()
            .ok()?;
        text.push_str(&String::from_utf8_lossy(&output.stderr));
    }
    // Building the extractor needs a linker, which not every machine has set
    // up for zig. Parsing is the same everywhere, so one platform running this
    // is enough and the rest say so rather than failing.
    if !text.contains("function ") {
        eprintln!("skipping {name}: the extractor did not build\n{text}");
        return None;
    }
    Some(parse(&text))
}

fn text_of(tables: &Tables, id: zag_facts::StringId) -> String {
    String::from_utf8_lossy(string_bytes(&tables.strings, id)).into_owned()
}

fn function_name(tables: &Tables, index: usize) -> String {
    text_of(tables, tables.functions.name[index])
}

fn calls_in(extraction: &Extraction, caller: &str, verbs: &[&str]) -> bool {
    extraction
        .calls
        .iter()
        .any(|(from, callee)| from == caller && verbs.contains(&last_segment(callee)))
}

fn runnable_names() -> Vec<&'static str> {
    NAMES.to_vec()
}

#[test]
fn the_tables_declare_exactly_the_functions_the_parser_found() {
    for name in runnable_names() {
        let Some(extraction) = extract(name) else {
            continue;
        };
        let tables = tables_for(name).expect("registered");
        let declared: BTreeSet<String> = (0..function_count(&tables.functions))
            .map(|index| function_name(&tables, index))
            .collect();
        // The parser sees private declarations, so unlike reflection this runs
        // both ways.
        assert_eq!(
            declared, extraction.functions,
            "{name}: the tables and the program disagree about which functions exist"
        );
    }
}

#[test]
fn every_parameter_in_the_tables_is_named_the_same_in_the_program() {
    for name in runnable_names() {
        let Some(extraction) = extract(name) else {
            continue;
        };
        let tables = tables_for(name).expect("registered");
        for index in 0..function_count(&tables.functions) {
            let owner = function_name(&tables, index);
            for row in zag_facts::tables::function_parameters(
                &tables.functions,
                zag_facts::FunctionId(index as u32),
            ) {
                let parameter = text_of(&tables, tables.parameters.name[row]);
                assert!(
                    extraction
                        .parameters
                        .contains(&(owner.clone(), parameter.clone())),
                    "{name}: {owner} has no parameter called {parameter}"
                );
            }
        }
    }
}

#[test]
fn every_call_edge_in_the_tables_is_a_call_in_the_program() {
    for name in runnable_names() {
        let Some(extraction) = extract(name) else {
            continue;
        };
        let tables = tables_for(name).expect("registered");
        for row in 0..tables.calls.caller.len() {
            let caller = function_name(&tables, tables.calls.caller[row].0 as usize);
            let callee = function_name(&tables, tables.calls.callee[row].0 as usize);
            let found = extraction
                .calls
                .iter()
                .any(|(from, target)| *from == caller && last_segment(target) == callee);
            assert!(found, "{name}: {caller} never calls {callee}");
        }
    }
}

#[test]
fn every_memory_operation_in_the_tables_happens_in_the_program() {
    for name in runnable_names() {
        let Some(extraction) = extract(name) else {
            continue;
        };
        let tables = tables_for(name).expect("registered");
        for row in 0..tables.memory_operations.function.len() {
            let owner = function_name(&tables, tables.memory_operations.function[row].0 as usize);
            let (verbs, description) = match tables.memory_operations.kind[row] {
                MemoryOperationKind::Allocate => (ALLOCATING.as_slice(), "allocate"),
                MemoryOperationKind::Free => (FREEING.as_slice(), "free"),
                MemoryOperationKind::Resize => (RESIZING.as_slice(), "resize"),
            };
            assert!(
                calls_in(&extraction, &owner, verbs),
                "{name}: the tables say {owner} should {description} and it never does"
            );
        }
    }
}

/// The other direction. `every_memory_operation_in_the_tables_happens_in_the
/// _program` checks that nothing in the tables was invented, and on its own it
/// says nothing about what the tables left out. A free the parser reported and
/// the tables never recorded is a field that silently loses its owner.
///
/// Only a free reached through one of the function's own parameters counts. A
/// free of a field of a local is a free the frontend cannot attribute to any
/// field, and the tables say so by recording it with no field at all.
#[test]
fn every_free_the_parser_found_through_a_parameter_reaches_the_tables() {
    for name in runnable_names() {
        let Some(extraction) = extract(name) else {
            continue;
        };
        let tables = tables_for(name).expect("registered");
        let recorded: Vec<(String, String)> = (0..tables.memory_operations.function.len())
            .filter(|row| {
                tables.memory_operations.kind[*row] == MemoryOperationKind::Free
                    && tables.memory_operations.place[*row] == PlaceKind::FieldOfParameter
            })
            .filter_map(|row| {
                let owner =
                    function_name(&tables, tables.memory_operations.function[row].0 as usize);
                let field = tables.memory_operations.place_field[row];
                let field = tables.fields.name.get(field.0 as usize)?;
                Some((owner, text_of(&tables, *field)))
            })
            .collect();
        for (owner, holder, field) in &extraction.freed {
            if !extraction
                .parameters
                .contains(&(owner.clone(), holder.clone()))
            {
                continue;
            }
            let wanted = (owner.clone(), field.clone());
            let written = extraction
                .freed
                .iter()
                .filter(|(from, through, what)| (from, what) == (owner, field) && through == holder)
                .count();
            let found = recorded.iter().filter(|entry| **entry == wanted).count();
            assert!(
                found >= written,
                "{name}: {owner} frees {holder}.{field} {written} time(s) and the tables record it {found}"
            );
        }
    }
}

/// The same set the frontend calls a literal. A table claiming a static
/// literal is claiming the frontend would have read one here.
fn is_literal(value: &str) -> bool {
    value == "null"
        || value.starts_with('"')
        || value.starts_with("&.{")
        || value.starts_with(".{")
        || value.starts_with("0x")
        || value
            .chars()
            .next()
            .is_some_and(|byte| byte.is_ascii_digit())
}

#[test]
fn every_field_assignment_in_the_tables_is_a_field_the_program_writes() {
    for name in runnable_names() {
        let Some(extraction) = extract(name) else {
            continue;
        };
        let tables = tables_for(name).expect("registered");
        for row in 0..tables.field_assignments.field.len() {
            let field = text_of(
                &tables,
                tables.fields.name[tables.field_assignments.field[row].0 as usize],
            );
            let owner = function_name(&tables, tables.field_assignments.function[row].0 as usize);
            let found = extraction
                .initialisers
                .iter()
                .find(|(function, name, _)| *function == owner && *name == field)
                .unwrap_or_else(|| panic!("{name}: {owner} never writes {field}"));
            let value = found.2.as_str();
            match tables.field_assignments.source[row] {
                AssignmentSource::Allocation => assert!(
                    calls_in(&extraction, &owner, ALLOCATING.as_slice()),
                    "{name}: {owner} assigns {field} from an allocation it never makes"
                ),
                AssignmentSource::Parameter => assert!(
                    extraction
                        .parameters
                        .contains(&(owner.clone(), value.to_string())),
                    "{name}: {field} is assigned {value:?}, which is not a parameter of {owner}"
                ),
                AssignmentSource::StaticLiteral => assert!(
                    is_literal(value),
                    "{name}: {field} is assigned {value:?}, which is not a literal"
                ),
                AssignmentSource::Unknown => {}
            }
        }
    }
}

#[test]
fn the_extractor_reaches_a_private_function_reflection_cannot_see() {
    let Some(extraction) = extract("wordcount") else {
        return;
    };
    assert!(
        extraction.functions.contains("release"),
        "wordcount's private helper should be visible to the parser"
    );
}
