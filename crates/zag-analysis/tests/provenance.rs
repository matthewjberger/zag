use proptest::prelude::*;
use zag_analysis::provenance::{
    AllocatorClass, classify_source, join, resolve_allocator_provenance,
};
use zag_facts::build::{
    intern, push_allocator_source, push_call, push_call_argument, push_function, push_opaque_type,
    push_parameter,
};
use zag_facts::fixture::example_tables;
use zag_facts::tables::{
    AllocatorSourceKind, PARAMETER_FLAG_ALLOCATOR, Tables, empty_tables, string_bytes,
};
use zag_facts::validate::validate;
use zag_facts::{AllocatorSourceId, FunctionId, NO_INDEX, StructId};

const CLASSES: [AllocatorClass; 4] = [
    AllocatorClass::Unset,
    AllocatorClass::Global,
    AllocatorClass::Arena,
    AllocatorClass::Conflicting,
];

fn class_from_index(index: usize) -> AllocatorClass {
    CLASSES[index % CLASSES.len()]
}

proptest! {
    #[test]
    fn join_is_commutative(left in 0usize..4, right in 0usize..4) {
        let left = class_from_index(left);
        let right = class_from_index(right);
        prop_assert_eq!(join(left, right), join(right, left));
    }

    #[test]
    fn join_is_associative(first in 0usize..4, second in 0usize..4, third in 0usize..4) {
        let first = class_from_index(first);
        let second = class_from_index(second);
        let third = class_from_index(third);
        prop_assert_eq!(
            join(join(first, second), third),
            join(first, join(second, third))
        );
    }

    #[test]
    fn join_is_idempotent(only in 0usize..4) {
        let only = class_from_index(only);
        prop_assert_eq!(join(only, only), only);
    }

    #[test]
    fn unset_is_the_identity(only in 0usize..4) {
        let only = class_from_index(only);
        prop_assert_eq!(join(AllocatorClass::Unset, only), only);
    }

    #[test]
    fn conflicting_absorbs(only in 0usize..4) {
        let only = class_from_index(only);
        prop_assert_eq!(
            join(AllocatorClass::Conflicting, only),
            AllocatorClass::Conflicting
        );
    }
}

#[test]
fn a_global_allocator_and_an_arena_disagree() {
    assert_eq!(
        join(AllocatorClass::Global, AllocatorClass::Arena),
        AllocatorClass::Conflicting
    );
}

fn allocator_chain(length: u32) -> Tables {
    let mut tables = empty_tables();
    let name = intern(&mut tables.strings, b"Allocator");
    let allocator = push_opaque_type(&mut tables, name);
    for index in 0..length {
        let name = intern(&mut tables.strings, format!("f{index}").as_bytes());
        let function = push_function(&mut tables, name, StructId(NO_INDEX));
        let parameter_name = intern(&mut tables.strings, b"allocator");
        push_parameter(
            &mut tables,
            function,
            parameter_name,
            allocator,
            PARAMETER_FLAG_ALLOCATOR,
        );
    }
    tables
}

#[test]
fn a_concrete_allocator_flows_along_the_whole_call_chain() {
    let mut tables = allocator_chain(4);
    let global = push_allocator_source(
        &mut tables,
        AllocatorSourceKind::Global,
        FunctionId(NO_INDEX),
        NO_INDEX,
    );
    let mut forwarded = Vec::new();
    for index in 0..4u32 {
        forwarded.push(push_allocator_source(
            &mut tables,
            AllocatorSourceKind::Parameter,
            FunctionId(index),
            0,
        ));
    }
    for index in 0..3u32 {
        let call = push_call(&mut tables, FunctionId(index), FunctionId(index + 1));
        let source = if index == 0 {
            global
        } else {
            forwarded[index as usize]
        };
        push_call_argument(&mut tables, call, 0, source);
    }
    assert_eq!(validate(&tables), Ok(()));
    let provenance = resolve_allocator_provenance(&tables);
    assert!(provenance.converged);
    assert_eq!(
        provenance.parameter_class,
        vec![
            AllocatorClass::Unset,
            AllocatorClass::Global,
            AllocatorClass::Global,
            AllocatorClass::Global
        ]
    );
}

#[test]
fn two_callers_passing_different_allocators_conflict() {
    let mut tables = allocator_chain(3);
    let global = push_allocator_source(
        &mut tables,
        AllocatorSourceKind::Global,
        FunctionId(NO_INDEX),
        NO_INDEX,
    );
    let arena = push_allocator_source(
        &mut tables,
        AllocatorSourceKind::Arena,
        FunctionId(NO_INDEX),
        NO_INDEX,
    );
    let call = push_call(&mut tables, FunctionId(0), FunctionId(2));
    push_call_argument(&mut tables, call, 0, global);
    let call = push_call(&mut tables, FunctionId(1), FunctionId(2));
    push_call_argument(&mut tables, call, 0, arena);
    assert_eq!(validate(&tables), Ok(()));
    let provenance = resolve_allocator_provenance(&tables);
    assert!(provenance.converged);
    assert_eq!(provenance.parameter_class[2], AllocatorClass::Conflicting);
}

#[test]
fn an_uncalled_function_keeps_an_unset_allocator() {
    let tables = allocator_chain(2);
    let provenance = resolve_allocator_provenance(&tables);
    assert!(provenance.converged);
    assert_eq!(
        provenance.parameter_class,
        vec![AllocatorClass::Unset, AllocatorClass::Unset]
    );
}

#[test]
fn a_recursive_call_still_converges() {
    let mut tables = allocator_chain(1);
    let forwarded = push_allocator_source(
        &mut tables,
        AllocatorSourceKind::Parameter,
        FunctionId(0),
        0,
    );
    let call = push_call(&mut tables, FunctionId(0), FunctionId(0));
    push_call_argument(&mut tables, call, 0, forwarded);
    let provenance = resolve_allocator_provenance(&tables);
    assert!(provenance.converged);
    assert_eq!(provenance.parameter_class, vec![AllocatorClass::Unset]);
}

#[test]
fn an_out_of_range_source_is_treated_as_conflicting() {
    let tables = example_tables();
    let provenance = resolve_allocator_provenance(&tables);
    assert_eq!(
        classify_source(&tables, &provenance, AllocatorSourceId(9999)),
        AllocatorClass::Conflicting
    );
}

#[test]
fn the_fixture_resolves_the_allocator_of_each_entry_point() {
    let tables = example_tables();
    let provenance = resolve_allocator_provenance(&tables);
    assert!(provenance.converged);
    let initialize = function_named(&tables, b"init");
    let parse_node = function_named(&tables, b"parseNode");
    let initialize_allocator = tables.functions.parameter_start[initialize] as usize;
    let parse_node_allocator = tables.functions.parameter_start[parse_node] as usize;
    assert_eq!(
        provenance.parameter_class[initialize_allocator],
        AllocatorClass::Global
    );
    assert_eq!(
        provenance.parameter_class[parse_node_allocator],
        AllocatorClass::Arena
    );
}

fn function_named(tables: &Tables, wanted: &[u8]) -> usize {
    tables
        .functions
        .name
        .iter()
        .position(|name| string_bytes(&tables.strings, *name) == wanted)
        .expect("the fixture defines this function")
}
