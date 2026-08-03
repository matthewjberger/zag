use crate::build::{
    declare_field, declare_function, declare_parameter, declare_struct, intern,
    push_allocator_source, push_call, push_call_argument, push_field_assignment, push_integer_type,
    push_memory_operation, push_opaque_type, push_slice_type, push_void_type,
    set_function_signature, struct_type,
};
use crate::handles::{FunctionId, MemoryOperationId, NO_INDEX, StructId};
use crate::tables::{
    AllocatorSourceKind, AssignmentSource, MemoryOperationKind, PARAMETER_FLAG_ALLOCATOR,
    PlaceKind, Tables, empty_tables,
};

pub fn tables() -> Tables {
    let mut tables = empty_tables();
    tables.target = intern(&mut tables.strings, b"x86_64-linux");

    let byte = push_integer_type(&mut tables, 8, false);
    let word = push_integer_type(&mut tables, 32, false);
    let text = push_slice_type(&mut tables, byte);
    let allocator_name = intern(&mut tables.strings, b"Allocator");
    let allocator = push_opaque_type(&mut tables, allocator_name);

    let token = declare_struct(&mut tables, b"Token", 24, 8, 0);
    let token_text = declare_field(&mut tables, token, b"text", text, 0);
    declare_field(&mut tables, token, b"length", word, 16);
    let token_type = struct_type(&tables, token);
    let token_slice = push_slice_type(&mut tables, token_type);

    let document = declare_struct(&mut tables, b"Document", 48, 8, 0);
    let document_source = declare_field(&mut tables, document, b"source", text, 0);
    let document_tokens = declare_field(&mut tables, document, b"tokens", token_slice, 16);
    let document_separators = declare_field(&mut tables, document, b"separators", text, 32);

    let parse = declare_function(&mut tables, b"parse", StructId(NO_INDEX));
    declare_parameter(
        &mut tables,
        parse,
        b"arena",
        allocator,
        PARAMETER_FLAG_ALLOCATOR,
    );
    declare_parameter(&mut tables, parse, b"source", text, 0);

    let main = declare_function(&mut tables, b"main", StructId(NO_INDEX));

    let document_type = struct_type(&tables, document);
    let void = push_void_type(&mut tables);
    set_function_signature(&mut tables, parse, document_type, StructId(NO_INDEX), true);
    set_function_signature(&mut tables, main, void, StructId(NO_INDEX), true);

    let arena = push_allocator_source(
        &mut tables,
        AllocatorSourceKind::Arena,
        FunctionId(NO_INDEX),
        NO_INDEX,
    );
    let parse_allocator =
        push_allocator_source(&mut tables, AllocatorSourceKind::Parameter, parse, 0);

    let call = push_call(&mut tables, main, parse);
    push_call_argument(&mut tables, call, 0, arena);

    let allocate_text = push_memory_operation(
        &mut tables,
        parse,
        MemoryOperationKind::Allocate,
        parse_allocator,
        PlaceKind::FieldOfParameter,
        token_text,
    );
    let allocate_tokens = push_memory_operation(
        &mut tables,
        parse,
        MemoryOperationKind::Allocate,
        parse_allocator,
        PlaceKind::FieldOfParameter,
        document_tokens,
    );

    push_field_assignment(
        &mut tables,
        token_text,
        parse,
        AssignmentSource::Allocation,
        allocate_text,
    );
    push_field_assignment(
        &mut tables,
        document_source,
        parse,
        AssignmentSource::Parameter,
        MemoryOperationId(NO_INDEX),
    );
    push_field_assignment(
        &mut tables,
        document_tokens,
        parse,
        AssignmentSource::Allocation,
        allocate_tokens,
    );
    push_field_assignment(
        &mut tables,
        document_separators,
        parse,
        AssignmentSource::StaticLiteral,
        MemoryOperationId(NO_INDEX),
    );

    tables
}
