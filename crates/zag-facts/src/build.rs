use crate::handles::{
    AllocatorSourceId, CallId, ExpressionId, FieldId, FunctionId, MemoryOperationId, StringId,
    StructId, TypeId,
};
use crate::tables::{
    AllocatorSourceKind, AssignmentSource, ExpressionKind, MemoryOperationKind, PlaceKind, Strings,
    Tables, TypeKind, string_bytes, string_count,
};

/// Appends without looking for an existing copy. Use this where identifiers
/// are written once and never compared, such as the emitted syntax tree, where
/// `intern` would be quadratic in the number of identifiers written.
pub fn push_string(strings: &mut Strings, text: &[u8]) -> StringId {
    if strings.offsets.is_empty() {
        strings.offsets.push(0);
    }
    strings.bytes.extend_from_slice(text);
    strings.offsets.push(strings.bytes.len() as u32);
    StringId(string_count(strings) as u32 - 1)
}

/// Deduplicating append, for the fact tables. The Zig frontend hands over an
/// already interned blob, so this only runs while building tables by hand.
pub fn intern(strings: &mut Strings, text: &[u8]) -> StringId {
    for index in 0..string_count(strings) {
        if string_bytes(strings, StringId(index as u32)) == text {
            return StringId(index as u32);
        }
    }
    push_string(strings, text)
}

pub fn push_integer_type(tables: &mut Tables, bit_width: u32, signed: bool) -> TypeId {
    let bytes = bit_width.div_ceil(8);
    let flags = if signed {
        crate::tables::TYPE_FLAG_SIGNED
    } else {
        0
    };
    push_type_row(
        tables,
        TypeKind::Integer,
        TypeId(crate::handles::NO_INDEX),
        StringId(crate::handles::NO_INDEX),
        bytes,
        bytes,
        bit_width,
        flags,
    )
}

pub fn push_slice_type(tables: &mut Tables, element: TypeId) -> TypeId {
    push_type_row(
        tables,
        TypeKind::Slice,
        element,
        StringId(crate::handles::NO_INDEX),
        16,
        8,
        0,
        0,
    )
}

pub fn push_pointer_type(tables: &mut Tables, element: TypeId) -> TypeId {
    push_type_row(
        tables,
        TypeKind::Pointer,
        element,
        StringId(crate::handles::NO_INDEX),
        8,
        8,
        0,
        0,
    )
}

pub fn push_void_type(tables: &mut Tables) -> TypeId {
    push_type_row(
        tables,
        TypeKind::Void,
        TypeId(crate::handles::NO_INDEX),
        StringId(crate::handles::NO_INDEX),
        0,
        1,
        0,
        0,
    )
}

pub fn push_opaque_type(tables: &mut Tables, name: StringId) -> TypeId {
    push_type_row(
        tables,
        TypeKind::Opaque,
        TypeId(crate::handles::NO_INDEX),
        name,
        0,
        1,
        0,
        0,
    )
}

pub fn push_struct_type(tables: &mut Tables, name: StringId, size: u32, alignment: u32) -> TypeId {
    push_type_row(
        tables,
        TypeKind::Struct,
        TypeId(crate::handles::NO_INDEX),
        name,
        size,
        alignment,
        0,
        0,
    )
}

fn push_type_row(
    tables: &mut Tables,
    kind: TypeKind,
    element: TypeId,
    name: StringId,
    size: u32,
    alignment: u32,
    bit_width: u32,
    flags: u32,
) -> TypeId {
    let types = &mut tables.types;
    types.kind.push(kind);
    types.element.push(element);
    types.name.push(name);
    types.size.push(size);
    types.alignment.push(alignment);
    types.bit_width.push(bit_width);
    types.flags.push(flags);
    TypeId(types.kind.len() as u32 - 1)
}

/// Creates the struct type and the struct row together, which is how every
/// caller wants them.
pub fn declare_struct(
    tables: &mut Tables,
    name: &[u8],
    size: u32,
    alignment: u32,
    flags: u32,
) -> StructId {
    let interned = push_string(&mut tables.strings, name);
    let kind = push_struct_type(tables, interned, size, alignment);
    push_struct(tables, interned, kind, size, alignment, flags)
}

pub fn struct_type(tables: &Tables, owner: StructId) -> TypeId {
    tables
        .structs
        .type_id
        .get(owner.0 as usize)
        .copied()
        .unwrap_or(TypeId(crate::handles::NO_INDEX))
}

pub fn declare_field(
    tables: &mut Tables,
    owner: StructId,
    name: &[u8],
    field_type: TypeId,
    offset: u32,
) -> FieldId {
    let interned = push_string(&mut tables.strings, name);
    push_field(tables, owner, interned, field_type, offset)
}

pub fn declare_function(tables: &mut Tables, name: &[u8], owner: StructId) -> FunctionId {
    let interned = push_string(&mut tables.strings, name);
    push_function(tables, interned, owner)
}

pub fn declare_parameter(
    tables: &mut Tables,
    owner: FunctionId,
    name: &[u8],
    parameter_type: TypeId,
    flags: u32,
) {
    let interned = push_string(&mut tables.strings, name);
    push_parameter(tables, owner, interned, parameter_type, flags);
}

