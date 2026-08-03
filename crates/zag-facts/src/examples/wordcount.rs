use crate::build::{
    declare_field, declare_function, declare_parameter, declare_struct, intern,
    push_allocator_source, push_call, push_call_argument, push_expression,
    push_field_assignment_with, push_integer_type, push_memory_operation, push_opaque_type,
    push_pointer_type, push_slice_type, push_void_type, set_function_signature, set_struct_deinit,
    struct_type,
};
use crate::handles::{FieldId, FunctionId, NO_INDEX, StringId, StructId};
use crate::tables::{
    AllocatorSourceKind, AssignmentSource, ExpressionKind, MemoryOperationKind,
    PARAMETER_FLAG_ALLOCATOR, PARAMETER_FLAG_MUTABLE, PlaceKind, Tables, empty_tables,
};

pub fn tables() -> Tables {
    let mut tables = empty_tables();
    tables.target = intern(&mut tables.strings, b"x86_64-linux");

    let byte = push_integer_type(&mut tables, 8, false);
    let word = push_integer_type(&mut tables, 32, false);
    let text = push_slice_type(&mut tables, byte);
    let allocator_name = intern(&mut tables.strings, b"Allocator");
    let allocator = push_opaque_type(&mut tables, allocator_name);

    let counts = declare_struct(&mut tables, b"Counts", 24, 8, 0);
    let counts_text = declare_field(&mut tables, counts, b"text", text, 0);
    declare_field(&mut tables, counts, b"words", word, 16);
    declare_field(&mut tables, counts, b"lines", word, 20);
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

    let allocate = push_memory_operation(
        &mut tables,
        initialize,
        MemoryOperationKind::Allocate,
        initialize_allocator,
        PlaceKind::FieldOfParameter,
        counts_text,
    );
    push_memory_operation(
        &mut tables,
        release,
        MemoryOperationKind::Free,
        release_allocator,
        PlaceKind::FieldOfParameter,
        counts_text,
    );
    // `words` and `lines` are counted in a loop, so nothing the frontend can
    // read assigns them and they carry no expression at all. `text` is the
    // copy the allocation makes.
    let copied = push_expression(
        &mut tables,
        ExpressionKind::Allocation,
        StringId(NO_INDEX),
        1,
        text,
        FieldId(NO_INDEX),
        &[],
    );
    push_field_assignment_with(
        &mut tables,
        counts_text,
        initialize,
        AssignmentSource::Allocation,
        allocate,
        copied,
    );

    tables
}
