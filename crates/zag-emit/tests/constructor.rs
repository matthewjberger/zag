//! A constructor is written from the expression a field is set to. These check
//! each shape the expression table can hold, and check that one field the port
//! cannot spell stops the whole constructor rather than producing a body with
//! a hole in it.

use zag_analysis::analyze;
use zag_emit::lower::lower;
use zag_facts::build::{
    declare_field, declare_function, declare_parameter, declare_struct, intern, push_expression,
    push_field_assignment_with, push_integer_type, push_optional_type, push_slice_type,
    push_string, struct_type,
};
use zag_facts::fixture::example_tables;
use zag_facts::tables::{
    AssignmentSource, ExpressionKind, PARAMETER_FLAG_ALLOCATOR, Tables, empty_tables,
};
use zag_facts::{ExpressionId, FieldId, FunctionId, MemoryOperationId, NO_INDEX, StringId, TypeId};
use zag_render::render;

struct Holder {
    tables: Tables,
    word: TypeId,
    optional: TypeId,
    fields: Vec<FieldId>,
    initialize: FunctionId,
}

/// One struct with the given field types, an `init` taking an allocator and a
/// `body` slice, which is the shape every case below fills in.
fn holder(field_types: &[&str]) -> Holder {
    let mut tables = empty_tables();
    tables.target = intern(&mut tables.strings, b"x86_64-linux");
    let byte = push_integer_type(&mut tables, 8, false);
    let word = push_integer_type(&mut tables, 32, false);
    let bytes = push_slice_type(&mut tables, byte);
    let optional = push_optional_type(&mut tables, bytes);
    let allocator_name = intern(&mut tables.strings, b"Allocator");
    let allocator = zag_facts::build::push_opaque_type(&mut tables, allocator_name);

    let owner = declare_struct(&mut tables, b"Holder", 24, 8, 0);
    let mut fields = Vec::new();
    for (index, declared) in field_types.iter().enumerate() {
        let kind = match *declared {
            "bytes" => bytes,
            "optional" => optional,
            _ => word,
        };
        fields.push(declare_field(
            &mut tables,
            owner,
            format!("field{index}").as_bytes(),
            kind,
            index as u32 * 8,
        ));
    }
    let _ = struct_type(&tables, owner);
    let initialize = declare_function(&mut tables, b"init", owner);
    declare_parameter(
        &mut tables,
        initialize,
        b"allocator",
        allocator,
        PARAMETER_FLAG_ALLOCATOR,
    );
    declare_parameter(&mut tables, initialize, b"body", bytes, 0);
    Holder {
        tables,
        word,
        optional,
        fields,
        initialize,
    }
}

fn simple(holder: &mut Holder, kind: ExpressionKind, text: &[u8], parameter: u32) -> ExpressionId {
    let interned = if text.is_empty() {
        StringId(NO_INDEX)
    } else {
        push_string(&mut holder.tables.strings, text)
    };
    push_expression(
        &mut holder.tables,
        kind,
        interned,
        parameter,
        holder.word,
        FieldId(NO_INDEX),
        &[],
    )
}

fn assign(holder: &mut Holder, index: usize, expression: ExpressionId) {
    assign_from(holder, index, AssignmentSource::Unknown, expression);
}

/// The source decides the ownership class, and a field the analysis cannot own
/// gets no constructor at all, so a case about a reference field has to say
/// where the reference came from.
fn assign_from(
    holder: &mut Holder,
    index: usize,
    source: AssignmentSource,
    expression: ExpressionId,
) {
    push_field_assignment_with(
        &mut holder.tables,
        holder.fields[index],
        holder.initialize,
        source,
        MemoryOperationId(NO_INDEX),
        expression,
    );
}

fn ported(holder: &Holder) -> String {
    let analysis = analyze(&holder.tables);
    let ast = lower(&holder.tables, &analysis.ownership);
    String::from_utf8(render(&ast).expect("the tree must render")).expect("the output is text")
}

