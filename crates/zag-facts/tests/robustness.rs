use proptest::prelude::*;
use zag_facts::build::{intern, push_string};
use zag_facts::fixture::example_tables;
use zag_facts::tables::{Strings, Tables, empty_tables, string_bytes, string_count};
use zag_facts::validate::{Violation, validate};
use zag_facts::wire::{decode, encode};
use zag_facts::{NO_INDEX, StringId};

#[test]
fn appending_does_not_deduplicate_and_interning_does() {
    let mut appended = Strings::default();
    push_string(&mut appended, b"alpha");
    push_string(&mut appended, b"alpha");
    assert_eq!(string_count(&appended), 2);

    let mut interned = Strings::default();
    intern(&mut interned, b"alpha");
    intern(&mut interned, b"alpha");
    assert_eq!(string_count(&interned), 1);
}

#[test]
fn appending_still_round_trips_each_identifier() {
    let mut strings = Strings::default();
    let first = push_string(&mut strings, b"alpha");
    let second = push_string(&mut strings, b"alpha");
    assert_ne!(first, second);
    assert_eq!(string_bytes(&strings, first), b"alpha");
    assert_eq!(string_bytes(&strings, second), b"alpha");
}

#[test]
fn a_backwards_offset_pair_reads_as_empty_rather_than_panicking() {
    let mut strings = Strings::default();
    intern(&mut strings, b"alpha");
    intern(&mut strings, b"beta");
    strings.offsets.swap(1, 2);
    assert_eq!(string_bytes(&strings, StringId(1)), b"");
}

#[test]
fn an_offset_past_the_blob_reads_as_empty_rather_than_panicking() {
    let mut strings = Strings::default();
    intern(&mut strings, b"alpha");
    let last = strings.offsets.len() - 1;
    strings.offsets[last] = 500;
    assert_eq!(string_bytes(&strings, StringId(0)), b"");
}

#[test]
fn a_field_range_that_overflows_is_reported_rather_than_panicking() {
    let mut tables = example_tables();
    tables.structs.field_start[0] = u32::MAX;
    tables.structs.field_count[0] = 5;
    let violations = validate(&tables).expect_err("an overflowing range must be reported");
    assert!(
        violations
            .iter()
            .any(|violation| matches!(violation, Violation::FieldRangeMismatch { .. }))
    );
}

#[test]
fn a_parameter_range_that_overflows_is_reported_rather_than_panicking() {
    let mut tables = example_tables();
    tables.functions.parameter_start[0] = u32::MAX;
    tables.functions.parameter_count[0] = 5;
    let violations = validate(&tables).expect_err("an overflowing range must be reported");
    assert!(
        violations
            .iter()
            .any(|violation| matches!(violation, Violation::ParameterRangeMismatch { .. }))
    );
}

#[test]
fn ragged_columns_do_not_panic_the_structural_checks() {
    let mut tables = example_tables();
    tables.structs.field_start.clear();
    assert!(validate(&tables).is_err());

    let mut tables = example_tables();
    tables.functions.parameter_count.truncate(1);
    assert!(validate(&tables).is_err());
}

#[test]
fn a_struct_whose_name_points_nowhere_is_reported() {
    let mut tables = example_tables();
    tables.structs.name[0] = StringId(NO_INDEX);
    let violations = validate(&tables).expect_err("a struct must have a name");
    assert!(violations.iter().any(|violation| matches!(
        violation,
        Violation::HandleOutOfRange {
            table: "structs",
            column: "name",
            ..
        }
    )));
}

#[test]
fn a_field_whose_name_points_nowhere_is_reported() {
    let mut tables = example_tables();
    tables.fields.name[0] = StringId(4242);
    let violations = validate(&tables).expect_err("a field must have a name");
    assert!(violations.iter().any(|violation| matches!(
        violation,
        Violation::HandleOutOfRange {
            table: "fields",
            column: "name",
            ..
        }
    )));
}

#[test]
fn a_function_whose_name_points_nowhere_is_reported() {
    let mut tables = example_tables();
    tables.functions.name[0] = StringId(4242);
    let violations = validate(&tables).expect_err("a function must have a name");
    assert!(violations.iter().any(|violation| matches!(
        violation,
        Violation::HandleOutOfRange {
            table: "functions",
            column: "name",
            ..
        }
    )));
}

#[test]
fn a_target_pointing_past_the_string_table_is_reported() {
    let mut tables = example_tables();
    tables.target = StringId(4242);
    let violations = validate(&tables).expect_err("the target must name a string");
    assert!(violations.iter().any(|violation| matches!(
        violation,
        Violation::HandleOutOfRange {
            table: "tables",
            column: "target",
            ..
        }
    )));
}