pub fn push_struct(
    tables: &mut Tables,
    name: StringId,
    type_id: TypeId,
    size: u32,
    alignment: u32,
    flags: u32,
) -> StructId {
    let field_start = tables.fields.owner.len() as u32;
    let structs = &mut tables.structs;
    structs.name.push(name);
    structs.type_id.push(type_id);
    structs.field_start.push(field_start);
    structs.field_count.push(0);
    structs.size.push(size);
    structs.alignment.push(alignment);
    structs.flags.push(flags);
    structs.deinit.push(FunctionId(crate::handles::NO_INDEX));
    structs.kind.push(crate::tables::ContainerKind::Struct);
    StructId(structs.name.len() as u32 - 1)
}

pub fn set_struct_kind(tables: &mut Tables, owner: StructId, kind: crate::tables::ContainerKind) {
    if let Some(slot) = tables.structs.kind.get_mut(owner.0 as usize) {
        *slot = kind;
    }
}

pub fn push_field(
    tables: &mut Tables,
    owner: StructId,
    name: StringId,
    field_type: TypeId,
    offset: u32,
) -> FieldId {
    let fields = &mut tables.fields;
    fields.owner.push(owner);
    fields.name.push(name);
    fields.field_type.push(field_type);
    fields.offset.push(offset);
    let id = FieldId(fields.owner.len() as u32 - 1);
    tables.structs.field_count[owner.0 as usize] += 1;
    id
}

pub fn set_struct_deinit(tables: &mut Tables, owner: StructId, function: FunctionId) {
    tables.structs.deinit[owner.0 as usize] = function;
}

pub fn push_function(tables: &mut Tables, name: StringId, owner: StructId) -> FunctionId {
    let parameter_start = tables.parameters.owner.len() as u32;
    let functions = &mut tables.functions;
    functions.name.push(name);
    functions.owner.push(owner);
    functions.parameter_start.push(parameter_start);
    functions.parameter_count.push(0);
    FunctionId(functions.name.len() as u32 - 1)
}

pub fn push_parameter(
    tables: &mut Tables,
    owner: FunctionId,
    name: StringId,
    parameter_type: TypeId,
    flags: u32,
) {
    let parameters = &mut tables.parameters;
    parameters.owner.push(owner);
    parameters.name.push(name);
    parameters.parameter_type.push(parameter_type);
    parameters.flags.push(flags);
    tables.functions.parameter_count[owner.0 as usize] += 1;
}

pub fn push_allocator_source(
    tables: &mut Tables,
    kind: AllocatorSourceKind,
    function: FunctionId,
    parameter_index: u32,
) -> AllocatorSourceId {
    let sources = &mut tables.allocator_sources;
    sources.kind.push(kind);
    sources.function.push(function);
    sources.parameter_index.push(parameter_index);
    AllocatorSourceId(sources.kind.len() as u32 - 1)
}

pub fn push_call(tables: &mut Tables, caller: FunctionId, callee: FunctionId) -> CallId {
    let calls = &mut tables.calls;
    calls.caller.push(caller);
    calls.callee.push(callee);
    CallId(calls.caller.len() as u32 - 1)
}

pub fn push_call_argument(
    tables: &mut Tables,
    call: CallId,
    parameter_index: u32,
    source: AllocatorSourceId,
) {
    let arguments = &mut tables.call_arguments;
    arguments.call.push(call);
    arguments.parameter_index.push(parameter_index);
    arguments.source.push(source);
}

pub fn push_memory_operation(
    tables: &mut Tables,
    function: FunctionId,
    kind: MemoryOperationKind,
    allocator: AllocatorSourceId,
    place: PlaceKind,
    place_field: FieldId,
) -> MemoryOperationId {
    let operations = &mut tables.memory_operations;
    operations.function.push(function);
    operations.kind.push(kind);
    operations.allocator.push(allocator);
    operations.place.push(place);
    operations.place_field.push(place_field);
    MemoryOperationId(operations.function.len() as u32 - 1)
}

pub fn push_field_assignment(
    tables: &mut Tables,
    field: FieldId,
    function: FunctionId,
    source: AssignmentSource,
    memory_operation: MemoryOperationId,
) {
    push_field_assignment_with(
        tables,
        field,
        function,
        source,
        memory_operation,
        ExpressionId(crate::handles::NO_INDEX),
    );
}

pub fn push_field_assignment_with(
    tables: &mut Tables,
    field: FieldId,
    function: FunctionId,
    source: AssignmentSource,
    memory_operation: MemoryOperationId,
    expression: ExpressionId,
) {
    let assignments = &mut tables.field_assignments;
    assignments.field.push(field);
    assignments.function.push(function);
    assignments.source.push(source);
    assignments.memory_operation.push(memory_operation);
    assignments.expression.push(expression);
}

pub fn push_expression(
    tables: &mut Tables,
    kind: ExpressionKind,
    text: StringId,
    parameter: u32,
    result: TypeId,
    field: FieldId,
    children: &[ExpressionId],
) -> ExpressionId {
    let child_start = tables.expressions.children.len() as u32;
    let expressions = &mut tables.expressions;
    expressions.children.extend_from_slice(children);
    expressions.kind.push(kind);
    expressions.text.push(text);
    expressions.parameter.push(parameter);
    expressions.result.push(result);
    expressions.field.push(field);
    expressions.child_start.push(child_start);
    expressions.child_count.push(children.len() as u32);
    ExpressionId(expressions.kind.len() as u32 - 1)
}
