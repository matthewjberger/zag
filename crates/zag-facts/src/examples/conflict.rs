use crate::build::{
    declare_field, declare_function, declare_parameter, declare_struct, intern,
    push_allocator_source, push_call, push_call_argument, push_field_assignment, push_integer_type,
    push_memory_operation, push_opaque_type, push_slice_type, push_void_type,
    set_function_signature, struct_type,
};
use crate::handles::{FieldId, FunctionId, NO_INDEX, StructId};
use crate::tables::{
    AllocatorSourceKind, AssignmentSource, MemoryOperationKind, PARAMETER_FLAG_ALLOCATOR,
    PlaceKind, Tables, empty_tables,
};

pub fn tables() -> Tables {
    let mut tables = empty_tables();
    tables.target = intern(&mut tables.strings, b"x86_64-linux");

    let byte = push_integer_type(&mut tables, 8, false);
    let bytes = push_slice_type(&mut tables, byte);
    let allocator_name = intern(&mut tables.strings, b"Allocator");
    let allocator = push_opaque_type(&mut tables, allocator_name);

    let cache = declare_struct(&mut tables, b"Cache", 16, 8, 0);
    let cache_entries = declare_field(&mut tables, cache, b"entries", bytes, 0);

    let make_cache = declare_function(&mut tables, b"makeCache", StructId(NO_INDEX));
    declare_parameter(
        &mut tables,
        make_cache,
        b"allocator",
        allocator,
        PARAMETER_FLAG_ALLOCATOR,
    );
    declare_parameter(&mut tables, make_cache, b"bytes", bytes, 0);

    let from_heap = declare_function(&mut tables, b"fromHeap", StructId(NO_INDEX));
    declare_parameter(&mut tables, from_heap, b"bytes", bytes, 0);

    let from_arena = declare_function(&mut tables, b"fromArena", StructId(NO_INDEX));
    declare_parameter(
        &mut tables,
        from_arena,
        b"arena",
        allocator,
        PARAMETER_FLAG_ALLOCATOR,
    );
    declare_parameter(&mut tables, from_arena, b"bytes", bytes, 0);

    let main = declare_function(&mut tables, b"main", StructId(NO_INDEX));

    let cache_type = struct_type(&tables, cache);
    let void = push_void_type(&mut tables);
    for function in [make_cache, from_heap, from_arena] {
        set_function_signature(&mut tables, function, cache_type, StructId(NO_INDEX), true);
    }
    set_function_signature(&mut tables, main, void, StructId(NO_INDEX), true);

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
    let make_cache_allocator =
        push_allocator_source(&mut tables, AllocatorSourceKind::Parameter, make_cache, 0);
    let from_arena_allocator =
        push_allocator_source(&mut tables, AllocatorSourceKind::Parameter, from_arena, 0);

    let call = push_call(&mut tables, from_heap, make_cache);
    push_call_argument(&mut tables, call, 0, page);
    let call = push_call(&mut tables, from_arena, make_cache);
    push_call_argument(&mut tables, call, 0, from_arena_allocator);
    push_call(&mut tables, main, from_heap);
    let call = push_call(&mut tables, main, from_arena);
    push_call_argument(&mut tables, call, 0, arena);

    let allocate = push_memory_operation(
        &mut tables,
        make_cache,
        MemoryOperationKind::Allocate,
        make_cache_allocator,
        PlaceKind::FieldOfParameter,
        cache_entries,
    );
    // main frees a local it happens to know the allocator of. A frontend
    // cannot attribute that to the field, so the free carries no field and the
    // ownership pass never sees it.
    push_memory_operation(
        &mut tables,
        main,
        MemoryOperationKind::Free,
        page,
        PlaceKind::Local,
        FieldId(NO_INDEX),
    );
    push_field_assignment(
        &mut tables,
        cache_entries,
        make_cache,
        AssignmentSource::Allocation,
        allocate,
    );

    tables
}
