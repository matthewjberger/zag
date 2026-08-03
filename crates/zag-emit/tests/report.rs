//! Every line the report can print, produced from real tables rather than
//! assembled as a string. A message the code cannot actually reach is worse
//! than no message, so each case here builds the situation that reaches it.

use zag_analysis::analyze;
use zag_emit::function::Refusal;
use zag_emit::report::{Disposition, disposition, outcome_text, render_report};
use zag_facts::build::{
    declare_field, declare_function, declare_parameter, declare_struct, intern,
    push_allocator_source, push_call, push_call_argument, push_expression,
    push_field_assignment_with, push_integer_type, push_memory_operation, push_opaque_type,
    push_slice_type, push_void_type, set_function_signature, struct_type,
};
use zag_facts::tables::{
    AllocatorSourceKind, AssignmentSource, ExpressionKind, MemoryOperationKind,
    PARAMETER_FLAG_ALLOCATOR, PlaceKind, Tables, empty_tables,
};
use zag_facts::{FieldId, FunctionId, MemoryOperationId, NO_INDEX, StringId, StructId};

fn reported(tables: &Tables) -> String {
    let analysis = analyze(tables);
    String::from_utf8(render_report(tables, &analysis)).expect("the report is text")
}

fn outcome_of(tables: &Tables, function: FunctionId) -> Disposition {
    let analysis = analyze(tables);
    let lifetimes = zag_emit::lower::lifetimes_by_type(tables, &analysis.ownership);
    let lookups = zag_emit::index::build_index(tables);
    let lowering =
        zag_emit::lower::lowering(&lifetimes, &lookups, zag_facts::tables::ROOT_MODULE, false);
    disposition(tables, &analysis.ownership, lowering, function)
}

/// A struct with one field and an `init` that fills it however the case wants.
fn holder(fill: impl Fn(&mut Tables, StructId, FieldId, FunctionId)) -> Tables {
    let mut tables = empty_tables();
    tables.target = intern(&mut tables.strings, b"x86_64-linux");
    let byte = push_integer_type(&mut tables, 8, false);
    let text = push_slice_type(&mut tables, byte);
    let owner = declare_struct(&mut tables, b"Holder", 16, 8, 0);
    let field = declare_field(&mut tables, owner, b"data", text, 0);
    let holder = struct_type(&tables, owner);
    let initialize = declare_function(&mut tables, b"init", owner);
    declare_parameter(&mut tables, initialize, b"body", text, 0);
    set_function_signature(&mut tables, initialize, holder, StructId(NO_INDEX), false);
    fill(&mut tables, owner, field, initialize);
    tables
}

#[test]
fn a_field_nothing_assigns_names_that_field() {
    let tables = holder(|_, _, _, _| {});
    let report = reported(&tables);
    assert!(
        report.contains("no constructor: nothing the port could read assigns data"),
        "{report}"
    );
}

#[test]
fn an_expression_the_port_cannot_spell_is_quoted_back() {
    let tables = holder(|tables, _, field, initialize| {
        let text = zag_facts::build::push_string(&mut tables.strings, b"countWords(input)");
        let value = push_expression(
            tables,
            ExpressionKind::Unsupported,
            text,
            NO_INDEX,
            zag_facts::TypeId(NO_INDEX),
            FieldId(NO_INDEX),
            &[],
        );
        push_field_assignment_with(
            tables,
            field,
            initialize,
            AssignmentSource::Parameter,
            MemoryOperationId(NO_INDEX),
            value,
        );
    });
    let report = reported(&tables);
    assert!(
        report.contains(
            "no constructor: data is set from countWords(input), which the port cannot spell"
        ),
        "{report}"
    );
}

#[test]
fn an_unspellable_expression_nested_inside_a_literal_is_still_found() {
    let tables = holder(|tables, _, field, initialize| {
        let text = zag_facts::build::push_string(&mut tables.strings, b"@ptrCast(raw)");
        let inner = push_expression(
            tables,
            ExpressionKind::Unsupported,
            text,
            NO_INDEX,
            zag_facts::TypeId(NO_INDEX),
            FieldId(NO_INDEX),
            &[],
        );
        let value = push_expression(
            tables,
            ExpressionKind::Cast,
            StringId(NO_INDEX),
            NO_INDEX,
            zag_facts::TypeId(NO_INDEX),
            FieldId(NO_INDEX),
            &[inner],
        );
        push_field_assignment_with(
            tables,
            field,
            initialize,
            AssignmentSource::Parameter,
            MemoryOperationId(NO_INDEX),
            value,
        );
    });
    assert!(
        reported(&tables).contains("is set from @ptrCast(raw)"),
        "{}",
        reported(&tables)
    );
}

#[test]
fn a_field_whose_owner_was_not_decided_says_so() {
    let tables = holder(|tables, _, field, initialize| {
        let value = push_expression(
            tables,
            ExpressionKind::Parameter,
            StringId(NO_INDEX),
            0,
            zag_facts::TypeId(NO_INDEX),
            FieldId(NO_INDEX),
            &[],
        );
        // An unknown source is what leaves the ownership pass with nothing to
        // decide from, which is the situation this line exists for.
        push_field_assignment_with(
            tables,
            field,
            initialize,
            AssignmentSource::Unknown,
            MemoryOperationId(NO_INDEX),
            value,
        );
    });
    let report = reported(&tables);
    assert!(
        report.contains("no constructor: who owns data was not decided"),
        "{report}"
    );
}

