use zag_analysis::analyze;
use zag_analysis::ownership::{Confidence, EvidenceKind, OwnershipClass, field_evidence};
use zag_facts::fixture::example_tables;
use zag_facts::tables::{
    AssignmentSource, MemoryOperationKind, Tables, empty_tables, string_bytes,
};
use zag_facts::{FieldId, FunctionId, NO_INDEX};

fn field_named(tables: &Tables, owner: &[u8], field: &[u8]) -> FieldId {
    for row in 0..tables.fields.owner.len() {
        let owner_name = tables.structs.name[tables.fields.owner[row].0 as usize];
        if string_bytes(&tables.strings, owner_name) != owner {
            continue;
        }
        if string_bytes(&tables.strings, tables.fields.name[row]) == field {
            return FieldId(row as u32);
        }
    }
    panic!("the fixture has no field {owner:?}.{field:?}");
}

fn class_of(tables: &Tables, owner: &[u8], field: &[u8]) -> (OwnershipClass, Confidence) {
    let analysis = analyze(tables);
    let index = field_named(tables, owner, field).0 as usize;
    (
        analysis.ownership.class[index],
        analysis.ownership.confidence[index],
    )
}

#[test]
fn an_empty_program_classifies_nothing() {
    let analysis = analyze(&empty_tables());
    assert_eq!(analysis.ownership.class, Vec::new());
}

#[test]
fn a_scalar_field_is_a_value() {
    assert_eq!(
        class_of(&example_tables(), b"Buffer", b"length"),
        (OwnershipClass::Value, Confidence::High)
    );
    assert_eq!(
        class_of(&example_tables(), b"Header", b"magic"),
        (OwnershipClass::Value, Confidence::High)
    );
}

#[test]
fn a_field_allocated_globally_and_freed_from_deinit_is_owned() {
    assert_eq!(
        class_of(&example_tables(), b"Buffer", b"data"),
        (OwnershipClass::Owned, Confidence::High)
    );
}

#[test]
fn a_field_allocated_from_an_arena_is_arena_owned() {
    assert_eq!(
        class_of(&example_tables(), b"Node", b"label"),
        (OwnershipClass::Arena, Confidence::High)
    );
}

#[test]
fn a_field_assigned_only_a_literal_and_never_freed_is_static() {
    assert_eq!(
        class_of(&example_tables(), b"Node", b"children"),
        (OwnershipClass::Static, Confidence::High)
    );
}

#[test]
fn a_field_assigned_from_a_parameter_and_never_freed_is_borrowed() {
    assert_eq!(
        class_of(&example_tables(), b"View", b"bytes"),
        (OwnershipClass::Borrowed, Confidence::Medium)
    );
}

#[test]
fn a_field_with_no_evidence_at_all_is_unknown() {
    assert_eq!(
        class_of(&example_tables(), b"Cache", b"entries"),
        (OwnershipClass::Unknown, Confidence::Low)
    );
}

#[test]
fn a_field_with_no_evidence_says_so_in_its_evidence() {
    let tables = example_tables();
    let analysis = analyze(&tables);
    let field = field_named(&tables, b"Cache", b"entries");
    let kinds: Vec<EvidenceKind> = field_evidence(&analysis.ownership, field)
        .map(|row| analysis.ownership.evidence_kind[row])
        .collect();
    assert_eq!(kinds, vec![EvidenceKind::NoAssignmentsFound]);
}

