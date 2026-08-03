use zag_analysis::analyze;
use zag_analysis::call_graph::{build_call_graph, callees, reachable_from};
use zag_analysis::ownership::{Confidence, OwnershipClass};
use zag_analysis::provenance::{AllocatorClass, classify_source, resolve_allocator_provenance};
use zag_facts::fixture::example_tables;
use zag_facts::tables::{AllocatorSourceKind, MemoryOperationKind, Tables, string_bytes};
use zag_facts::{AllocatorSourceId, FieldId, FunctionId, NO_INDEX};

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

#[test]
fn a_call_from_a_function_that_does_not_exist_is_dropped_rather_than_panicking() {
    let mut tables = example_tables();
    tables.calls.caller[0] = FunctionId(9999);
    let graph = build_call_graph(&tables);
    assert_eq!(graph.edge_target.len(), tables.calls.caller.len() - 1);
    assert_eq!(graph.edge_call.len(), graph.edge_target.len());
}

#[test]
fn a_call_to_a_function_that_does_not_exist_still_builds_an_edge() {
    let mut tables = example_tables();
    tables.calls.callee[0] = FunctionId(9999);
    let graph = build_call_graph(&tables);
    assert_eq!(graph.edge_target.len(), tables.calls.caller.len());
    let reachable = reachable_from(&graph, tables.calls.caller[0]);
    assert_eq!(reachable.len(), tables.functions.name.len());
}

#[test]
fn a_short_callee_column_does_not_panic_the_call_graph() {
    let mut tables = example_tables();
    tables.calls.callee.clear();
    let graph = build_call_graph(&tables);
    assert_eq!(graph.edge_target.len(), 0);
    assert_eq!(*graph.edge_start.last().expect("a terminator"), 0);
}

#[test]
fn the_edge_ranges_always_tile_the_edge_table() {
    let mut tables = example_tables();
    tables.calls.caller[0] = FunctionId(9999);
    let graph = build_call_graph(&tables);
    assert_eq!(graph.edge_start[0], 0);
    assert_eq!(
        *graph.edge_start.last().expect("a terminator") as usize,
        graph.edge_target.len()
    );
    for function in 0..tables.functions.name.len() {
        let range = callees(&graph, FunctionId(function as u32));
        assert!(range.end <= graph.edge_target.len());
    }
}

#[test]
fn ragged_call_argument_columns_do_not_panic_provenance() {
    let mut tables = example_tables();
    tables.call_arguments.parameter_index.clear();
    let provenance = resolve_allocator_provenance(&tables);
    assert!(provenance.converged);

    let mut tables = example_tables();
    tables.call_arguments.source.truncate(1);
    let _ = resolve_allocator_provenance(&tables);
}

#[test]
fn ragged_allocator_source_columns_do_not_panic_provenance() {
    let mut tables = example_tables();
    tables.allocator_sources.function.clear();
    let provenance = resolve_allocator_provenance(&tables);
    assert_eq!(
        classify_source(&tables, &provenance, AllocatorSourceId(2)),
        AllocatorClass::Conflicting
    );
}

#[test]
fn a_parameter_start_past_the_parameter_table_resolves_to_conflicting() {
    let mut tables = example_tables();
    tables.functions.parameter_start[0] = 9999;
    let provenance = resolve_allocator_provenance(&tables);
    let source = tables
        .allocator_sources
        .kind
        .iter()
        .position(|kind| *kind == AllocatorSourceKind::Parameter)
        .expect("the fixture forwards an allocator");
    assert_eq!(
        classify_source(&tables, &provenance, AllocatorSourceId(source as u32)),
        AllocatorClass::Conflicting
    );
}

#[test]
fn a_short_function_table_does_not_panic_any_pass() {
    let mut tables = example_tables();
    tables.functions.parameter_count.clear();
    let analysis = analyze(&tables);
    assert_eq!(analysis.ownership.class.len(), tables.fields.owner.len());
}

#[test]
fn an_arena_field_that_is_also_freed_loses_confidence() {
    let mut tables = example_tables();
    let label = field_named(&tables, b"Node", b"label");
    let free_row = tables
        .memory_operations
        .kind
        .iter()
        .position(|kind| *kind == MemoryOperationKind::Free)
        .expect("the fixture frees something");
    tables.memory_operations.place_field[free_row] = label;
    let analysis = analyze(&tables);
    let slot = label.0 as usize;
    assert_eq!(analysis.ownership.class[slot], OwnershipClass::Arena);
    assert_eq!(analysis.ownership.confidence[slot], Confidence::Medium);
}

#[test]
fn an_arena_field_that_is_never_freed_keeps_full_confidence() {
    let tables = example_tables();
    let label = field_named(&tables, b"Node", b"label");
    let analysis = analyze(&tables);
    let slot = label.0 as usize;
    assert_eq!(analysis.ownership.class[slot], OwnershipClass::Arena);
    assert_eq!(analysis.ownership.confidence[slot], Confidence::High);
}

#[test]
fn an_absent_allocator_source_resolves_to_conflicting() {
    let tables = example_tables();
    let provenance = resolve_allocator_provenance(&tables);
    assert_eq!(
        classify_source(&tables, &provenance, AllocatorSourceId(NO_INDEX)),
        AllocatorClass::Conflicting
    );
}
