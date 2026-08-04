use crate::build::{
    intern, name_root_module, push_allocator_source, push_body_expression, push_call,
    push_call_argument, push_expression, push_field, push_field_assignment_at, push_function,
    push_integer_type, push_memory_operation, push_opaque_type, push_parameter, push_pointer_type,
    push_slice_type, push_string, push_struct, push_struct_type, push_void_type,
    set_expression_line, set_function_body, set_function_line, set_function_signature,
    set_struct_deinit,
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
    buffer_length: FieldId,
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
    let buffer_length = push_field(tables, buffer, field_name, types.unsigned_32, 16);

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
        buffer_length,
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
    entry_point: FunctionId,
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

    let name = intern(&mut tables.strings, b"main");
    let entry_point = push_function(tables, name, StructId(NO_INDEX));

    for (function, line) in [
        (initialize, 18),
        (deinitialize, 25),
        (release, 30),
        (make_buffer, 35),
        (parse_node, 50),
        (parse_tree, 59),
        (make_view, 68),
        (entry_point, 78),
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
    set_function_signature(tables, entry_point, void, no_set, true);

    FixtureFunctions {
        initialize,
        release,
        make_buffer,
        parse_node,
        parse_tree,
        make_view,
        entry_point,
    }
}

fn push_fixture_flow(
    tables: &mut Tables,
    types: &FixtureTypes,
    structs: &FixtureStructs,
    functions: &FixtureFunctions,
) {
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
    // `main` reaches all three, which is how it comes to fail with whatever
    // they fail with. It hands none of them an allocator.
    push_call(tables, functions.entry_point, functions.make_buffer);
    push_call(tables, functions.entry_point, functions.parse_tree);
    push_call(tables, functions.entry_point, functions.make_view);

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

    // `.data = try allocator.dupe(u8, bytes)` and `.length = @intCast(bytes.len)`,
    // which between them are what lets `init` come across as a constructor.
    let copied = allocation(tables, 1, types.slice_of_bytes, 20);
    push_field_assignment_at(
        tables,
        structs.buffer_data,
        functions.initialize,
        AssignmentSource::Allocation,
        allocate_data,
        copied,
        20,
    );
    let counted = push_expression(
        tables,
        crate::tables::ExpressionKind::Length,
        crate::handles::StringId(NO_INDEX),
        1,
        types.unsigned_32,
        FieldId(NO_INDEX),
        &[],
    );
    let counted = push_expression(
        tables,
        crate::tables::ExpressionKind::Cast,
        crate::handles::StringId(NO_INDEX),
        NO_INDEX,
        types.unsigned_32,
        FieldId(NO_INDEX),
        &[counted],
    );
    set_expression_line(tables, counted, 21);
    push_field_assignment_at(
        tables,
        structs.buffer_length,
        functions.initialize,
        AssignmentSource::Unknown,
        MemoryOperationId(NO_INDEX),
        counted,
        21,
    );

    let copied = allocation(tables, 1, types.slice_of_bytes, 52);
    push_field_assignment_at(
        tables,
        structs.node_label,
        functions.parse_node,
        AssignmentSource::Allocation,
        allocate_label,
        copied,
        52,
    );
    let empty = push_string(&mut tables.strings, b"&.{}");
    let empty = push_expression(
        tables,
        crate::tables::ExpressionKind::Literal,
        empty,
        NO_INDEX,
        types.slice_of_words,
        FieldId(NO_INDEX),
        &[],
    );
    set_expression_line(tables, empty, 53);
    push_field_assignment_at(
        tables,
        structs.node_children,
        functions.parse_node,
        AssignmentSource::StaticLiteral,
        MemoryOperationId(NO_INDEX),
        empty,
        53,
    );

    let borrowed = push_expression(
        tables,
        crate::tables::ExpressionKind::Parameter,
        crate::handles::StringId(NO_INDEX),
        0,
        types.slice_of_bytes,
        FieldId(NO_INDEX),
        &[],
    );
    set_expression_line(tables, borrowed, 69);
    push_field_assignment_at(
        tables,
        structs.view_bytes,
        functions.make_view,
        AssignmentSource::Parameter,
        MemoryOperationId(NO_INDEX),
        borrowed,
        69,
    );

    // `return .{ .bytes = bytes };`, which is the whole body. The literal
    // carries the struct it builds and each member carries the field it fills,
    // the same shape a constructor is written from.
    let named = push_string(&mut tables.strings, b"bytes");
    let read = push_expression(
        tables,
        crate::tables::ExpressionKind::Identifier,
        named,
        crate::handles::NO_INDEX,
        TypeId(NO_INDEX),
        FieldId(NO_INDEX),
        &[],
    );
    set_expression_line(tables, read, 69);
    let member = push_expression(
        tables,
        crate::tables::ExpressionKind::StructLiteral,
        crate::handles::StringId(NO_INDEX),
        crate::handles::NO_INDEX,
        TypeId(NO_INDEX),
        structs.view_bytes,
        &[read],
    );
    set_expression_line(tables, member, 69);
    let literal = push_expression(
        tables,
        crate::tables::ExpressionKind::StructLiteral,
        crate::handles::StringId(NO_INDEX),
        crate::handles::NO_INDEX,
        types.view,
        FieldId(NO_INDEX),
        &[member],
    );
    set_expression_line(tables, literal, 69);
    let body = push_body_expression(
        tables,
        crate::tables::ExpressionKind::Block,
        crate::handles::StringId(NO_INDEX),
        69,
        &[literal],
    );
    set_function_body(tables, functions.make_view, body);
}

fn allocation(
    tables: &mut Tables,
    parameter: u32,
    result: TypeId,
    line: u32,
) -> crate::handles::ExpressionId {
    let expression = push_expression(
        tables,
        crate::tables::ExpressionKind::Allocation,
        crate::handles::StringId(NO_INDEX),
        parameter,
        result,
        FieldId(NO_INDEX),
        &[],
    );
    set_expression_line(tables, expression, line);
    expression
}

pub fn example_tables() -> Tables {
    let mut tables = empty_tables();
    tables.target = intern(&mut tables.strings, b"x86_64-linux");
    name_root_module(&mut tables, b"", b"main.zig");
    let types = push_fixture_types(&mut tables);
    let structs = push_fixture_structs(&mut tables, &types);
    let functions = push_fixture_functions(&mut tables, &types, &structs);
    push_fixture_flow(&mut tables, &types, &structs, &functions);
    tables
}
