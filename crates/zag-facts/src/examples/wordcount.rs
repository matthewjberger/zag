use crate::build::{
    declare_field, declare_function, declare_parameter, declare_struct, intern, name_root_module,
    push_allocator_source, push_call, push_call_argument, push_expression,
    push_field_assignment_at, push_integer_type, push_memory_operation, push_opaque_type,
    push_pointer_type, push_slice_type, push_string, push_void_type, set_expression_line,
    set_function_line, set_function_signature, set_struct_deinit, struct_type,
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
    let allocator_name = intern(&mut tables.strings, b"Allocator");
    let allocator = push_opaque_type(&mut tables, allocator_name);

    let counts = declare_struct(&mut tables, b"Counts", 40, 8, 0);
    let counts_text = declare_field(&mut tables, counts, b"text", text, 0);
    let counts_name = declare_field(&mut tables, counts, b"name", text, 16);
    let counts_words = declare_field(&mut tables, counts, b"words", word, 32);
    let counts_lines = declare_field(&mut tables, counts, b"lines", word, 36);
    let counts_type = struct_type(&tables, counts);
    let counts_pointer = push_pointer_type(&mut tables, counts_type);

    let initialize = declare_function(&mut tables, b"init", counts);
    declare_parameter(
        &mut tables,
        initialize,
        b"allocator",
        allocator,
        PARAMETER_FLAG_ALLOCATOR,
    );
    declare_parameter(&mut tables, initialize, b"name", text, 0);
    declare_parameter(&mut tables, initialize, b"input", text, 0);

    let deinitialize = declare_function(&mut tables, b"deinit", counts);
    declare_parameter(
        &mut tables,
        deinitialize,
        b"self",
        counts_pointer,
        PARAMETER_FLAG_MUTABLE,
    );
    declare_parameter(
        &mut tables,
        deinitialize,
        b"allocator",
        allocator,
        PARAMETER_FLAG_ALLOCATOR,
    );
    set_struct_deinit(&mut tables, counts, deinitialize);

    let release = declare_function(&mut tables, b"release", StructId(NO_INDEX));
    declare_parameter(
        &mut tables,
        release,
        b"self",
        counts_pointer,
        PARAMETER_FLAG_MUTABLE,
    );
    declare_parameter(
        &mut tables,
        release,
        b"allocator",
        allocator,
        PARAMETER_FLAG_ALLOCATOR,
    );

    let main = declare_function(&mut tables, b"main", StructId(NO_INDEX));

    let void = push_void_type(&mut tables);
    set_function_signature(
        &mut tables,
        initialize,
        counts_type,
        StructId(NO_INDEX),
        true,
    );
    set_function_signature(&mut tables, deinitialize, void, StructId(NO_INDEX), false);
    set_function_signature(&mut tables, release, void, StructId(NO_INDEX), false);
    set_function_signature(&mut tables, main, void, StructId(NO_INDEX), true);
    for (function, line) in [
        (initialize, 10),
        (deinitialize, 33),
        (release, 38),
        (main, 43),
    ] {
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
    let release_allocator =
        push_allocator_source(&mut tables, AllocatorSourceKind::Parameter, release, 1);

    let call = push_call(&mut tables, deinitialize, release);
    push_call_argument(&mut tables, call, 1, deinitialize_allocator);
    let call = push_call(&mut tables, main, initialize);
    push_call_argument(&mut tables, call, 0, page);
    let call = push_call(&mut tables, main, deinitialize);
    push_call_argument(&mut tables, call, 1, page);

    // Two owned fields freed by one function, which is the ordinary shape and
    // the one where an argument belonging to the wrong call costs a field its
    // ownership without anything else changing.
    for (field, parameter, line) in [(counts_text, 2u32, 24u32), (counts_name, 1, 25)] {
        let allocate = push_memory_operation(
            &mut tables,
            initialize,
            MemoryOperationKind::Allocate,
            initialize_allocator,
            PlaceKind::FieldOfParameter,
            field,
        );
        push_memory_operation(
            &mut tables,
            release,
            MemoryOperationKind::Free,
            release_allocator,
            PlaceKind::FieldOfParameter,
            field,
        );
        let copied = push_expression(
            &mut tables,
            ExpressionKind::Allocation,
            StringId(NO_INDEX),
            parameter,
            text,
            FieldId(NO_INDEX),
            &[],
        );
        set_expression_line(&mut tables, copied, line);
        push_field_assignment_at(
            &mut tables,
            field,
            initialize,
            AssignmentSource::Allocation,
            allocate,
            copied,
            line,
        );
    }
    // `words` and `lines` are set from locals a loop filled, which the port
    // cannot read, so each carries the Zig it could not spell rather than
    // nothing at all.
    for (field, name, line) in [
        (counts_words, b"words".as_slice(), 26),
        (counts_lines, b"lines".as_slice(), 27),
    ] {
        let spelled = push_string(&mut tables.strings, name);
        let unreadable = push_expression(
            &mut tables,
            ExpressionKind::Unsupported,
            spelled,
            NO_INDEX,
            word,
            FieldId(NO_INDEX),
            &[],
        );
        set_expression_line(&mut tables, unreadable, line);
        push_field_assignment_at(
            &mut tables,
            field,
            initialize,
            AssignmentSource::Unknown,
            MemoryOperationId(NO_INDEX),
            unreadable,
            line,
        );
    }

    tables
}