fn optional_expression(holder: &mut Holder, kind: ExpressionKind, parameter: u32) -> ExpressionId {
    push_expression(
        &mut holder.tables,
        kind,
        StringId(NO_INDEX),
        parameter,
        holder.optional,
        FieldId(NO_INDEX),
        &[],
    )
}

#[test]
fn an_optional_field_set_to_null_comes_across_as_none() {
    let mut holder = holder(&["optional"]);
    let value = optional_expression(&mut holder, ExpressionKind::Null, NO_INDEX);
    assign_from(&mut holder, 0, AssignmentSource::StaticLiteral, value);
    let source = ported(&holder);
    assert!(
        source.contains("pub field0: Option<&'static [u8]>,"),
        "{source}"
    );
    assert!(source.contains("field0: None,"), "{source}");
}

#[test]
fn a_value_going_into_an_optional_field_is_wrapped_in_some() {
    let mut holder = holder(&["optional"]);
    let value = optional_expression(&mut holder, ExpressionKind::Parameter, 1);
    assign_from(&mut holder, 0, AssignmentSource::Parameter, value);
    let source = ported(&holder);
    assert!(source.contains("pub field0: Option<&'a [u8]>,"), "{source}");
    assert!(source.contains("field0: Some(body),"), "{source}");
}

#[test]
fn a_literal_field_comes_across_verbatim() {
    let mut holder = holder(&["word"]);
    let value = simple(&mut holder, ExpressionKind::Literal, b"0x2A", NO_INDEX);
    assign(&mut holder, 0, value);
    let source = ported(&holder);
    assert!(
        source.contains("pub fn new(body: &[u8]) -> Self {"),
        "{source}"
    );
    assert!(source.contains("field0: 0x2A,"), "{source}");
}

#[test]
fn a_parameter_field_names_the_parameter() {
    let mut holder = holder(&["bytes"]);
    let value = simple(&mut holder, ExpressionKind::Parameter, b"", 1);
    assign_from(&mut holder, 0, AssignmentSource::Parameter, value);
    let source = ported(&holder);
    assert!(source.contains("field0: body,"), "{source}");
}

#[test]
fn a_field_the_analysis_cannot_own_stops_the_constructor() {
    let mut holder = holder(&["bytes"]);
    let value = simple(&mut holder, ExpressionKind::Parameter, b"", 1);
    assign(&mut holder, 0, value);
    let source = ported(&holder);
    assert!(
        source.contains("Option<core::ptr::NonNull<[u8]>>"),
        "{source}"
    );
    assert!(!source.contains("impl Holder"), "{source}");
}

#[test]
fn a_length_becomes_a_call() {
    let mut holder = holder(&["word"]);
    let value = simple(&mut holder, ExpressionKind::Length, b"", 1);
    assign(&mut holder, 0, value);
    assert!(ported(&holder).contains("field0: body.len(),"));
}

#[test]
fn a_cast_becomes_a_checked_conversion() {
    let mut holder = holder(&["word"]);
    let inner = simple(&mut holder, ExpressionKind::Length, b"", 1);
    let value = push_expression(
        &mut holder.tables,
        ExpressionKind::Cast,
        StringId(NO_INDEX),
        NO_INDEX,
        holder.word,
        FieldId(NO_INDEX),
        &[inner],
    );
    assign(&mut holder, 0, value);
    assert!(
        ported(&holder).contains("field0: u32::try_from(body.len()).unwrap(),"),
        "{}",
        ported(&holder)
    );
}

#[test]
fn an_allocation_becomes_a_copy_into_a_box() {
    // An owned field needs a free site and a resolved allocator behind it, so
    // this reads the case that has both rather than assembling one.
    let tables = zag_facts::examples::tables_for("netpacket").expect("registered");
    let analysis = analyze(&tables);
    let ast = lower(&tables, &analysis.ownership);
    let source = String::from_utf8(render(&ast).expect("renders")).expect("text");
    assert!(source.contains("payload: Box::from(body),"), "{source}");
}

#[test]
fn the_allocator_parameter_does_not_survive_into_the_signature() {
    let mut holder = holder(&["word"]);
    let value = simple(&mut holder, ExpressionKind::Literal, b"1", NO_INDEX);
    assign(&mut holder, 0, value);
    let source = ported(&holder);
    assert!(source.contains("pub fn new(body: &[u8])"), "{source}");
    assert!(!source.contains("allocator"), "{source}");
}

