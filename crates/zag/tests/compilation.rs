//! Ports each example and hands the result straight to rustc. `zag-verify`
//! compiles the output checked in beside each program, which is a different
//! claim: this one compiles what the pipeline produced a moment ago, so an
//! emitter that starts writing Rust that does not build fails here even when
//! nobody has regenerated anything.
//!
//! Metadata is the only thing emitted, so there is no code generation and no
//! linking, but constants are still evaluated. That is what makes the layout
//! assertions the emitter wrote part of the check rather than decoration.

use std::path::{Path, PathBuf};
use std::process::Command;
use zag_facts::examples::{NAMES, tables_for};

fn scratch(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("zag-compile-{name}"))
}

fn compile(name: &str, source: &[u8]) -> Result<(), String> {
    let directory = scratch(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).map_err(|cause| cause.to_string())?;
    let path = directory.join("port.rs");
    std::fs::write(&path, source).map_err(|cause| cause.to_string())?;
    let outcome = build(&path, &directory);
    let _ = std::fs::remove_dir_all(&directory);
    outcome
}

fn build(path: &Path, directory: &Path) -> Result<(), String> {
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

fn ported(name: &str) -> Vec<u8> {
    let tables = tables_for(name).unwrap_or_else(|| panic!("{name} has no fact tables"));
    zag::generate(&tables)
        .unwrap_or_else(|cause| panic!("{name}: {}", zag::describe(&cause)))
        .source
}

#[test]
fn every_port_the_pipeline_produces_compiles() {
    for name in NAMES {
        let source = ported(name);
        if let Err(complaint) = compile(name, &source) {
            panic!(
                "{name}: the port does not compile\n{}\n{complaint}",
                String::from_utf8_lossy(&source)
            );
        }
    }
}

#[test]
fn the_layout_assertions_are_evaluated_rather_than_ignored() {
    // Without this the test above would pass on a port whose assertions are
    // all false, which would make the whole check decorative.
    let source = String::from_utf8(ported("netpacket")).expect("the port is text");
    assert!(
        source.contains("size_of::<Header>() == 12"),
        "netpacket should assert its header size:\n{source}"
    );
    let broken = source.replace("size_of::<Header>() == 12", "size_of::<Header>() == 13");
    let complaint = compile("negative-control", broken.as_bytes())
        .expect_err("a false layout assertion has to fail the build");
    assert!(
        complaint.contains("evaluation"),
        "the failure should come from evaluating the constant:\n{complaint}"
    );
}

#[test]
fn a_port_that_names_a_type_that_does_not_exist_fails_the_check() {
    let complaint = compile(
        "missing-type",
        b"pub struct Holder { pub value: NoSuchType }",
    )
    .expect_err("an unknown type has to fail the build");
    assert!(complaint.contains("NoSuchType"), "{complaint}");
}

#[test]
fn every_port_is_free_of_lifetimes_it_does_not_declare() {
    // A struct that names another struct has to supply the lifetimes that one
    // declares, which is the mistake the tokenizer example first caught.
    for name in NAMES {
        let source = String::from_utf8(ported(name)).expect("the port is text");
        for lifetime in ["'a", "'bump"] {
            let uses = source.matches(lifetime).count();
            if uses == 0 {
                continue;
            }
            assert!(
                source.contains(&format!("<{lifetime}>"))
                    || source.contains(&format!("<'a, {lifetime}>"))
                    || source.contains(&format!("<{lifetime}, ")),
                "{name} uses {lifetime} without declaring it:\n{source}"
            );
        }
    }
}
