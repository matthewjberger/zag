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
            | EvidenceKind::NoAssignmentsFound => {}
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
    ];
    for class in &all {
        match class {
            OwnershipClass::Value
            | OwnershipClass::Owned
            | OwnershipClass::Borrowed
            | OwnershipClass::Static
            | OwnershipClass::Arena
            | OwnershipClass::Unknown => {}
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

#[test]
fn the_readme_points_at_the_guide() {
    let readme = std::fs::read_to_string(workspace_root().join("README.md"))
        .expect("the readme is readable");
    assert!(readme.contains("docs/PORTING.md"));
}
