use crate::build::{
    intern, name_root_module, push_allocator_source, push_call, push_call_argument, push_field,
    push_field_assignment, push_function, push_integer_type, push_memory_operation,
    push_opaque_type, push_parameter, push_pointer_type, push_slice_type, push_struct,
    push_struct_type, push_void_type, set_function_line, set_function_signature, set_struct_deinit,
};
use crate::handles::{FieldId, FunctionId, MemoryOperationId, NO_INDEX, StructId, TypeId};
use crate::tables::{
    AllocatorSourceKind, AssignmentSource, MemoryOperationKind, PARAMETER_FLAG_ALLOCATOR,
    PARAMETER_FLAG_MUTABLE, PlaceKind, STRUCT_FLAG_EXTERN, Tables, empty_tables,
};

struct FixtureTypes {
    unsigned_16: TypeId,
    unsigned_32: TypeId,
    slice_of_bytes: TypeId,
    slice_of_words: TypeId,
    allocator: TypeId,
    pointer_to_buffer: TypeId,
    buffer: TypeId,
    header: TypeId,
    node: TypeId,
    view: TypeId,
    cache: TypeId,
}

fn push_fixture_types(tables: &mut Tables) -> FixtureTypes {
    let unsigned_8 = push_integer_type(tables, 8, false);
    let unsigned_16 = push_integer_type(tables, 16, false);
    let unsigned_32 = push_integer_type(tables, 32, false);
    let slice_of_bytes = push_slice_type(tables, unsigned_8);
    let slice_of_words = push_slice_type(tables, unsigned_32);
    let allocator_name = intern(&mut tables.strings, b"Allocator");
    let allocator = push_opaque_type(tables, allocator_name);
    let buffer_name = intern(&mut tables.strings, b"Buffer");
    let buffer = push_struct_type(tables, buffer_name, 24, 8);
    let pointer_to_buffer = push_pointer_type(tables, buffer);
    let header_name = intern(&mut tables.strings, b"Header");
    let header = push_struct_type(tables, header_name, 8, 4);
    let node_name = intern(&mut tables.strings, b"Node");
    let node = push_struct_type(tables, node_name, 32, 8);
    let view_name = intern(&mut tables.strings, b"View");
    let view = push_struct_type(tables, view_name, 16, 8);
    let cache_name = intern(&mut tables.strings, b"Cache");
    let cache = push_struct_type(tables, cache_name, 16, 8);
    FixtureTypes {
        unsigned_16,
        unsigned_32,
        slice_of_bytes,
        slice_of_words,
        allocator,
        pointer_to_buffer,
        buffer,
        header,
        node,
        view,
        cache,
    }
}

struct FixtureStructs {
    buffer: StructId,
    buffer_data: FieldId,
    node_label: FieldId,
    node_children: FieldId,
    view_bytes: FieldId,
}

fn push_fixture_structs(tables: &mut Tables, types: &FixtureTypes) -> FixtureStructs {
    let name = intern(&mut tables.strings, b"Buffer");
    let buffer = push_struct(tables, name, types.buffer, 24, 8, 0);
    let field_name = intern(&mut tables.strings, b"data");
    let buffer_data = push_field(tables, buffer, field_name, types.slice_of_bytes, 0);
    let field_name = intern(&mut tables.strings, b"length");
    push_field(tables, buffer, field_name, types.unsigned_32, 16);

    let name = intern(&mut tables.strings, b"Header");
    let header = push_struct(tables, name, types.header, 8, 4, STRUCT_FLAG_EXTERN);
    let field_name = intern(&mut tables.strings, b"magic");
    push_field(tables, header, field_name, types.unsigned_32, 0);
    let field_name = intern(&mut tables.strings, b"version");
    push_field(tables, header, field_name, types.unsigned_16, 4);
    let field_name = intern(&mut tables.strings, b"flags");
    push_field(tables, header, field_name, types.unsigned_16, 6);

    let name = intern(&mut tables.strings, b"Node");
    let node = push_struct(tables, name, types.node, 32, 8, 0);
    let field_name = intern(&mut tables.strings, b"label");
    let node_label = push_field(tables, node, field_name, types.slice_of_bytes, 0);
    let field_name = intern(&mut tables.strings, b"children");
    let node_children = push_field(tables, node, field_name, types.slice_of_words, 16);

    let name = intern(&mut tables.strings, b"View");
    let view = push_struct(tables, name, types.view, 16, 8, 0);
    let field_name = intern(&mut tables.strings, b"bytes");
    let view_bytes = push_field(tables, view, field_name, types.slice_of_bytes, 0);

    let name = intern(&mut tables.strings, b"Cache");
    let cache = push_struct(tables, name, types.cache, 16, 8, 0);
    let field_name = intern(&mut tables.strings, b"entries");
    push_field(tables, cache, field_name, types.slice_of_bytes, 0);

    FixtureStructs {
        buffer,
        buffer_data,
        node_label,
        node_children,
        view_bytes,
    }
}

