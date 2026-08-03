//! A function the port keeps but cannot write the body of still comes across
//! as a signature. These check what goes into one, and check that the two
//! things the port refuses to invent, an unnamed error set and a lifetime with
//! nothing to tie it to, leave the function out instead.

use zag_analysis::analyze;
use zag_emit::lower::lower;
use zag_emit::report::{Disposition, disposition};
use zag_facts::build::{
    declare_field, declare_function, declare_parameter, declare_struct, intern,
    push_field_assignment, push_integer_type, push_opaque_type, push_pointer_type, push_slice_type,
    push_void_type, set_function_signature, struct_type,
};
use zag_facts::tables::{
    AssignmentSource, PARAMETER_FLAG_ALLOCATOR, PARAMETER_FLAG_MUTABLE, Tables, empty_tables,
};
use zag_facts::{FunctionId, MemoryOperationId, NO_INDEX, StructId, TypeId};
use zag_render::render;

struct Program {
    tables: Tables,
    byte: TypeId,
    word: TypeId,
    text: TypeId,
    void: TypeId,
    allocator: TypeId,
}

fn program() -> Program {
    let mut tables = empty_tables();
    tables.target = intern(&mut tables.strings, b"x86_64-linux");
    let byte = push_integer_type(&mut tables, 8, false);
    let word = push_integer_type(&mut tables, 32, false);
    let text = push_slice_type(&mut tables, byte);
    let void = push_void_type(&mut tables);
    let allocator_name = intern(&mut tables.strings, b"Allocator");
    let allocator = push_opaque_type(&mut tables, allocator_name);
    Program {
        tables,
        byte,
        word,
        text,
        void,
        allocator,
    }
}

fn ported(program: &Program) -> String {
    let analysis = analyze(&program.tables);
    let ast = lower(&program.tables, &analysis.ownership);
    String::from_utf8(render(&ast).expect("the tree must render")).expect("the output is text")
}

fn outcome(program: &Program, function: FunctionId) -> Disposition {
    let analysis = analyze(&program.tables);
    let lifetimes = zag_emit::lower::lifetimes_by_type(&program.tables, &analysis.ownership);
    let lowering = zag_emit::lower::lowering(&lifetimes, zag_facts::tables::ROOT_MODULE, false);
    disposition(&program.tables, &analysis.ownership, lowering, function)
}

#[test]
fn an_infallible_function_comes_across_with_its_body_left_open() {
    let mut program = program();
    let function = declare_function(&mut program.tables, b"measure", StructId(NO_INDEX));
    declare_parameter(&mut program.tables, function, b"text", program.text, 0);
    set_function_signature(
        &mut program.tables,
        function,
        program.word,
        StructId(NO_INDEX),
        false,
    );
    let source = ported(&program);
    assert!(
        source.contains("pub fn measure(text: &[u8]) -> u32 {"),
        "{source}"
    );
    assert!(source.contains("todo!()"), "{source}");
    assert_eq!(outcome(&program, function), Disposition::Signature);
}

#[test]
fn a_camel_case_name_comes_across_in_snake_case() {
    let mut program = program();
    let function = declare_function(&mut program.tables, b"makeBufferFrom", StructId(NO_INDEX));
    declare_parameter(&mut program.tables, function, b"text", program.text, 0);
    set_function_signature(
        &mut program.tables,
        function,
        program.word,
        StructId(NO_INDEX),
        false,
    );
    assert!(
        ported(&program).contains("pub fn make_buffer_from("),
        "{}",
        ported(&program)
    );
}

#[test]
fn a_function_that_gives_nothing_back_writes_no_return_type() {
    let mut program = program();
    let function = declare_function(&mut program.tables, b"reset", StructId(NO_INDEX));
    set_function_signature(
        &mut program.tables,
        function,
        program.void,
        StructId(NO_INDEX),
        false,
    );
    let source = ported(&program);
    assert!(source.contains("pub fn reset() {"), "{source}");
}

#[test]
fn a_named_error_set_becomes_the_error_half_of_a_result() {
    let mut program = program();
    let failure = declare_struct(&mut program.tables, b"ParseError", 1, 1, 0);
    zag_facts::build::set_struct_kind(
        &mut program.tables,
        failure,
        zag_facts::tables::ContainerKind::ErrorSet,
    );
    declare_field(&mut program.tables, failure, b"Empty", program.void, 0);
    let function = declare_function(&mut program.tables, b"parse", StructId(NO_INDEX));
    declare_parameter(&mut program.tables, function, b"text", program.text, 0);
    set_function_signature(&mut program.tables, function, program.word, failure, true);
    assert!(
        ported(&program).contains("pub fn parse(text: &[u8]) -> Result<u32, ParseError> {"),
        "{}",
        ported(&program)
    );
}

