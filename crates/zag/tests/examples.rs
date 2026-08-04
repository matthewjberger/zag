use std::path::{Path, PathBuf};
use zag_facts::examples::{NAMES, tables_for};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the crate lives two directories under the workspace root")
        .to_path_buf()
}

fn example_directory(name: &str) -> PathBuf {
    workspace_root().join("examples").join(name)
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|cause| panic!("{}: {cause}", path.display()))
        .replace("\r\n", "\n")
}

fn ported(name: &str) -> zag_emit::Output {
    let tables = tables_for(name).unwrap_or_else(|| panic!("{name} has no fact tables"));
    zag::generate(&tables).unwrap_or_else(|cause| panic!("{name}: {}", zag::describe(&cause)))
}

fn runnable_names() -> Vec<&'static str> {
    NAMES.to_vec()
}

#[test]
fn there_are_examples_to_check() {
    assert!(runnable_names().len() >= 4);
}

#[test]
fn every_example_carries_a_zig_project_that_stands_on_its_own() {
    for name in runnable_names() {
        let directory = example_directory(name);
        for required in ["build.zig", "build.zig.zon", "src/main.zig"] {
            let path = directory.join(required);
            assert!(
                path.is_file(),
                "{name} is missing {required}, so it cannot be built on its own"
            );
        }
    }
}

#[test]
fn every_example_directory_has_fact_tables() {
    let directory = workspace_root().join("examples");
    let found: Vec<String> = std::fs::read_dir(&directory)
        .expect("the examples directory exists")
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    for name in &found {
        assert!(
            tables_for(name).is_some(),
            "examples/{name} has no entry in zag_facts::examples"
        );
    }
    for name in runnable_names() {
        assert!(
            found.iter().any(|entry| entry == name),
            "{name} is registered but has no directory under examples"
        );
    }
}

#[test]
fn every_example_ports_to_the_output_checked_in_beside_it() {
    for name in runnable_names() {
        let output = ported(name);
        let directory = example_directory(name).join("expected");
        let source = String::from_utf8(output.source).expect("the port is text");
        let report = String::from_utf8(output.report).expect("the report is text");
        assert_eq!(
            source,
            read(&directory.join("port.rs")),
            "{name} port drifted, regenerate with `just examples`"
        );
        assert_eq!(
            report,
            read(&directory.join("port.report.txt")),
            "{name} report drifted, regenerate with `just examples`"
        );
    }
}

#[test]
fn every_example_survives_the_wire_format_unchanged() {
    for name in runnable_names() {
        let tables = tables_for(name).expect("registered");
        let bytes = zag_facts::wire::encode(&tables);
        let decoded = zag_facts::wire::decode(&bytes).expect("a table set it just wrote");
        assert_eq!(tables, decoded, "{name} does not survive the wire format");
    }
}

#[test]
fn every_example_produces_well_formed_tables() {
    for name in NAMES {
        let tables = tables_for(name).expect("registered");
        assert_eq!(
            zag_facts::validate::validate(&tables),
            Ok(()),
            "{name} builds tables the validator rejects"
        );
    }
}

#[test]
fn every_example_settles_its_allocator_provenance() {
    for name in NAMES {
        let tables = tables_for(name).expect("registered");
        let analysis = zag_analysis::analyze(&tables);
        assert!(
            analysis.provenance.converged,
            "{name} does not reach a fixed point"
        );
    }
}

#[test]
fn porting_an_example_is_deterministic() {
    for name in runnable_names() {
        assert_eq!(ported(name), ported(name));
    }
}

#[test]
fn the_examples_reach_every_ownership_class() {
    use zag_analysis::ownership::OwnershipClass;
    let mut seen = Vec::new();
    for name in runnable_names() {
        let tables = tables_for(name).expect("registered");
        for class in zag_analysis::analyze(&tables).ownership.class {
            if !seen.contains(&class) {
                seen.push(class);
            }
        }
    }
    for class in [
        OwnershipClass::Value,
        OwnershipClass::Owned,
        OwnershipClass::Borrowed,
        OwnershipClass::Static,
        OwnershipClass::Arena,
        OwnershipClass::Unknown,
    ] {
        assert!(
            seen.contains(&class),
            "no runnable example produces {class:?}, so nothing exercises it end to end"
        );
    }
}

#[test]
fn the_examples_readme_describes_each_one() {
    let readme = read(&workspace_root().join("examples").join("README.md"));
    for name in runnable_names() {
        assert!(
            readme.contains(name),
            "examples/README.md does not mention {name}"
        );
    }
}