struct FixtureFunctions {
    initialize: FunctionId,
    release: FunctionId,
    make_buffer: FunctionId,
    parse_node: FunctionId,
    parse_tree: FunctionId,
    make_view: FunctionId,
}

fn push_allocator_parameter(tables: &mut Tables, owner: FunctionId, name: &[u8], kind: TypeId) {
    let parameter_name = intern(&mut tables.strings, name);
    push_parameter(
        tables,
        owner,
        parameter_name,
        kind,
        PARAMETER_FLAG_ALLOCATOR,
    );
}

fn push_plain_parameter(tables: &mut Tables, owner: FunctionId, name: &[u8], kind: TypeId) {
    let parameter_name = intern(&mut tables.strings, name);
    push_parameter(tables, owner, parameter_name, kind, 0);
}

fn push_receiver_parameter(tables: &mut Tables, owner: FunctionId, name: &[u8], kind: TypeId) {
    let parameter_name = intern(&mut tables.strings, name);
    push_parameter(tables, owner, parameter_name, kind, PARAMETER_FLAG_MUTABLE);
}

fn push_fixture_functions(
    tables: &mut Tables,
    types: &FixtureTypes,
    structs: &FixtureStructs,
) -> FixtureFunctions {
    let name = intern(&mut tables.strings, b"init");
    let initialize = push_function(tables, name, structs.buffer);
    push_allocator_parameter(tables, initialize, b"allocator", types.allocator);
    push_plain_parameter(tables, initialize, b"bytes", types.slice_of_bytes);

    let name = intern(&mut tables.strings, b"deinit");
    let deinitialize = push_function(tables, name, structs.buffer);
    push_receiver_parameter(tables, deinitialize, b"self", types.pointer_to_buffer);
    push_allocator_parameter(tables, deinitialize, b"allocator", types.allocator);
    set_struct_deinit(tables, structs.buffer, deinitialize);

    let name = intern(&mut tables.strings, b"release");
    let release = push_function(tables, name, StructId(NO_INDEX));
    push_receiver_parameter(tables, release, b"self", types.pointer_to_buffer);
    push_allocator_parameter(tables, release, b"allocator", types.allocator);

    let name = intern(&mut tables.strings, b"makeBuffer");
    let make_buffer = push_function(tables, name, StructId(NO_INDEX));
    push_plain_parameter(tables, make_buffer, b"bytes", types.slice_of_bytes);

    let name = intern(&mut tables.strings, b"parseNode");
    let parse_node = push_function(tables, name, StructId(NO_INDEX));
    push_allocator_parameter(tables, parse_node, b"arena", types.allocator);
    push_plain_parameter(tables, parse_node, b"text", types.slice_of_bytes);

    let name = intern(&mut tables.strings, b"parseTree");
    let parse_tree = push_function(tables, name, StructId(NO_INDEX));
    push_plain_parameter(tables, parse_tree, b"text", types.slice_of_bytes);

    let name = intern(&mut tables.strings, b"makeView");
    let make_view = push_function(tables, name, StructId(NO_INDEX));
    push_plain_parameter(tables, make_view, b"bytes", types.slice_of_bytes);

    for (function, line) in [
        (initialize, 16),
        (deinitialize, 23),
        (release, 28),
        (make_buffer, 33),
        (parse_node, 48),
        (parse_tree, 57),
        (make_view, 66),
    ] {
        set_function_line(tables, function, line);
    }

    let void = push_void_type(tables);
    let no_set = StructId(NO_INDEX);
    set_function_signature(tables, initialize, types.buffer, no_set, true);
    set_function_signature(tables, deinitialize, void, no_set, false);
    set_function_signature(tables, release, void, no_set, false);
    set_function_signature(tables, make_buffer, types.buffer, no_set, true);
    set_function_signature(tables, parse_node, types.node, no_set, true);
    set_function_signature(tables, parse_tree, types.node, no_set, true);
    set_function_signature(tables, make_view, types.view, no_set, false);

    FixtureFunctions {
        initialize,
        release,
        make_buffer,
        parse_node,
        parse_tree,
        make_view,
    }
}