/// Two callers handing one allocator parameter different allocators, which is
/// the shape `conflict` has and the only one that produces this section.
fn disagreeing() -> Tables {
    let mut tables = empty_tables();
    tables.target = intern(&mut tables.strings, b"x86_64-linux");
    let byte = push_integer_type(&mut tables, 8, false);
    let text = push_slice_type(&mut tables, byte);
    let void = push_void_type(&mut tables);
    let allocator_name = intern(&mut tables.strings, b"Allocator");
    let allocator = push_opaque_type(&mut tables, allocator_name);

    let owner = declare_struct(&mut tables, b"Cache", 16, 8, 0);
    let field = declare_field(&mut tables, owner, b"entries", text, 0);

    let make = declare_function(&mut tables, b"makeCache", StructId(NO_INDEX));
    declare_parameter(
        &mut tables,
        make,
        b"allocator",
        allocator,
        PARAMETER_FLAG_ALLOCATOR,
    );
    set_function_signature(&mut tables, make, void, StructId(NO_INDEX), false);
    let from_heap = declare_function(&mut tables, b"fromHeap", StructId(NO_INDEX));
    set_function_signature(&mut tables, from_heap, void, StructId(NO_INDEX), false);
    let from_arena = declare_function(&mut tables, b"fromArena", StructId(NO_INDEX));
    set_function_signature(&mut tables, from_arena, void, StructId(NO_INDEX), false);

    let page = push_allocator_source(
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
    let inside = push_allocator_source(&mut tables, AllocatorSourceKind::Parameter, make, 0);

    let call = push_call(&mut tables, from_heap, make);
    push_call_argument(&mut tables, call, 0, page);
    let call = push_call(&mut tables, from_arena, make);
    push_call_argument(&mut tables, call, 0, arena);

    let allocate = push_memory_operation(
        &mut tables,
        make,
        MemoryOperationKind::Allocate,
        inside,
        PlaceKind::FieldOfParameter,
        field,
    );
    let value = push_expression(
        &mut tables,
        ExpressionKind::Allocation,
        StringId(NO_INDEX),
        0,
        text,
        FieldId(NO_INDEX),
        &[],
    );
    push_field_assignment_with(
        &mut tables,
        field,
        make,
        AssignmentSource::Allocation,
        allocate,
        value,
    );
    tables
}

#[test]
fn a_conflict_names_both_callers_and_what_each_handed_over() {
    let report = reported(&disagreeing());
    assert!(report.contains("allocator conflicts: 1"), "{report}");
    assert!(
        report.contains("makeCache takes allocator from callers that disagree"),
        "{report}"
    );
    assert!(
        report.contains("from fromHeap: the global allocator"),
        "{report}"
    );
    assert!(report.contains("from fromArena: an arena"), "{report}");
}

#[test]
fn a_program_with_no_conflict_prints_no_conflict_section() {
    let tables = holder(|_, _, _, _| {});
    assert!(
        !reported(&tables).contains("allocator conflicts"),
        "{}",
        reported(&tables)
    );
}

#[test]
fn each_conflict_is_recorded_once_however_many_rounds_the_fixed_point_takes() {
    let analysis = analyze(&disagreeing());
    assert_eq!(analysis.provenance.disagreements.len(), 1);
}

#[test]
fn an_unnamed_error_set_is_a_different_outcome_from_a_borrowed_return() {
    let mut tables = holder(|_, _, _, _| {});
    let void = push_void_type(&mut tables);
    let fallible = declare_function(&mut tables, b"parse", StructId(NO_INDEX));
    set_function_signature(&mut tables, fallible, void, StructId(NO_INDEX), true);
    assert_eq!(
        outcome_of(&tables, fallible),
        Disposition::NotPorted(Refusal::UnnamedErrorSet)
    );
}

#[test]
fn a_return_type_that_never_resolved_is_its_own_outcome() {
    let mut tables = holder(|_, _, _, _| {});
    let function = declare_function(&mut tables, b"mystery", StructId(NO_INDEX));
    // Left with no signature at all, which is what the frontend leaves behind
    // when it could not read the return type.
    assert_eq!(
        outcome_of(&tables, function),
        Disposition::NotPorted(Refusal::ReturnTypeUnresolved)
    );
}

#[test]
fn every_outcome_has_wording_of_its_own() {
    let mut seen: Vec<&[u8]> = Vec::new();
    for outcome in [
        Disposition::Constructor,
        Disposition::SubsumedByDrop,
        Disposition::Signature,
        Disposition::NotPorted(Refusal::ReturnTypeUnresolved),
        Disposition::NotPorted(Refusal::UnnamedErrorSet),
        Disposition::NotPorted(Refusal::ReturnBorrowsAnArena),
        Disposition::NotPorted(Refusal::ReturnBorrowsWithNothingToTieItTo),
    ] {
        let text = outcome_text(outcome);
        assert!(
            !seen.contains(&text),
            "two outcomes read the same: {text:?}"
        );
        seen.push(text);
    }
}
