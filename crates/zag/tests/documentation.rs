use std::path::{Path, PathBuf};
use zag_analysis::ownership::{Confidence, EvidenceKind, OwnershipClass};
use zag_emit::report::render_report;
use zag_facts::fixture::example_tables;
use zag_facts::{FunctionId, NO_INDEX};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the crate lives two directories under the workspace root")
        .to_path_buf()
}

fn porting_guide() -> String {
    let path = workspace_root().join("docs").join("PORTING.md");
    std::fs::read_to_string(&path).expect("the porting guide is readable")
}

/// The match proves the list is complete. Adding a variant without adding it
/// here stops this file compiling, which is what keeps the guide in step.
fn every_evidence_kind() -> Vec<EvidenceKind> {
    let all = vec![
        EvidenceKind::FreedInDeinitClosure,
        EvidenceKind::FreedOutsideDeinitClosure,
        EvidenceKind::AssignedFromAllocation,
        EvidenceKind::AssignedFromParameter,
        EvidenceKind::AssignedFromLiteral,
        EvidenceKind::AssignedFromUnknown,
        EvidenceKind::AllocatorIsGlobal,
        EvidenceKind::AllocatorIsArena,
        EvidenceKind::AllocatorIsConflicting,
        EvidenceKind::NoAssignmentsFound,
        EvidenceKind::ResizedAfterAllocation,
        EvidenceKind::AlignmentCannotBeCarried,
    ];
    for kind in &all {
        match kind {
            EvidenceKind::FreedInDeinitClosure
            | EvidenceKind::FreedOutsideDeinitClosure
            | EvidenceKind::AssignedFromAllocation
            | EvidenceKind::AssignedFromParameter
            | EvidenceKind::AssignedFromLiteral
            | EvidenceKind::AssignedFromUnknown
            | EvidenceKind::AllocatorIsGlobal
            | EvidenceKind::AllocatorIsArena
            | EvidenceKind::AllocatorIsConflicting
            | EvidenceKind::NoAssignmentsFound
            | EvidenceKind::ResizedAfterAllocation
            | EvidenceKind::AlignmentCannotBeCarried => {}
        }
    }
    all
}

fn every_ownership_class() -> Vec<OwnershipClass> {
    let all = vec![
        OwnershipClass::Value,
        OwnershipClass::Owned,
        OwnershipClass::Borrowed,
        OwnershipClass::Static,
        OwnershipClass::Arena,
        OwnershipClass::Unknown,
        OwnershipClass::Grown,
    ];
    for class in &all {
        match class {
            OwnershipClass::Value
            | OwnershipClass::Owned
            | OwnershipClass::Borrowed
            | OwnershipClass::Static
            | OwnershipClass::Arena
            | OwnershipClass::Unknown
            | OwnershipClass::Grown => {}
        }
    }
    all
}

fn every_confidence() -> Vec<Confidence> {
    let all = vec![Confidence::High, Confidence::Medium, Confidence::Low];
    for confidence in &all {
        match confidence {
            Confidence::High | Confidence::Medium | Confidence::Low => {}
        }
    }
    all
}

fn report_with_all_evidence() -> String {
    let tables = example_tables();
    let mut analysis = zag_analysis::analyze(&tables);
    let kinds = every_evidence_kind();
    let count = kinds.len() as u32;
    analysis.ownership.evidence_function = vec![FunctionId(NO_INDEX); kinds.len()];
    analysis.ownership.evidence_kind = kinds;
    for row in 0..analysis.ownership.evidence_start.len() {
        analysis.ownership.evidence_start[row] = if row == 0 { 0 } else { count };
        analysis.ownership.evidence_count[row] = if row == 0 { count } else { 0 };
    }
    for (row, class) in every_ownership_class().into_iter().enumerate() {
        if row < analysis.ownership.class.len() {
            analysis.ownership.class[row] = class;
        }
    }
    for (row, confidence) in every_confidence().into_iter().enumerate() {
        if row < analysis.ownership.confidence.len() {
            analysis.ownership.confidence[row] = confidence;
        }
    }
    String::from_utf8(render_report(&tables, &analysis)).expect("the report is text")
}

