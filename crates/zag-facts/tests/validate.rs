use zag_facts::build::intern;
use zag_facts::fixture::example_tables;
use zag_facts::tables::{TypeKind, empty_tables};
use zag_facts::validate::{Violation, validate};
use zag_facts::{FieldId, FunctionId, StringId, StructId, TypeId};

#[test]
fn the_fixture_is_valid() {
    assert_eq!(validate(&example_tables()), Ok(()));
}

#[test]
fn an_empty_table_set_is_valid() {
    assert_eq!(validate(&empty_tables()), Ok(()));
}

#[test]
fn a_short_column_is_reported() {
    let mut tables = example_tables();
    tables.types.size.pop();
    let violations = validate(&tables).expect_err("a short column must be reported");
    assert!(violations.iter().any(|violation| matches!(
        violation,
        Violation::ColumnLengthMismatch {
            table: "types",
            column: "size",
            ..
        }
    )));
}

#[test]
fn a_field_pointing_at_a_missing_struct_is_reported() {
    let mut tables = example_tables();
    tables.fields.owner[0] = StructId(99);
    let violations = validate(&tables).expect_err("a dangling owner must be reported");
    assert!(violations.iter().any(|violation| matches!(
        violation,
        Violation::HandleOutOfRange {
            table: "fields",
            column: "owner",
            ..
        }
    )));
}

#[test]
fn a_field_range_that_does_not_tile_the_field_table_is_reported() {
    let mut tables = example_tables();
    tables.structs.field_count[0] += 1;
    let violations = validate(&tables).expect_err("an overlapping field range must be reported");
    assert!(
        violations
            .iter()
            .any(|violation| matches!(violation, Violation::FieldRangeMismatch { .. }))
    );
}

#[test]
fn a_parameter_range_that_does_not_tile_the_parameter_table_is_reported() {
    let mut tables = example_tables();
    tables.functions.parameter_count[0] += 1;
    let violations =
        validate(&tables).expect_err("an overlapping parameter range must be reported");
    assert!(
        violations
            .iter()
            .any(|violation| matches!(violation, Violation::ParameterRangeMismatch { .. }))
    );
}

#[test]
fn calls_out_of_caller_order_are_reported() {
    let mut tables = example_tables();
    tables.calls.caller.reverse();
    let violations = validate(&tables).expect_err("unsorted calls must be reported");
    assert!(
        violations
            .iter()
            .any(|violation| matches!(violation, Violation::CallsNotSortedByCaller { .. }))
    );
}

#[test]
fn fields_out_of_owner_order_are_reported() {
    let mut tables = example_tables();
    tables.fields.owner.swap(0, 8);
    let violations = validate(&tables).expect_err("ungrouped fields must be reported");
    assert!(
        violations
            .iter()
            .any(|violation| matches!(violation, Violation::FieldsNotGroupedByOwner { .. }))
    );
}

#[test]
fn string_offsets_that_run_backwards_are_reported() {
    let mut tables = empty_tables();
    intern(&mut tables.strings, b"alpha");
    intern(&mut tables.strings, b"beta");
    tables.strings.offsets.swap(1, 2);
    let violations = validate(&tables).expect_err("non monotonic offsets must be reported");
    assert!(
        violations
            .iter()
            .any(|violation| matches!(violation, Violation::StringOffsetsNotMonotonic { .. }))
    );
}

#[test]
fn string_offsets_past_the_byte_blob_are_reported() {
    let mut tables = empty_tables();
    intern(&mut tables.strings, b"alpha");
    let last = tables.strings.offsets.len() - 1;
    tables.strings.offsets[last] = 500;
    let violations = validate(&tables).expect_err("out of range offsets must be reported");
    assert!(
        violations
            .iter()
            .any(|violation| matches!(violation, Violation::StringOffsetOutOfRange { .. }))
    );
}

#[test]
fn a_sentinel_handle_is_accepted_where_absence_is_meaningful() {
    let mut tables = empty_tables();
    let name = intern(&mut tables.strings, b"Anonymous");
    tables.types.kind.push(TypeKind::Opaque);
    tables.types.element.push(TypeId(zag_facts::NO_INDEX));
    tables.types.count.push(0);
    tables.types.name.push(name);
    tables.types.size.push(0);
    tables.types.alignment.push(1);
    tables.types.bit_width.push(0);
    tables.types.flags.push(0);
    assert_eq!(validate(&tables), Ok(()));
}

#[test]
fn a_sentinel_handle_is_rejected_where_absence_is_not_meaningful() {
    let mut tables = example_tables();
    tables.field_assignments.field[0] = FieldId(zag_facts::NO_INDEX);
    let violations = validate(&tables).expect_err("a required handle must not be absent");
    assert!(violations.iter().any(|violation| matches!(
        violation,
        Violation::HandleOutOfRange {
            table: "field_assignments",
            column: "field",
            ..
        }
    )));
}

#[test]
fn a_deinit_pointing_at_a_missing_function_is_reported() {
    let mut tables = example_tables();
    tables.structs.deinit[0] = FunctionId(42);
    let violations = validate(&tables).expect_err("a dangling deinit must be reported");
    assert!(violations.iter().any(|violation| matches!(
        violation,
        Violation::HandleOutOfRange {
            table: "structs",
            column: "deinit",
            ..
        }
    )));
}

#[test]
fn a_type_name_pointing_past_the_string_table_is_reported() {
    let mut tables = example_tables();
    tables.types.name[0] = StringId(9999);
    let violations = validate(&tables).expect_err("a dangling string handle must be reported");
    assert!(violations.iter().any(|violation| matches!(
        violation,
        Violation::HandleOutOfRange {
            table: "types",
            column: "name",
            ..
        }
    )));
}