#[test]
fn one_field_the_port_cannot_spell_stops_the_whole_constructor() {
    let mut holder = holder(&["word", "word"]);
    let good = simple(&mut holder, ExpressionKind::Literal, b"1", NO_INDEX);
    assign(&mut holder, 0, good);
    let bad = simple(&mut holder, ExpressionKind::Unsupported, b"", NO_INDEX);
    assign(&mut holder, 1, bad);
    let source = ported(&holder);
    assert!(!source.contains("impl Holder"), "{source}");
    assert!(source.contains("pub struct Holder"), "{source}");
}

#[test]
fn a_field_with_no_assignment_at_all_stops_the_constructor() {
    let mut holder = holder(&["word", "word"]);
    let value = simple(&mut holder, ExpressionKind::Literal, b"1", NO_INDEX);
    assign(&mut holder, 0, value);
    assert!(!ported(&holder).contains("impl Holder"));
}

#[test]
fn a_struct_with_no_init_gets_no_constructor() {
    let mut tables = empty_tables();
    tables.target = intern(&mut tables.strings, b"x86_64-linux");
    let word = push_integer_type(&mut tables, 32, false);
    let owner = declare_struct(&mut tables, b"Plain", 4, 4, 0);
    declare_field(&mut tables, owner, b"value", word, 0);
    let analysis = analyze(&tables);
    let ast = lower(&tables, &analysis.ownership);
    let source = String::from_utf8(render(&ast).expect("renders")).expect("text");
    assert!(!source.contains("impl"), "{source}");
}

/// The coverage example fills its buffer from an allocation and a length, both
/// of which the port can spell, so it gets a constructor and the allocator
/// parameter does not survive into it.
#[test]
fn the_coverage_example_gets_the_constructor_its_expressions_allow() {
    let tables = example_tables();
    let analysis = analyze(&tables);
    let ast = lower(&tables, &analysis.ownership);
    let source = String::from_utf8(render(&ast).expect("renders")).expect("text");
    assert!(source.contains("impl Buffer {"), "{source}");
    assert!(
        source.contains("pub fn new(bytes: &[u8]) -> Self {"),
        "{source}"
    );
    assert!(source.contains("data: Box::from(bytes),"), "{source}");
    assert!(
        source.contains("length: u32::try_from(bytes.len()).unwrap(),"),
        "{source}"
    );
}

/// A boxed slice is exactly as long as it was allocated, so a field something
/// reallocates has to be a vector, and the copy that fills it has to be the one
/// that produces a vector.
#[test]
fn a_field_that_is_reallocated_becomes_a_vector_rather_than_a_boxed_slice() {
    let mut tables = zag_facts::examples::tables_for("netpacket").expect("registered");
    let field = tables
        .fields
        .name
        .iter()
        .position(|name| zag_facts::tables::string_bytes(&tables.strings, *name) == b"payload")
        .expect("the packet carries a payload");
    let allocate = tables
        .memory_operations
        .kind
        .iter()
        .position(|kind| *kind == zag_facts::tables::MemoryOperationKind::Allocate)
        .expect("the packet allocates its payload");
    let function = tables.memory_operations.function[allocate];
    let allocator = tables.memory_operations.allocator[allocate];
    zag_facts::build::push_memory_operation(
        &mut tables,
        function,
        zag_facts::tables::MemoryOperationKind::Resize,
        allocator,
        zag_facts::tables::PlaceKind::FieldOfParameter,
        FieldId(field as u32),
    );
    let analysis = analyze(&tables);
    let ast = lower(&tables, &analysis.ownership);
    let source = String::from_utf8(render(&ast).expect("renders")).expect("text");
    assert!(source.contains("pub payload: Vec<u8>,"), "{source}");
    assert!(source.contains("payload: body.to_vec(),"), "{source}");
    assert!(!source.contains("Box<[u8]>"), "{source}");
}