#[test]
fn the_guide_explains_every_evidence_line_the_report_can_print() {
    let guide = porting_guide();
    let report = report_with_all_evidence();
    let lines: Vec<&str> = report
        .lines()
        .filter_map(|line| line.trim().strip_prefix("evidence: "))
        .collect();
    assert_eq!(lines.len(), every_evidence_kind().len());
    for line in lines {
        let phrase = line.split(" (").next().expect("a phrase");
        assert!(
            guide.contains(phrase),
            "docs/PORTING.md does not explain the evidence line {phrase:?}"
        );
    }
}

#[test]
fn the_guide_explains_every_ownership_class_the_report_can_print() {
    let guide = porting_guide();
    let report = report_with_all_evidence();
    let classes: Vec<&str> = report
        .lines()
        .filter_map(|line| line.trim().strip_prefix("class: "))
        .collect();
    assert!(!classes.is_empty());
    for class in classes {
        assert!(
            guide.contains(&format!("`{class}`")),
            "docs/PORTING.md does not explain the class {class:?}"
        );
    }
}

#[test]
fn the_guide_explains_every_confidence_the_report_can_print() {
    let guide = porting_guide();
    let report = report_with_all_evidence();
    let levels: Vec<&str> = report
        .lines()
        .filter_map(|line| line.trim().strip_prefix("confidence: "))
        .collect();
    assert!(!levels.is_empty());
    for level in levels {
        assert!(
            guide.contains(&format!("`{level}`")),
            "docs/PORTING.md does not explain the confidence {level:?}"
        );
    }
}

#[test]
fn the_guide_names_every_lifetime_the_emitter_can_introduce() {
    let guide = porting_guide();
    for lifetime in ["'a", "'bump", "'static"] {
        assert!(
            guide.contains(lifetime),
            "docs/PORTING.md does not mention the {lifetime} lifetime"
        );
    }
}

fn documents() -> Vec<(&'static str, String)> {
    let root = workspace_root();
    [
        "README.md",
        "CLAUDE.md",
        "docs/PORTING.md",
        "examples/README.md",
    ]
    .into_iter()
    .map(|name| {
        let text = std::fs::read_to_string(root.join(name))
            .unwrap_or_else(|cause| panic!("{name}: {cause}"));
        (name, text)
    })
    .collect()
}

fn recipes() -> Vec<String> {
    let text = std::fs::read_to_string(workspace_root().join("justfile"))
        .expect("the justfile is readable");
    text.lines()
        .filter(|line| !line.starts_with([' ', '\t', '#', '[']))
        .filter(|line| !line.contains(":=") && !line.starts_with("set ") && line.contains(':'))
        .filter_map(|line| {
            let line = line.strip_prefix('@').unwrap_or(line);
            let head = line.split(':').next()?;
            let name = head.split_whitespace().next()?;
            name.chars()
                .all(|letter| letter.is_ascii_lowercase() || letter == '-')
                .then(|| name.to_string())
        })
        .collect()
}

/// Every `just something` the documentation tells a reader to run has to be a
/// recipe. A renamed recipe leaves instructions that fail on the first try.
fn mentioned_recipes(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("just ")
            && let Some(name) = rest.split_whitespace().next()
        {
            found.push(name.to_string());
        }
        let mut cursor = line;
        while let Some(start) = cursor.find("`just ") {
            let rest = &cursor[start + 6..];
            let end = rest.find('`').unwrap_or(rest.len());
            if let Some(name) = rest[..end].split_whitespace().next() {
                found.push(name.to_string());
            }
            cursor = &rest[end.min(rest.len())..];
        }
    }
    found
}

#[test]
fn there_are_recipes_to_check() {
    let recipes = recipes();
    assert!(recipes.contains(&"port".to_string()), "{recipes:?}");
    assert!(recipes.contains(&"check".to_string()), "{recipes:?}");
}

#[test]
fn every_recipe_the_documentation_names_exists() {
    let recipes = recipes();
    let mut checked = 0;
    for (name, text) in documents() {
        for mentioned in mentioned_recipes(&text) {
            checked += 1;
            assert!(
                recipes.contains(&mentioned),
                "{name} tells the reader to run `just {mentioned}`, which is not a recipe"
            );
        }
    }
    // Without this the check would pass on documentation that names no recipe
    // at all, which is the state a broken scan produces.
    assert!(checked > 8, "only {checked} recipe mentions were found");
}

