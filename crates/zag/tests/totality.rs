use proptest::prelude::*;
use zag_facts::fixture::example_tables;
use zag_facts::tables::Tables;

const COLUMNS: usize = 20;

fn corrupt(tables: &mut Tables, column: usize, value: u32) {
    match column % COLUMNS {
        0 => tables.structs.field_start[0] = value,
        1 => tables.structs.field_count[0] = value,
        2 => tables.functions.parameter_start[0] = value,
        3 => tables.functions.parameter_count[0] = value,
        4 => tables.fields.owner[0] = zag_facts::StructId(value),
        5 => tables.fields.field_type[0] = zag_facts::TypeId(value),
        6 => tables.fields.name[0] = zag_facts::StringId(value),
        7 => tables.calls.caller[0] = zag_facts::FunctionId(value),
        8 => tables.calls.callee[0] = zag_facts::FunctionId(value),
        9 => tables.memory_operations.place_field[0] = zag_facts::FieldId(value),
        10 => tables.memory_operations.function[0] = zag_facts::FunctionId(value),
        11 => tables.memory_operations.allocator[0] = zag_facts::AllocatorSourceId(value),
        12 => tables.field_assignments.field[0] = zag_facts::FieldId(value),
        13 => tables.field_assignments.function[0] = zag_facts::FunctionId(value),
        14 => tables.field_assignments.memory_operation[0] = zag_facts::MemoryOperationId(value),
        15 => tables.allocator_sources.function[0] = zag_facts::FunctionId(value),
        16 => tables.allocator_sources.parameter_index[0] = value,
        17 => tables.call_arguments.call[0] = zag_facts::CallId(value),
        18 => tables.call_arguments.parameter_index[0] = value,
        _ => tables.structs.name[0] = zag_facts::StringId(value),
    }
}

fn truncate(tables: &mut Tables, column: usize, keep: usize) {
    match column % COLUMNS {
        0 => tables.structs.field_start.truncate(keep),
        1 => tables.structs.field_count.truncate(keep),
        2 => tables.structs.name.truncate(keep),
        3 => tables.structs.size.truncate(keep),
        4 => tables.structs.alignment.truncate(keep),
        5 => tables.structs.flags.truncate(keep),
        6 => tables.structs.deinit.truncate(keep),
        7 => tables.fields.name.truncate(keep),
        8 => tables.fields.field_type.truncate(keep),
        9 => tables.fields.offset.truncate(keep),
        10 => tables.functions.parameter_start.truncate(keep),
        11 => tables.functions.parameter_count.truncate(keep),
        12 => tables.functions.name.truncate(keep),
        13 => tables.calls.callee.truncate(keep),
        14 => tables.memory_operations.function.truncate(keep),
        15 => tables.memory_operations.allocator.truncate(keep),
        16 => tables.field_assignments.source.truncate(keep),
        17 => tables.types.element.truncate(keep),
        18 => tables.types.bit_width.truncate(keep),
        _ => tables.types.name.truncate(keep),
    }
}

/// The passes run before the validator in every test that damages a table on
/// purpose, so they have to be total on their own rather than leaning on it.
fn run_every_pass(tables: &Tables) {
    let analysis = zag_analysis::analyze(tables);
    let _ = zag_emit::generate(tables, &analysis);
}

#[test]
fn the_undamaged_fixture_runs_every_pass() {
    run_every_pass(&example_tables());
}

proptest! {
    #[test]
    fn the_pipeline_never_panics_on_arbitrary_bytes(
        bytes in prop::collection::vec(any::<u8>(), 0..512),
    ) {
        let _ = zag::generate_from_bytes(&bytes);
    }

    #[test]
    fn the_pipeline_never_panics_on_a_prefix_of_a_real_fact_file(length in 0usize..2048) {
        let bytes = zag_facts::wire::encode(&example_tables());
        let _ = zag::generate_from_bytes(&bytes[..length.min(bytes.len())]);
    }

    #[test]
    fn every_pass_survives_a_corrupted_handle(column in 0usize..COLUMNS, value in any::<u32>()) {
        let mut tables = example_tables();
        corrupt(&mut tables, column, value);
        run_every_pass(&tables);
    }

    #[test]
    fn every_pass_survives_a_truncated_column(column in 0usize..COLUMNS, keep in 0usize..6) {
        let mut tables = example_tables();
        truncate(&mut tables, column, keep);
        run_every_pass(&tables);
    }

    #[test]
    fn every_pass_survives_two_damaged_columns(
        first in 0usize..COLUMNS,
        second in 0usize..COLUMNS,
        value in any::<u32>(),
        keep in 0usize..6,
    ) {
        let mut tables = example_tables();
        corrupt(&mut tables, first, value);
        truncate(&mut tables, second, keep);
        run_every_pass(&tables);
    }

    #[test]
    fn the_validator_agrees_with_itself_across_the_wire(
        column in 0usize..COLUMNS,
        value in any::<u32>(),
    ) {
        let mut tables = example_tables();
        corrupt(&mut tables, column, value);
        let bytes = zag_facts::wire::encode(&tables);
        let decoded = zag_facts::wire::decode(&bytes).expect("column values do not stop a decode");
        prop_assert_eq!(
            zag_facts::validate::validate(&tables).is_ok(),
            zag_facts::validate::validate(&decoded).is_ok()
        );
    }
}