fn push_fixture_flow(tables: &mut Tables, structs: &FixtureStructs, functions: &FixtureFunctions) {
    let deinitialize = tables.structs.deinit[structs.buffer.0 as usize];

    let global = push_allocator_source(
        tables,
        AllocatorSourceKind::Global,
        FunctionId(NO_INDEX),
        NO_INDEX,
    );
    let arena = push_allocator_source(
        tables,
        AllocatorSourceKind::Arena,
        FunctionId(NO_INDEX),
        NO_INDEX,
    );
    let initialize_allocator = push_allocator_source(
        tables,
        AllocatorSourceKind::Parameter,
        functions.initialize,
        0,
    );
    let deinitialize_allocator =
        push_allocator_source(tables, AllocatorSourceKind::Parameter, deinitialize, 1);
    let release_allocator =
        push_allocator_source(tables, AllocatorSourceKind::Parameter, functions.release, 1);
    let parse_node_allocator = push_allocator_source(
        tables,
        AllocatorSourceKind::Parameter,
        functions.parse_node,
        0,
    );

    let call = push_call(tables, deinitialize, functions.release);
    push_call_argument(tables, call, 1, deinitialize_allocator);
    let call = push_call(tables, functions.make_buffer, functions.initialize);
    push_call_argument(tables, call, 0, global);
    let call = push_call(tables, functions.parse_tree, functions.parse_node);
    push_call_argument(tables, call, 0, arena);

    let allocate_data = push_memory_operation(
        tables,
        functions.initialize,
        MemoryOperationKind::Allocate,
        initialize_allocator,
        PlaceKind::FieldOfParameter,
        structs.buffer_data,
    );
    push_memory_operation(
        tables,
        functions.release,
        MemoryOperationKind::Free,
        release_allocator,
        PlaceKind::FieldOfParameter,
        structs.buffer_data,
    );
    let allocate_label = push_memory_operation(
        tables,
        functions.parse_node,
        MemoryOperationKind::Allocate,
        parse_node_allocator,
        PlaceKind::FieldOfParameter,
        structs.node_label,
    );

    push_field_assignment(
        tables,
        structs.buffer_data,
        functions.initialize,
        AssignmentSource::Allocation,
        allocate_data,
    );
    push_field_assignment(
        tables,
        structs.node_label,
        functions.parse_node,
        AssignmentSource::Allocation,
        allocate_label,
    );
    push_field_assignment(
        tables,
        structs.node_children,
        functions.parse_node,
        AssignmentSource::StaticLiteral,
        MemoryOperationId(NO_INDEX),
    );
    push_field_assignment(
        tables,
        structs.view_bytes,
        functions.make_view,
        AssignmentSource::Parameter,
        MemoryOperationId(NO_INDEX),
    );
}

pub fn example_tables() -> Tables {
    let mut tables = empty_tables();
    tables.target = intern(&mut tables.strings, b"x86_64-linux");
    name_root_module(&mut tables, b"", b"example.zig");
    let types = push_fixture_types(&mut tables);
    let structs = push_fixture_structs(&mut tables, &types);
    let functions = push_fixture_functions(&mut tables, &types, &structs);
    push_fixture_flow(&mut tables, &structs, &functions);
    tables
}