#[test]
fn owned_evidence_names_the_function_that_frees_and_the_function_that_allocates() {
    let tables = example_tables();
    let analysis = analyze(&tables);
    let field = field_named(&tables, b"Buffer", b"data");
    let rows: Vec<(EvidenceKind, FunctionId)> = field_evidence(&analysis.ownership, field)
        .map(|row| {
            (
                analysis.ownership.evidence_kind[row],
                analysis.ownership.evidence_function[row],
            )
        })
        .collect();
    let names: Vec<(EvidenceKind, Vec<u8>)> = rows
        .iter()
        .map(|(kind, function)| {
            let name = if function.0 == NO_INDEX {
                Vec::new()
            } else {
                string_bytes(&tables.strings, tables.functions.name[function.0 as usize]).to_vec()
            };
            (*kind, name)
        })
        .collect();
    assert_eq!(
        names,
        vec![
            (EvidenceKind::FreedInDeinitClosure, b"release".to_vec()),
            (EvidenceKind::AssignedFromAllocation, b"init".to_vec()),
            (EvidenceKind::AllocatorIsGlobal, Vec::new()),
        ]
    );
}

#[test]
fn a_free_outside_the_deinit_closure_lowers_confidence() {
    let mut tables = example_tables();
    let field = field_named(&tables, b"Buffer", b"data");
    let free_row = tables
        .memory_operations
        .kind
        .iter()
        .position(|kind| *kind == MemoryOperationKind::Free)
        .expect("the fixture frees something");
    assert_eq!(tables.memory_operations.place_field[free_row], field);
    let unrelated = tables
        .functions
        .name
        .iter()
        .position(|name| string_bytes(&tables.strings, *name) == b"makeView")
        .expect("the fixture defines makeView");
    tables.memory_operations.function[free_row] = FunctionId(unrelated as u32);
    let analysis = analyze(&tables);
    assert_eq!(
        (
            analysis.ownership.class[field.0 as usize],
            analysis.ownership.confidence[field.0 as usize]
        ),
        (OwnershipClass::Owned, Confidence::Medium)
    );
}

#[test]
fn mixed_assignment_sources_fall_back_to_unknown() {
    let mut tables = example_tables();
    let field = field_named(&tables, b"Buffer", b"data");
    zag_facts::build::push_field_assignment(
        &mut tables,
        field,
        FunctionId(0),
        AssignmentSource::Parameter,
        zag_facts::MemoryOperationId(NO_INDEX),
    );
    let analysis = analyze(&tables);
    assert_eq!(
        (
            analysis.ownership.class[field.0 as usize],
            analysis.ownership.confidence[field.0 as usize]
        ),
        (OwnershipClass::Unknown, Confidence::Low)
    );
}

#[test]
fn an_allocation_whose_allocator_conflicts_is_not_reported_as_owned() {
    let mut tables = example_tables();
    let field = field_named(&tables, b"Buffer", b"data");
    let allocate_row = tables
        .memory_operations
        .kind
        .iter()
        .position(|kind| *kind == MemoryOperationKind::Allocate)
        .expect("the fixture allocates something");
    tables.memory_operations.allocator[allocate_row] = zag_facts::AllocatorSourceId(NO_INDEX);
    let analysis = analyze(&tables);
    assert_eq!(
        analysis.ownership.class[field.0 as usize],
        OwnershipClass::Unknown
    );
}

#[test]
fn every_field_gets_exactly_one_classification() {
    let tables = example_tables();
    let analysis = analyze(&tables);
    assert_eq!(analysis.ownership.class.len(), tables.fields.owner.len());
    assert_eq!(
        analysis.ownership.confidence.len(),
        tables.fields.owner.len()
    );
    assert_eq!(
        analysis.ownership.evidence_start.len(),
        tables.fields.owner.len()
    );
}

#[test]
fn evidence_ranges_tile_the_evidence_table_without_gaps() {
    let tables = example_tables();
    let analysis = analyze(&tables);
    let mut expected = 0u32;
    for row in 0..tables.fields.owner.len() {
        assert_eq!(analysis.ownership.evidence_start[row], expected);
        expected += analysis.ownership.evidence_count[row];
    }
    assert_eq!(expected as usize, analysis.ownership.evidence_kind.len());
    assert_eq!(
        analysis.ownership.evidence_kind.len(),
        analysis.ownership.evidence_function.len()
    );
}

#[test]
fn analysis_is_deterministic() {
    let tables = example_tables();
    assert_eq!(analyze(&tables), analyze(&tables));
}
