use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the crate lives two directories under the workspace root")
        .to_path_buf()
}

fn crate_manifests() -> Vec<PathBuf> {
    let crates = workspace_root().join("crates");
    let mut manifests: Vec<PathBuf> = std::fs::read_dir(&crates)
        .expect("the crates directory exists")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path().join("Cargo.toml"))
        .filter(|path| path.is_file())
        .collect();
    manifests.sort();
    manifests
}

#[test]
fn there_is_more_than_one_crate_to_check() {
    assert!(crate_manifests().len() > 1);
}

#[test]
fn every_crate_inherits_the_workspace_lints() {
    for manifest in crate_manifests() {
        let text = std::fs::read_to_string(&manifest).expect("the manifest is readable");
        let inherits = text.contains("[lints]") && text.contains("workspace = true");
        assert!(
            inherits,
            "{} must inherit the workspace lints, which is what forbids unsafe code",
            manifest.display()
        );
    }
}

#[test]
fn the_workspace_forbids_unsafe_code() {
    let manifest = workspace_root().join("Cargo.toml");
    let text = std::fs::read_to_string(&manifest).expect("the manifest is readable");
    assert!(text.contains("unsafe_code = \"forbid\""), "{text}");
}

fn source_files() -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![workspace_root().join("crates")];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.filter_map(|entry| entry.ok()) {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|kind| kind == "rs") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

fn code_before_any_string_literal(line: &str) -> &str {
    match line.find('"') {
        Some(quote) => &line[..quote],
        None => line,
    }
}

#[test]
fn no_source_file_writes_the_unsafe_keyword() {
    for path in source_files() {
        let text = std::fs::read_to_string(&path).expect("the source is readable");
        for (number, line) in text.lines().enumerate() {
            let code = code_before_any_string_literal(line);
            assert!(
                !code.split_whitespace().any(|word| word == "unsafe"),
                "{}:{} writes the unsafe keyword",
                path.display(),
                number + 1
            );
        }
    }
}

#[test]
fn the_unsafe_check_looks_past_string_literals_but_not_past_code() {
    assert_eq!(
        code_before_any_string_literal("let text = \"unsafe\";"),
        "let text = "
    );
    assert_eq!(code_before_any_string_literal("unsafe { }"), "unsafe { }");
}

/// The coverage example is the one program that reaches every ownership class,
/// so a declaration going missing from it silently narrows what the suite
/// covers.
#[test]
fn the_coverage_example_still_reaches_every_ownership_class() {
    let source = workspace_root()
        .join("examples")
        .join("coverage")
        .join("src")
        .join("main.zig");
    let text = std::fs::read_to_string(&source).expect("the coverage source is readable");
    for name in ["Buffer", "Header", "Node", "View", "Cache"] {
        assert!(
            text.contains(name),
            "the coverage example must still declare {name}"
        );
    }
}
