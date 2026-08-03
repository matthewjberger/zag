use zag_analysis::ownership::build_field_index;
use zag_facts::fixture::example_tables;
use zag_facts::tables::{
    MemoryOperationKind, PlaceKind, Tables, empty_tables, field_count, memory_operation_count,
};
use zag_facts::{FieldId, NO_INDEX};

fn free_rows_for(tables: &Tables, field: FieldId) -> Vec<u32> {
    let index = build_field_index(tables);
    let slot = field.0 as usize;
    index.free_rows[index.free_start[slot] as usize..index.free_start[slot + 1] as usize].to_vec()
}

fn assignment_rows_for(tables: &Tables, field: FieldId) -> Vec<u32> {
    let index = build_field_index(tables);
    let slot = field.0 as usize;
    index.assignment_rows
        [index.assignment_start[slot] as usize..index.assignment_start[slot + 1] as usize]
        .to_vec()
}

#[test]
fn an_empty_program_has_an_empty_index() {
    let index = build_field_index(&empty_tables());
    assert_eq!(index.free_start, vec![0]);
    assert_eq!(index.free_rows, Vec::new());
    assert_eq!(index.assignment_start, vec![0]);
    assert_eq!(index.assignment_rows, Vec::new());
}

#[test]
fn the_ranges_tile_the_grouped_rows() {
    let tables = example_tables();
    let index = build_field_index(&tables);
    let fields = field_count(&tables.fields);
    assert_eq!(index.free_start.len(), fields + 1);
    assert_eq!(index.assignment_start.len(), fields + 1);
    assert_eq!(index.free_start[0], 0);
    assert_eq!(index.assignment_start[0], 0);
    for slot in 0..fields {
        assert!(index.free_start[slot] <= index.free_start[slot + 1]);
        assert!(index.assignment_start[slot] <= index.assignment_start[slot + 1]);
    }
    assert_eq!(index.free_start[fields] as usize, index.free_rows.len());
    assert_eq!(
        index.assignment_start[fields] as usize,
        index.assignment_rows.len()
    );
}

#[test]
fn every_row_lands_under_the_field_it_touches() {
    let tables = example_tables();
    let index = build_field_index(&tables);
    for slot in 0..field_count(&tables.fields) {
        for entry in index.free_start[slot] as usize..index.free_start[slot + 1] as usize {
            let row = index.free_rows[entry] as usize;
            assert_eq!(
                tables.memory_operations.place_field[row],
                FieldId(slot as u32)
            );
            assert_eq!(
                tables.memory_operations.kind[row],
                MemoryOperationKind::Free
            );
        }
        for entry in
            index.assignment_start[slot] as usize..index.assignment_start[slot + 1] as usize
        {
            let row = index.assignment_rows[entry] as usize;
            assert_eq!(tables.field_assignments.field[row], FieldId(slot as u32));
        }
    }
}

#[test]
fn the_index_holds_every_qualifying_row_and_no_others() {
    let tables = example_tables();
    let index = build_field_index(&tables);
    let expected = (0..memory_operation_count(&tables.memory_operations))
        .filter(|row| {
            tables.memory_operations.kind[*row] == MemoryOperationKind::Free
                && tables.memory_operations.place[*row] == PlaceKind::FieldOfParameter
        })
        .count();
    assert_eq!(index.free_rows.len(), expected);
    assert_eq!(
        index.assignment_rows.len(),
        tables.field_assignments.field.len()
    );
}

#[test]
fn rows_stay_in_their_original_order_within_a_field() {
    let mut tables = example_tables();
    let field = tables.memory_operations.place_field[1];
    for _ in 0..3 {
        zag_facts::build::push_memory_operation(
            &mut tables,
            zag_facts::FunctionId(0),
            MemoryOperationKind::Free,
            zag_facts::AllocatorSourceId(0),
            PlaceKind::FieldOfParameter,
            field,
        );
    }
    let rows = free_rows_for(&tables, field);
    let mut sorted = rows.clone();
    sorted.sort();
    assert_eq!(rows, sorted);
    assert_eq!(rows.len(), 4);
}

#[test]
fn a_row_naming_no_field_is_dropped_rather_than_grouped() {
    let mut tables = example_tables();
    tables.memory_operations.place_field[1] = FieldId(NO_INDEX);
    let index = build_field_index(&tables);
    assert_eq!(index.free_rows.len(), 0);
}

#[test]
fn a_free_of_a_local_is_not_grouped_under_any_field() {
    let mut tables = example_tables();
    let field = tables.memory_operations.place_field[1];
    tables.memory_operations.place[1] = PlaceKind::Local;
    assert_eq!(free_rows_for(&tables, field), Vec::new());
}

#[test]
fn an_allocation_is_not_grouped_as_a_free() {
    let tables = example_tables();
    let index = build_field_index(&tables);
    for row in &index.free_rows {
        assert_eq!(
            tables.memory_operations.kind[*row as usize],
            MemoryOperationKind::Free
        );
    }
}

#[test]
fn a_field_with_nothing_touching_it_gets_an_empty_range() {
    let tables = example_tables();
    let entries = FieldId((field_count(&tables.fields) - 1) as u32);
    assert_eq!(free_rows_for(&tables, entries), Vec::new());
    assert_eq!(assignment_rows_for(&tables, entries), Vec::new());
}