#[test]
fn the_readme_shows_layout_assertions_the_emitter_actually_writes() {
    let port = std::fs::read_to_string(
        workspace_root()
            .join("examples")
            .join("netpacket")
            .join("expected")
            .join("port.rs"),
    )
    .expect("the netpacket port is readable");
    let readme = std::fs::read_to_string(workspace_root().join("README.md"))
        .expect("the readme is readable");
    let shown: Vec<&str> = readme
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("const _: () = assert!"))
        .collect();
    assert!(!shown.is_empty(), "the README should show what it claims");
    for line in shown {
        assert!(
            port.contains(line),
            "the README shows an assertion no port carries: {line}"
        );
    }
}

/// The match proves the list is complete, so a new outcome stops this file
/// compiling until the guide explains it.
fn every_disposition() -> Vec<zag_emit::report::Disposition> {
    use zag_emit::function::Refusal;
    use zag_emit::report::Disposition;
    let all = vec![
        Disposition::Constructor,
        Disposition::SubsumedByDrop,
        Disposition::Ported,
        Disposition::Signature,
        Disposition::NotPorted(Refusal::ReturnTypeUnresolved),
        Disposition::NotPorted(Refusal::UnnamedErrorSet),
        Disposition::NotPorted(Refusal::ReturnBorrowsAnArena),
        Disposition::NotPorted(Refusal::ReturnBorrowsWithNothingToTieItTo),
    ];
    for outcome in &all {
        match outcome {
            Disposition::Constructor
            | Disposition::SubsumedByDrop
            | Disposition::Ported
            | Disposition::Signature => {}
            Disposition::NotPorted(refusal) => match refusal {
                Refusal::ReturnTypeUnresolved
                | Refusal::UnnamedErrorSet
                | Refusal::ReturnBorrowsAnArena
                | Refusal::ReturnBorrowsWithNothingToTieItTo => {}
            },
        }
    }
    all
}

/// The wording is read out of the emitter rather than repeated here, where it
/// could drift from the code.
#[test]
fn the_guide_explains_every_outcome_a_function_can_reach() {
    let guide = porting_guide();
    for outcome in every_disposition() {
        let text = String::from_utf8(zag_emit::report::outcome_text(outcome).to_vec())
            .expect("the wording is text");
        assert!(
            guide.contains(&text),
            "docs/PORTING.md does not explain the outcome {text:?}"
        );
    }
}

/// Every outcome the examples between them reach has to be one the guide
/// explains, which is the other half of the check above: one proves the guide
/// covers what the code can say, this proves it covers what it does say.
#[test]
fn the_guide_explains_every_outcome_the_examples_produce() {
    let guide = porting_guide();
    for name in zag_facts::examples::NAMES {
        let tables = zag_facts::examples::tables_for(name).expect("registered");
        let analysis = zag_analysis::analyze(&tables);
        let report = String::from_utf8(render_report(&tables, &analysis)).expect("text");
        for line in report
            .lines()
            .skip_while(|line| !line.starts_with("functions:"))
            .skip(1)
            .take_while(|line| line.starts_with("  "))
            // The outcome per function is indented two. A line indented four
            // is the reason a constructor was refused, and it names a field,
            // so the guide explains its shape rather than its text.
            .filter(|line| !line.starts_with("    "))
        {
            let Some((_, outcome)) = line.trim_start().split_once(": ") else {
                continue;
            };
            // The trailing parenthesis is where the Zig was written, which the
            // guide explains as a shape rather than a literal file and line.
            let outcome = outcome.split(" (").next().unwrap_or(outcome);
            assert!(
                guide.contains(outcome),
                "docs/PORTING.md does not explain {outcome:?}, which {name} produces"
            );
        }
    }
}

#[test]
fn the_readme_points_at_the_guide() {
    let readme = std::fs::read_to_string(workspace_root().join("README.md"))
        .expect("the readme is readable");
    assert!(readme.contains("docs/PORTING.md"));
}
