//! What counts as a reference decides whether a field is ownership-analysed at
//! all, so an option that hides a slice has to answer the same way the slice
//! would.

use zag_facts::TypeId;
use zag_facts::build::{
    push_array_type, push_integer_type, push_optional_type, push_pointer_type, push_slice_type,
};
use zag_facts::tables::{empty_tables, is_reference_type};

#[test]
fn a_slice_and_a_pointer_are_references_and_a_scalar_is_not() {
    let mut tables = empty_tables();
    let byte = push_integer_type(&mut tables, 8, false);
    let slice = push_slice_type(&mut tables, byte);
    let pointer = push_pointer_type(&mut tables, byte);
    assert!(is_reference_type(&tables.types, slice));
    assert!(is_reference_type(&tables.types, pointer));
    assert!(!is_reference_type(&tables.types, byte));
}

#[test]
fn an_option_answers_for_whatever_it_holds() {
    let mut tables = empty_tables();
    let byte = push_integer_type(&mut tables, 8, false);
    let slice = push_slice_type(&mut tables, byte);
    let optional_slice = push_optional_type(&mut tables, slice);
    let optional_scalar = push_optional_type(&mut tables, byte);
    assert!(is_reference_type(&tables.types, optional_slice));
    assert!(!is_reference_type(&tables.types, optional_scalar));
}

#[test]
fn an_array_owns_its_elements_and_is_not_a_reference() {
    let mut tables = empty_tables();
    let byte = push_integer_type(&mut tables, 8, false);
    let slice = push_slice_type(&mut tables, byte);
    let array = push_array_type(&mut tables, slice, 4, 64);
    assert!(!is_reference_type(&tables.types, array));
}

#[test]
fn a_type_that_is_not_there_is_not_a_reference() {
    let tables = empty_tables();
    assert!(!is_reference_type(&tables.types, TypeId(9999)));
}

#[test]
fn an_option_that_holds_itself_terminates() {
    let mut tables = empty_tables();
    let byte = push_integer_type(&mut tables, 8, false);
    let looping = push_optional_type(&mut tables, byte);
    tables.types.element[looping.0 as usize] = looping;
    assert!(!is_reference_type(&tables.types, looping));
}
