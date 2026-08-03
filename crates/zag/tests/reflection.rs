//! Checks the hand-built fact tables against what the Zig compiler says about
//! the programs they claim to describe. Declarations and layout are resolved
//! by the compiler, so a table that drifted from its Zig fails here.
//!
//! The dataflow the tables also carry, which call allocates what and which
//! function frees it, is out of reach of type information and stays hand
//! supplied until the frontend lands.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use zag_facts::examples::{NAMES, is_synthetic, tables_for};
use zag_facts::tables::{
    STRUCT_FLAG_EXTERN, Tables, function_count, string_bytes, struct_count, struct_fields,
};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the crate lives two directories under the workspace root")
        .to_path_buf()
}

fn zig_is_available() -> bool {
    Command::new("zig")
        .arg("version")
        .output()
        .is_ok_and(|output| output.status.success())
}

#[derive(Debug, PartialEq, Eq)]
struct ReflectedStruct {
    layout: String,
    size: u32,
    alignment: u32,
    fields: Vec<(String, u32)>,
}

#[derive(Debug, Default)]
struct Reflection {
    structs: BTreeMap<String, ReflectedStruct>,
    functions: BTreeMap<String, usize>,
}

fn attribute(parts: &[&str], key: &str) -> Option<String> {
    parts.iter().find_map(|part| {
        part.strip_prefix(key)
            .and_then(|rest| rest.strip_prefix('='))
            .map(|value| value.to_string())
    })
}

fn number(parts: &[&str], key: &str) -> u32 {
    attribute(parts, key)
        .and_then(|value| value.parse().ok())
        .unwrap_or_default()
}

fn parse(output: &str) -> Reflection {
    let mut reflection = Reflection::default();
    for line in output.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        let (Some(kind), Some(name)) = (parts.first(), parts.get(1)) else {
            continue;
        };
        match *kind {
            "struct" => {
                reflection.structs.insert(
                    (*name).to_string(),
                    ReflectedStruct {
                        layout: attribute(&parts, "layout").unwrap_or_default(),
                        size: number(&parts, "size"),
                        alignment: number(&parts, "align"),
                        fields: Vec::new(),
                    },
                );
            }
            "field" => {
                let (owner, field) = name.split_once('.').unwrap_or((name, ""));
                if let Some(entry) = reflection.structs.get_mut(owner) {
                    entry
                        .fields
                        .push((field.to_string(), number(&parts, "offset")));
                }
            }
            "fn" => {
                let simple = name.rsplit('.').next().unwrap_or(name);
                reflection
                    .functions
                    .insert(simple.to_string(), number(&parts, "params") as usize);
            }
            _ => {}
        }
    }
    reflection
}

fn reflect(name: &str) -> Reflection {
    let root = workspace_root();
    let tool = root.join("tools").join("reflect").join("main.zig");
    let source = root
        .join("examples")
        .join(name)
        .join("src")
        .join("main.zig");
    // Analysed, never linked. The report arrives through `@compileError`, so
    // this fails to compile on purpose and no platform's libc is involved.
    let output = Command::new("zig")
        .arg("build-obj")
        .arg("-fno-emit-bin")
        .arg("--dep")
        .arg("target")
        .arg(format!("-Mroot={}", tool.display()))
        .arg(format!("-Mtarget={}", source.display()))
        .current_dir(&root)
        .output()
        .unwrap_or_else(|cause| panic!("running zig over {name}: {cause}"));
    let text = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        text.contains("struct "),
        "reflection over {name} produced nothing:\n{text}\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
    parse(&text)
}

fn text_of(tables: &Tables, id: zag_facts::StringId) -> String {
    String::from_utf8_lossy(string_bytes(&tables.strings, id)).into_owned()
}

/// Reflection reports structs. An enum, a union, and an error set are reported
/// by the parser instead, so they are compared there rather than here.
fn declared_structs(tables: &Tables) -> BTreeMap<String, ReflectedStruct> {
    (0..struct_count(&tables.structs))
        .filter(|index| {
            tables.structs.kind.get(*index).copied()
                == Some(zag_facts::tables::ContainerKind::Struct)
        })
        .map(|index| {
            let owner = zag_facts::StructId(index as u32);
            let layout = if tables.structs.flags[index] & STRUCT_FLAG_EXTERN != 0 {
                "extern"
            } else {
                "auto"
            };
            let fields = struct_fields(&tables.structs, owner)
                .map(|row| {
                    (
                        text_of(tables, tables.fields.name[row]),
                        tables.fields.offset[row],
                    )
                })
                .collect();
            (
                text_of(tables, tables.structs.name[index]),
                ReflectedStruct {
                    layout: layout.to_string(),
                    size: tables.structs.size[index],
                    alignment: tables.structs.alignment[index],
                    fields,
                },
            )
        })
        .collect()
}

fn declared_functions(tables: &Tables) -> BTreeMap<String, usize> {
    (0..function_count(&tables.functions))
        .map(|index| {
            (
                text_of(tables, tables.functions.name[index]),
                tables.functions.parameter_count[index] as usize,
            )
        })
        .collect()
}

fn runnable_names() -> Vec<&'static str> {
    NAMES
        .into_iter()
        .filter(|name| !is_synthetic(name))
        .collect()
}

#[test]
fn the_tables_describe_the_structs_the_compiler_resolved() {
    if !zig_is_available() {
        eprintln!("skipping: zig is not on PATH");
        return;
    }
    for name in runnable_names() {
        let reflected = reflect(name);
        let declared = declared_structs(&tables_for(name).expect("registered"));
        for (owner, expected) in &reflected.structs {
            let found = declared.get(owner).unwrap_or_else(|| {
                panic!("{name}: the tables have no struct {owner}, which the compiler resolved")
            });
            assert_eq!(
                found, expected,
                "{name}: the tables disagree with the compiler about {owner}"
            );
        }
        assert_eq!(
            declared.len(),
            reflected.structs.len(),
            "{name}: the tables declare structs the program does not"
        );
    }
}

#[test]
fn the_tables_describe_the_functions_the_compiler_resolved() {
    if !zig_is_available() {
        eprintln!("skipping: zig is not on PATH");
        return;
    }
    for name in runnable_names() {
        let reflected = reflect(name);
        let declared = declared_functions(&tables_for(name).expect("registered"));
        // Reflection reaches public declarations only, so a private helper is
        // in the tables and not here. The check runs one way for that reason.
        for (function, parameters) in &reflected.functions {
            let found = declared
                .get(function)
                .unwrap_or_else(|| panic!("{name}: the tables have no function {function}"));
            assert_eq!(
                found, parameters,
                "{name}: {function} takes a different number of parameters in the tables"
            );
        }
    }
}

#[test]
fn every_extern_struct_keeps_the_layout_its_assertions_claim() {
    if !zig_is_available() {
        eprintln!("skipping: zig is not on PATH");
        return;
    }
    let mut checked = 0;
    for name in runnable_names() {
        let reflected = reflect(name);
        let tables = tables_for(name).expect("registered");
        let declared = declared_structs(&tables);
        for (owner, entry) in &reflected.structs {
            if entry.layout != "extern" {
                continue;
            }
            checked += 1;
            let found = declared.get(owner).expect("checked by the struct test");
            assert_eq!(found.layout, "extern", "{name}: {owner} lost its C layout");
            assert_eq!(found.size, entry.size);
            assert_eq!(found.alignment, entry.alignment);
            assert_eq!(found.fields, entry.fields);
        }
    }
    assert!(checked > 0, "no example carries an extern struct");
}
