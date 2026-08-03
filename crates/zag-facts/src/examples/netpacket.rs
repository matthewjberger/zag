use crate::build::{
    declare_field, declare_function, declare_parameter, declare_struct, intern, name_root_module,
    push_allocator_source, push_call, push_call_argument, push_expression,
    push_field_assignment_with, push_integer_type, push_memory_operation, push_opaque_type,
    push_pointer_type, push_slice_type, push_string, push_void_type, set_function_line,
    set_function_signature, set_struct_deinit, struct_type,
};
use crate::handles::{
    ExpressionId, FieldId, FunctionId, MemoryOperationId, NO_INDEX, StringId, StructId, TypeId,
};
use crate::tables::{
    AllocatorSourceKind, AssignmentSource, ExpressionKind, MemoryOperationKind,
    PARAMETER_FLAG_ALLOCATOR, PARAMETER_FLAG_MUTABLE, PlaceKind, STRUCT_FLAG_EXTERN, Tables,
    empty_tables,
};

pub fn tables() -> Tables {
    let mut tables = empty_tables();
    tables.target = intern(&mut tables.strings, b"x86_64-linux");
    name_root_module(&mut tables, b"", b"main.zig");

    let byte = push_integer_type(&mut tables, 8, false);
    let half = push_integer_type(&mut tables, 16, false);
    let word = push_integer_type(&mut tables, 32, false);
    let payload = push_slice_type(&mut tables, byte);
    let allocator_name = intern(&mut tables.strings, b"Allocator");
    let allocator = push_opaque_type(&mut tables, allocator_name);

    let header = declare_struct(&mut tables, b"Header", 12, 4, STRUCT_FLAG_EXTERN);
    let packet_header_magic = declare_field(&mut tables, header, b"magic", word, 0);
    let packet_header_version = declare_field(&mut tables, header, b"version", half, 4);
    let packet_header_flags = declare_field(&mut tables, header, b"flags", half, 6);
    let packet_header_length = declare_field(&mut tables, header, b"length", word, 8);

    let header_type = struct_type(&tables, header);
    let packet = declare_struct(&mut tables, b"Packet", 32, 8, 0);
    // Zig reorders an auto layout, so the header follows the slice in memory
    // while staying first in declaration order.
    let packet_header = declare_field(&mut tables, packet, b"header", header_type, 16);
    let packet_payload = declare_field(&mut tables, packet, b"payload", payload, 0);
    let packet_type = struct_type(&tables, packet);
    let packet_pointer = push_pointer_type(&mut tables, packet_type);

    let initialize = declare_function(&mut tables, b"init", packet);
    declare_parameter(
        &mut tables,
        initialize,
        b"allocator",
        allocator,
        PARAMETER_FLAG_ALLOCATOR,
    );
    declare_parameter(&mut tables, initialize, b"version", half, 0);
    declare_parameter(&mut tables, initialize, b"body", payload, 0);

    let deinitialize = declare_function(&mut tables, b"deinit", packet);
    declare_parameter(
        &mut tables,
        deinitialize,
        b"self",
        packet_pointer,
        PARAMETER_FLAG_MUTABLE,
    );
    declare_parameter(
        &mut tables,
        deinitialize,
        b"allocator",
        allocator,
        PARAMETER_FLAG_ALLOCATOR,
    );
    set_struct_deinit(&mut tables, packet, deinitialize);

    let main = declare_function(&mut tables, b"main", StructId(NO_INDEX));

    let void = push_void_type(&mut tables);
    set_function_signature(
        &mut tables,
        initialize,
        packet_type,
        StructId(NO_INDEX),
        true,
    );
    set_function_signature(&mut tables, deinitialize, void, StructId(NO_INDEX), false);
    set_function_signature(&mut tables, main, void, StructId(NO_INDEX), true);
    for (function, line) in [(initialize, 26), (deinitialize, 40), (main, 45)] {
        set_function_line(&mut tables, function, line);
    }

    let page = push_allocator_source(
        &mut tables,
        AllocatorSourceKind::Global,
        FunctionId(NO_INDEX),
        NO_INDEX,
    );
    let initialize_allocator =
        push_allocator_source(&mut tables, AllocatorSourceKind::Parameter, initialize, 0);
    let deinitialize_allocator =
        push_allocator_source(&mut tables, AllocatorSourceKind::Parameter, deinitialize, 1);

    let call = push_call(&mut tables, main, initialize);
    push_call_argument(&mut tables, call, 0, page);
    let call = push_call(&mut tables, main, deinitialize);
    push_call_argument(&mut tables, call, 1, page);

    let allocate = push_memory_operation(
        &mut tables,
        initialize,
        MemoryOperationKind::Allocate,
        initialize_allocator,
        PlaceKind::FieldOfParameter,
        packet_payload,
    );
    push_memory_operation(
        &mut tables,
        deinitialize,
        MemoryOperationKind::Free,
        deinitialize_allocator,
        PlaceKind::FieldOfParameter,
        packet_payload,
    );
    // The expressions the constructor is written from. `header` is a literal
    // whose fields are themselves expressions, and `payload` is the copy the
    // allocation makes.
    let magic = literal(&mut tables, b"0x5A414750", word);
    let magic = field_value(&mut tables, packet_header_magic, word, magic);
    let version = push_expression(
        &mut tables,
        ExpressionKind::Parameter,
        StringId(NO_INDEX),
        1,
        half,
        FieldId(NO_INDEX),
        &[],
    );
    let version = field_value(&mut tables, packet_header_version, half, version);
    let flags = literal(&mut tables, b"0", half);
    let flags = field_value(&mut tables, packet_header_flags, half, flags);
    let length = push_expression(
        &mut tables,
        ExpressionKind::Length,
        StringId(NO_INDEX),
        2,
        word,
        FieldId(NO_INDEX),
        &[],
    );
    let length = push_expression(
        &mut tables,
        ExpressionKind::Cast,
        StringId(NO_INDEX),
        NO_INDEX,
        word,
        FieldId(NO_INDEX),
        &[length],
    );
    let length = field_value(&mut tables, packet_header_length, word, length);
    let header = push_expression(
        &mut tables,
        ExpressionKind::StructLiteral,
        StringId(NO_INDEX),
        NO_INDEX,
        header_type,
        FieldId(NO_INDEX),
        &[magic, version, flags, length],
    );
    let copied = push_expression(
        &mut tables,
        ExpressionKind::Allocation,
        StringId(NO_INDEX),
        2,
        payload,
        FieldId(NO_INDEX),
        &[],
    );

    push_field_assignment_with(
        &mut tables,
        packet_header,
        initialize,
        AssignmentSource::Unknown,
        MemoryOperationId(NO_INDEX),
        header,
    );
    push_field_assignment_with(
        &mut tables,
        packet_payload,
        initialize,
        AssignmentSource::Allocation,
        allocate,
        copied,
    );

    tables
}

fn literal(tables: &mut Tables, text: &[u8], result: TypeId) -> ExpressionId {
    let interned = push_string(&mut tables.strings, text);
    push_expression(
        tables,
        ExpressionKind::Literal,
        interned,
        NO_INDEX,
        result,
        FieldId(NO_INDEX),
        &[],
    )
}

fn field_value(
    tables: &mut Tables,
    field: FieldId,
    result: TypeId,
    value: ExpressionId,
) -> ExpressionId {
    push_expression(
        tables,
        ExpressionKind::StructLiteral,
        StringId(NO_INDEX),
        NO_INDEX,
        result,
        field,
        &[value],
    )
}