#[test]
fn an_error_set_the_zig_never_named_leaves_the_function_out() {
    let mut program = program();
    let function = declare_function(&mut program.tables, b"parse", StructId(NO_INDEX));
    declare_parameter(&mut program.tables, function, b"text", program.text, 0);
    set_function_signature(
        &mut program.tables,
        function,
        program.word,
        StructId(NO_INDEX),
        true,
    );
    assert!(
        !ported(&program).contains("pub fn parse"),
        "{}",
        ported(&program)
    );
    assert_eq!(outcome(&program, function), Disposition::NotPorted);
}

#[test]
fn the_allocator_parameter_does_not_survive_the_port() {
    let mut program = program();
    let function = declare_function(&mut program.tables, b"measure", StructId(NO_INDEX));
    declare_parameter(
        &mut program.tables,
        function,
        b"allocator",
        program.allocator,
        PARAMETER_FLAG_ALLOCATOR,
    );
    declare_parameter(&mut program.tables, function, b"text", program.text, 0);
    set_function_signature(
        &mut program.tables,
        function,
        program.word,
        StructId(NO_INDEX),
        false,
    );
    assert!(
        ported(&program).contains("pub fn measure(text: &[u8]) -> u32 {"),
        "{}",
        ported(&program)
    );
}

/// A method goes inside its struct's `impl` block, and its first parameter is
/// the receiver rather than an argument.
fn with_receiver(program: &mut Program, mutable: bool) -> FunctionId {
    let owner = declare_struct(&mut program.tables, b"Holder", 8, 8, 0);
    declare_field(&mut program.tables, owner, b"length", program.word, 0);
    let holder = struct_type(&program.tables, owner);
    let pointer = push_pointer_type(&mut program.tables, holder);
    let function = declare_function(&mut program.tables, b"length", owner);
    let flags = if mutable { PARAMETER_FLAG_MUTABLE } else { 0 };
    declare_parameter(&mut program.tables, function, b"self", pointer, flags);
    set_function_signature(
        &mut program.tables,
        function,
        program.word,
        StructId(NO_INDEX),
        false,
    );
    function
}

#[test]
fn a_pointer_receiver_the_zig_writes_through_becomes_a_mutable_borrow() {
    let mut program = program();
    with_receiver(&mut program, true);
    let source = ported(&program);
    assert!(source.contains("impl Holder {"), "{source}");
    assert!(
        source.contains("pub fn length(&mut self) -> u32 {"),
        "{source}"
    );
}

#[test]
fn a_const_pointer_receiver_becomes_a_shared_borrow() {
    let mut program = program();
    with_receiver(&mut program, false);
    assert!(
        ported(&program).contains("pub fn length(&self) -> u32 {"),
        "{}",
        ported(&program)
    );
}

/// A struct that borrows declares a lifetime, and a free function handing one
/// back has to introduce that lifetime and tie it to a parameter.
fn borrowing_view(program: &mut Program) -> (StructId, FunctionId) {
    let owner = declare_struct(&mut program.tables, b"View", 16, 8, 0);
    let field = declare_field(&mut program.tables, owner, b"bytes", program.text, 0);
    let view = struct_type(&program.tables, owner);
    let function = declare_function(&mut program.tables, b"makeView", StructId(NO_INDEX));
    declare_parameter(&mut program.tables, function, b"bytes", program.text, 0);
    set_function_signature(
        &mut program.tables,
        function,
        view,
        StructId(NO_INDEX),
        false,
    );
    push_field_assignment(
        &mut program.tables,
        field,
        function,
        AssignmentSource::Parameter,
        MemoryOperationId(NO_INDEX),
    );
    (owner, function)
}

#[test]
fn a_returned_borrow_is_tied_to_a_reference_parameter() {
    let mut program = program();
    borrowing_view(&mut program);
    let source = ported(&program);
    assert!(
        source.contains("pub fn make_view<'a>(bytes: &'a [u8]) -> View<'a> {"),
        "{source}"
    );
}

#[test]
fn a_returned_borrow_with_no_reference_parameter_leaves_the_function_out() {
    let mut program = program();
    let (_, function) = borrowing_view(&mut program);
    // The only parameter stops being a reference, so nothing in the signature
    // can carry the lifetime the return type needs.
    let row = zag_facts::tables::function_parameters(&program.tables.functions, function)
        .next()
        .expect("the function has a parameter");
    program.tables.parameters.parameter_type[row] = program.byte;
    assert!(
        !ported(&program).contains("pub fn make_view"),
        "{}",
        ported(&program)
    );
    assert_eq!(outcome(&program, function), Disposition::NotPorted);
}