#[test]
fn a_call_argument_naming_a_missing_allocator_source_is_reported() {
    let mut tables = example_tables();
    tables.call_arguments.source[0] = zag_facts::AllocatorSourceId(77);
    let violations = validate(&tables).expect_err("a call argument must name a source");
    assert!(violations.iter().any(|violation| matches!(
        violation,
        Violation::HandleOutOfRange {
            table: "call_arguments",
            column: "source",
            ..
        }
    )));
}

#[test]
fn an_unreferenced_tail_in_the_string_blob_is_reported() {
    let mut tables = empty_tables();
    tables.target = intern(&mut tables.strings, b"alpha");
    tables.strings.bytes.extend_from_slice(b"orphan");
    let violations = validate(&tables).expect_err("every byte must belong to a string");
    assert!(
        violations
            .iter()
            .any(|violation| matches!(violation, Violation::StringBlobNotFullyCovered { .. }))
    );
}

#[test]
fn an_allocator_source_naming_a_parameter_that_does_not_exist_is_reported() {
    let mut tables = example_tables();
    let row = tables
        .allocator_sources
        .kind
        .iter()
        .position(|kind| *kind == zag_facts::tables::AllocatorSourceKind::Parameter)
        .expect("the fixture forwards allocators through parameters");
    tables.allocator_sources.parameter_index[row] = 9;
    let violations = validate(&tables).expect_err("a parameter index must name a parameter");
    assert!(violations.iter().any(|violation| matches!(
        violation,
        Violation::ParameterIndexOutOfRange {
            table: "allocator_sources",
            ..
        }
    )));
}

#[test]
fn a_call_argument_naming_a_parameter_that_does_not_exist_is_reported() {
    let mut tables = example_tables();
    tables.call_arguments.parameter_index[0] = 9;
    let violations = validate(&tables).expect_err("a call argument must name a parameter");
    assert!(violations.iter().any(|violation| matches!(
        violation,
        Violation::ParameterIndexOutOfRange {
            table: "call_arguments",
            ..
        }
    )));
}

#[test]
fn an_absent_allocator_on_a_memory_operation_is_allowed() {
    let mut tables = example_tables();
    tables.memory_operations.allocator[0] = zag_facts::AllocatorSourceId(NO_INDEX);
    assert_eq!(validate(&tables), Ok(()));
}

fn corrupt(tables: &mut Tables, column: usize, value: u32) {
    match column % 12 {
        0 => tables.structs.field_start[0] = value,
        1 => tables.structs.field_count[0] = value,
        2 => tables.functions.parameter_start[0] = value,
        3 => tables.functions.parameter_count[0] = value,
        4 => tables.fields.owner[0] = zag_facts::StructId(value),
        5 => tables.fields.field_type[0] = zag_facts::TypeId(value),
        6 => tables.calls.caller[0] = zag_facts::FunctionId(value),
        7 => tables.calls.callee[0] = zag_facts::FunctionId(value),
        8 => tables.memory_operations.place_field[0] = zag_facts::FieldId(value),
        9 => tables.field_assignments.field[0] = zag_facts::FieldId(value),
        10 => tables.allocator_sources.parameter_index[0] = value,
        _ => tables.target = StringId(value),
    }
}

proptest! {
    #[test]
    fn validation_never_panics_on_a_corrupt_table(column in 0usize..12, value in any::<u32>()) {
        let mut tables = example_tables();
        corrupt(&mut tables, column, value);
        let _ = validate(&tables);
    }

    #[test]
    fn a_corrupt_table_survives_the_wire_format_and_still_validates_without_panicking(
        column in 0usize..12,
        value in any::<u32>(),
    ) {
        let mut tables = example_tables();
        corrupt(&mut tables, column, value);
        let bytes = encode(&tables);
        let decoded = decode(&bytes).expect("column values do not affect decodability");
        prop_assert_eq!(validate(&tables).is_ok(), validate(&decoded).is_ok());
    }

    #[test]
    fn reading_any_identifier_out_of_any_string_table_never_panics(
        bytes in prop::collection::vec(any::<u8>(), 0..24),
        offsets in prop::collection::vec(any::<u32>(), 0..8),
        identifier in any::<u32>(),
    ) {
        let strings = Strings { bytes, offsets };
        let _ = string_bytes(&strings, StringId(identifier));
    }
}
