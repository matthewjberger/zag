use crate::build::{
    declare_field, declare_function, declare_parameter, declare_struct, intern, name_root_module,
    push_allocator_source, push_array_type, push_call, push_call_argument, push_expression,
    push_field_assignment_with, push_integer_type, push_memory_operation, push_opaque_type,
    push_optional_type, push_pointer_type, push_slice_type, push_void_type, set_function_line,
    set_function_signature, set_struct_deinit, struct_type,
};
use crate::handles::{FieldId, FunctionId, MemoryOperationId, NO_INDEX, StringId, StructId};
use crate::tables::{
    AllocatorSourceKind, AssignmentSource, ExpressionKind, MemoryOperationKind,
    PARAMETER_FLAG_ALLOCATOR, PARAMETER_FLAG_MUTABLE, PlaceKind, Tables, empty_tables,
};

pub fn tables() -> Tables {
    let mut tables = empty_tables();
    tables.target = intern(&mut tables.strings, b"x86_64-linux");
    name_root_module(&mut tables, b"", b"main.zig");

    let byte = push_integer_type(&mut tables, 8, false);
    let word = push_integer_type(&mut tables, 32, false);
    let text = push_slice_type(&mut tables, byte);
    let channels = push_array_type(&mut tables, word, 4, 16);
    let label = push_optional_type(&mut tables, text);
    let allocator_name = intern(&mut tables.strings, b"Allocator");
    let allocator = push_opaque_type(&mut tables, allocator_name);

    let frame = declare_struct(&mut tables, b"Frame", 48, 8, 0);
    let frame_channels = declare_field(&mut tables, frame, b"channels", channels, 32);
    let frame_label = declare_field(&mut tables, frame, b"label", label, 0);
    let frame_source = declare_field(&mut tables, frame, b"source", text, 16);
    let frame_type = struct_type(&tables, frame);
    let frame_pointer = push_pointer_type(&mut tables, frame_type);

    let initialize = declare_function(&mut tables, b"init", frame);
    declare_parameter(
        &mut tables,
        initialize,
        b"allocator",
        allocator,
        PARAMETER_FLAG_ALLOCATOR,
    );
    declare_parameter(&mut tables, initialize, b"channels", channels, 0);
    declare_parameter(&mut tables, initialize, b"source", text, 0);

    let deinitialize = declare_function(&mut tables, b"deinit", frame);
    declare_parameter(
        &mut tables,
        deinitialize,
        b"self",
        frame_pointer,
        PARAMETER_FLAG_MUTABLE,
    );
    declare_parameter(
        &mut tables,
        deinitialize,
        b"allocator",
        allocator,
        PARAMETER_FLAG_ALLOCATOR,
    );
    set_struct_deinit(&mut tables, frame, deinitialize);

    let main = declare_function(&mut tables, b"main", StructId(NO_INDEX));

    let void = push_void_type(&mut tables);
    set_function_signature(
        &mut tables,
        initialize,
        frame_type,
        StructId(NO_INDEX),
        true,
    );
    set_function_signature(&mut tables, deinitialize, void, StructId(NO_INDEX), false);
    set_function_signature(&mut tables, main, void, StructId(NO_INDEX), true);
    for (function, line) in [(initialize, 10), (deinitialize, 18), (main, 23)] {
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
        frame_source,
    );
    push_memory_operation(
        &mut tables,
        deinitialize,
        MemoryOperationKind::Free,
        deinitialize_allocator,
        PlaceKind::FieldOfParameter,
        frame_source,
    );

    let handed_over = push_expression(
        &mut tables,
        ExpressionKind::Parameter,
        StringId(NO_INDEX),
        1,
        channels,
        FieldId(NO_INDEX),
        &[],
    );
    let absent = push_expression(
        &mut tables,
        ExpressionKind::Null,
        StringId(NO_INDEX),
        NO_INDEX,
        label,
        FieldId(NO_INDEX),
        &[],
    );
    let copied = push_expression(
        &mut tables,
        ExpressionKind::Allocation,
        StringId(NO_INDEX),
        2,
        text,
        FieldId(NO_INDEX),
        &[],
    );

    push_field_assignment_with(
        &mut tables,
        frame_channels,
        initialize,
        AssignmentSource::Parameter,
        MemoryOperationId(NO_INDEX),
        handed_over,
    );
    push_field_assignment_with(
        &mut tables,
        frame_label,
        initialize,
        AssignmentSource::StaticLiteral,
        MemoryOperationId(NO_INDEX),
        absent,
    );
    push_field_assignment_with(
        &mut tables,
        frame_source,
        initialize,
        AssignmentSource::Allocation,
        allocate,
        copied,
    );

    tables
}
