use crate::handles::{
    AllocatorSourceId, CallId, FieldId, FunctionId, MemoryOperationId, ModuleId, StringId,
    StructId, TypeId,
};
use crate::tables::{
    AllocatorSourceKind, AssignmentSource, MemoryOperationKind, PlaceKind, Tables, TypeKind,
    empty_tables,
};

pub const MAGIC: [u8; 8] = *b"ZAGFACT\x00";
pub const VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeError {
    BadMagic,
    UnsupportedVersion(u32),
    Truncated { offset: usize },
    TrailingBytes { offset: usize },
    UnknownEnumValue { column: &'static str, value: u32 },
}

fn write_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_u32_column(out: &mut Vec<u8>, values: &[u32]) {
    write_u32(out, values.len() as u32);
    for value in values {
        write_u32(out, *value);
    }
}

fn write_byte_column(out: &mut Vec<u8>, values: &[u8]) {
    write_u32(out, values.len() as u32);
    out.extend_from_slice(values);
}

fn read_u32(bytes: &[u8], cursor: &mut usize) -> Result<u32, DecodeError> {
    let start = *cursor;
    let end = start + 4;
    if end > bytes.len() {
        return Err(DecodeError::Truncated { offset: start });
    }
    let mut raw = [0u8; 4];
    raw.copy_from_slice(&bytes[start..end]);
    *cursor = end;
    Ok(u32::from_le_bytes(raw))
}

fn read_u32_column(bytes: &[u8], cursor: &mut usize) -> Result<Vec<u32>, DecodeError> {
    let count = read_u32(bytes, cursor)? as usize;
    let start = *cursor;
    let end = start
        .checked_add(
            count
                .checked_mul(4)
                .ok_or(DecodeError::Truncated { offset: start })?,
        )
        .ok_or(DecodeError::Truncated { offset: start })?;
    if end > bytes.len() {
        return Err(DecodeError::Truncated { offset: start });
    }
    let mut values = Vec::with_capacity(count);
    for index in 0..count {
        let at = start + index * 4;
        let mut raw = [0u8; 4];
        raw.copy_from_slice(&bytes[at..at + 4]);
        values.push(u32::from_le_bytes(raw));
    }
    *cursor = end;
    Ok(values)
}

fn read_byte_column(bytes: &[u8], cursor: &mut usize) -> Result<Vec<u8>, DecodeError> {
    let count = read_u32(bytes, cursor)? as usize;
    let start = *cursor;
    let end = start
        .checked_add(count)
        .ok_or(DecodeError::Truncated { offset: start })?;
    if end > bytes.len() {
        return Err(DecodeError::Truncated { offset: start });
    }
    *cursor = end;
    Ok(bytes[start..end].to_vec())
}

fn type_kind_from_raw(value: u32) -> Result<TypeKind, DecodeError> {
    match value {
        0 => Ok(TypeKind::Void),
        1 => Ok(TypeKind::Integer),
        2 => Ok(TypeKind::Bool),
        3 => Ok(TypeKind::Slice),
        4 => Ok(TypeKind::Pointer),
        5 => Ok(TypeKind::Struct),
        6 => Ok(TypeKind::Opaque),
        7 => Ok(TypeKind::Optional),
        8 => Ok(TypeKind::Array),
        other => Err(DecodeError::UnknownEnumValue {
            column: "types.kind",
            value: other,
        }),
    }
}

fn container_kind_from_raw(value: u32) -> Result<crate::tables::ContainerKind, DecodeError> {
    use crate::tables::ContainerKind;
    match value {
        0 => Ok(ContainerKind::Struct),
        1 => Ok(ContainerKind::Enum),
        2 => Ok(ContainerKind::Union),
        3 => Ok(ContainerKind::ErrorSet),
        other => Err(DecodeError::UnknownEnumValue {
            column: "structs.kind",
            value: other,
        }),
    }
}

fn allocator_source_kind_from_raw(value: u32) -> Result<AllocatorSourceKind, DecodeError> {
    match value {
        0 => Ok(AllocatorSourceKind::Global),
        1 => Ok(AllocatorSourceKind::Arena),
        2 => Ok(AllocatorSourceKind::Parameter),
        3 => Ok(AllocatorSourceKind::Unknown),
        other => Err(DecodeError::UnknownEnumValue {
            column: "allocator_sources.kind",
            value: other,
        }),
    }
}

fn memory_operation_kind_from_raw(value: u32) -> Result<MemoryOperationKind, DecodeError> {
    match value {
        0 => Ok(MemoryOperationKind::Allocate),
        1 => Ok(MemoryOperationKind::Free),
        other => Err(DecodeError::UnknownEnumValue {
            column: "memory_operations.kind",
            value: other,
        }),
    }
}

fn place_kind_from_raw(value: u32) -> Result<PlaceKind, DecodeError> {
    match value {
        0 => Ok(PlaceKind::FieldOfParameter),
        1 => Ok(PlaceKind::Local),
        2 => Ok(PlaceKind::Unknown),
        other => Err(DecodeError::UnknownEnumValue {
            column: "memory_operations.place",
            value: other,
        }),
    }
}

fn expression_kind_from_raw(value: u32) -> Result<crate::tables::ExpressionKind, DecodeError> {
    use crate::tables::ExpressionKind;
    match value {
        0 => Ok(ExpressionKind::Literal),
        1 => Ok(ExpressionKind::Parameter),
        2 => Ok(ExpressionKind::Length),
        3 => Ok(ExpressionKind::Cast),
        4 => Ok(ExpressionKind::Allocation),
        5 => Ok(ExpressionKind::StructLiteral),
        6 => Ok(ExpressionKind::Unsupported),
        7 => Ok(ExpressionKind::Null),
        8 => Ok(ExpressionKind::Identifier),
        9 => Ok(ExpressionKind::Field),
        10 => Ok(ExpressionKind::Binary),
        11 => Ok(ExpressionKind::Unary),
        12 => Ok(ExpressionKind::Index),
        13 => Ok(ExpressionKind::Call),
        14 => Ok(ExpressionKind::Branch),
        15 => Ok(ExpressionKind::Block),
        16 => Ok(ExpressionKind::Return),
        17 => Ok(ExpressionKind::Let),
        18 => Ok(ExpressionKind::Assign),
        19 => Ok(ExpressionKind::Group),
        20 => Ok(ExpressionKind::Question),
        21 => Ok(ExpressionKind::Method),
        22 => Ok(ExpressionKind::While),
        23 => Ok(ExpressionKind::For),
        other => Err(DecodeError::UnknownEnumValue {
            column: "expressions.kind",
            value: other,
        }),
    }
}

fn assignment_source_from_raw(value: u32) -> Result<AssignmentSource, DecodeError> {
    match value {
        0 => Ok(AssignmentSource::Allocation),
        1 => Ok(AssignmentSource::Parameter),
        2 => Ok(AssignmentSource::StaticLiteral),
        3 => Ok(AssignmentSource::Unknown),
        other => Err(DecodeError::UnknownEnumValue {
            column: "field_assignments.source",
            value: other,
        }),
    }
}

fn encode_modules(out: &mut Vec<u8>, tables: &Tables) {
    let modules = &tables.modules;
    write_u32_column(out, &raw_from(&modules.name, |value| value.0));
    write_u32_column(out, &raw_from(&modules.path, |value| value.0));
    write_u32_column(out, &modules.unresolved_start);
    write_u32_column(out, &modules.unresolved_count);

    let unresolved = &tables.unresolved_imports;
    write_u32_column(out, &raw_from(&unresolved.owner, |value| value.0));
    write_u32_column(out, &raw_from(&unresolved.name, |value| value.0));
}

fn encode_types(out: &mut Vec<u8>, tables: &Tables) {
    let types = &tables.types;
    write_u32_column(out, &raw_from(&types.kind, |value| *value as u32));
    write_u32_column(out, &raw_from(&types.element, |value| value.0));
    write_u32_column(out, &types.count);
    write_u32_column(out, &raw_from(&types.module, |value| value.0));
    write_u32_column(out, &raw_from(&types.name, |value| value.0));
    write_u32_column(out, &types.size);
    write_u32_column(out, &types.alignment);
    write_u32_column(out, &types.bit_width);
    write_u32_column(out, &types.flags);
}

fn encode_structs(out: &mut Vec<u8>, tables: &Tables) {
    let structs = &tables.structs;
    write_u32_column(out, &raw_from(&structs.name, |value| value.0));
    write_u32_column(out, &raw_from(&structs.module, |value| value.0));
    write_u32_column(out, &raw_from(&structs.type_id, |value| value.0));
    write_u32_column(out, &structs.field_start);
    write_u32_column(out, &structs.field_count);
    write_u32_column(out, &structs.size);
    write_u32_column(out, &structs.alignment);
    write_u32_column(out, &structs.flags);
    write_u32_column(out, &raw_from(&structs.deinit, |value| value.0));
    write_u32_column(out, &raw_from(&structs.kind, |value| *value as u32));
}

fn encode_fields(out: &mut Vec<u8>, tables: &Tables) {
    let fields = &tables.fields;
    write_u32_column(out, &raw_from(&fields.owner, |value| value.0));
    write_u32_column(out, &raw_from(&fields.name, |value| value.0));
    write_u32_column(out, &raw_from(&fields.field_type, |value| value.0));
    write_u32_column(out, &fields.offset);
}

fn encode_functions(out: &mut Vec<u8>, tables: &Tables) {
    let functions = &tables.functions;
    write_u32_column(out, &raw_from(&functions.name, |value| value.0));
    write_u32_column(out, &raw_from(&functions.module, |value| value.0));
    write_u32_column(out, &raw_from(&functions.owner, |value| value.0));
    write_u32_column(out, &functions.parameter_start);
    write_u32_column(out, &functions.parameter_count);
    write_u32_column(out, &raw_from(&functions.returns, |value| value.0));
    write_u32_column(out, &raw_from(&functions.error_set, |value| value.0));
    write_u32_column(out, &functions.flags);
    write_u32_column(out, &functions.line);
    write_u32_column(out, &raw_from(&functions.body, |value| value.0));

    let parameters = &tables.parameters;
    write_u32_column(out, &raw_from(&parameters.owner, |value| value.0));
    write_u32_column(out, &raw_from(&parameters.name, |value| value.0));
    write_u32_column(out, &raw_from(&parameters.parameter_type, |value| value.0));
    write_u32_column(out, &parameters.flags);
}

fn encode_side_tables(out: &mut Vec<u8>, tables: &Tables) {
    let sources = &tables.allocator_sources;
    write_u32_column(out, &raw_from(&sources.kind, |value| *value as u32));
    write_u32_column(out, &raw_from(&sources.function, |value| value.0));
    write_u32_column(out, &sources.parameter_index);

    let calls = &tables.calls;
    write_u32_column(out, &raw_from(&calls.caller, |value| value.0));
    write_u32_column(out, &raw_from(&calls.callee, |value| value.0));

    let arguments = &tables.call_arguments;
    write_u32_column(out, &raw_from(&arguments.call, |value| value.0));
    write_u32_column(out, &arguments.parameter_index);
    write_u32_column(out, &raw_from(&arguments.source, |value| value.0));

    let operations = &tables.memory_operations;
    write_u32_column(out, &raw_from(&operations.function, |value| value.0));
    write_u32_column(out, &raw_from(&operations.kind, |value| *value as u32));
    write_u32_column(out, &raw_from(&operations.allocator, |value| value.0));
    write_u32_column(out, &raw_from(&operations.place, |value| *value as u32));
    write_u32_column(out, &raw_from(&operations.place_field, |value| value.0));

    let expressions = &tables.expressions;
    write_u32_column(out, &raw_from(&expressions.kind, |value| *value as u32));
    write_u32_column(out, &raw_from(&expressions.text, |value| value.0));
    write_u32_column(out, &expressions.parameter);
    write_u32_column(out, &raw_from(&expressions.result, |value| value.0));
    write_u32_column(out, &raw_from(&expressions.field, |value| value.0));
    write_u32_column(out, &expressions.line);
    write_u32_column(out, &expressions.child_start);
    write_u32_column(out, &expressions.child_count);
    write_u32_column(out, &raw_from(&expressions.children, |value| value.0));

    let assignments = &tables.field_assignments;
    write_u32_column(out, &raw_from(&assignments.field, |value| value.0));
    write_u32_column(out, &raw_from(&assignments.function, |value| value.0));
    write_u32_column(out, &raw_from(&assignments.source, |value| *value as u32));
    write_u32_column(
        out,
        &raw_from(&assignments.memory_operation, |value| value.0),
    );
    write_u32_column(out, &raw_from(&assignments.expression, |value| value.0));
    write_u32_column(out, &assignments.line);
}

fn raw_from<T>(values: &[T], project: impl Fn(&T) -> u32) -> Vec<u32> {
    values.iter().map(project).collect()
}

pub fn encode(tables: &Tables) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&MAGIC);
    write_u32(&mut out, VERSION);
    write_u32(&mut out, tables.target.0);
    write_byte_column(&mut out, &tables.strings.bytes);
    write_u32_column(&mut out, &tables.strings.offsets);
    encode_modules(&mut out, tables);
    encode_types(&mut out, tables);
    encode_structs(&mut out, tables);
    encode_fields(&mut out, tables);
    encode_functions(&mut out, tables);
    encode_side_tables(&mut out, tables);
    out
}

fn decode_modules(
    bytes: &[u8],
    cursor: &mut usize,
    tables: &mut Tables,
) -> Result<(), DecodeError> {
    let modules = &mut tables.modules;
    modules.name = read_u32_column(bytes, cursor)?
        .into_iter()
        .map(StringId)
        .collect();
    modules.path = read_u32_column(bytes, cursor)?
        .into_iter()
        .map(StringId)
        .collect();
    modules.unresolved_start = read_u32_column(bytes, cursor)?;
    modules.unresolved_count = read_u32_column(bytes, cursor)?;

    let unresolved = &mut tables.unresolved_imports;
    unresolved.owner = read_u32_column(bytes, cursor)?
        .into_iter()
        .map(ModuleId)
        .collect();
    unresolved.name = read_u32_column(bytes, cursor)?
        .into_iter()
        .map(StringId)
        .collect();
    Ok(())
}

fn decode_types(bytes: &[u8], cursor: &mut usize, tables: &mut Tables) -> Result<(), DecodeError> {
    let types = &mut tables.types;
    types.kind = map_raw(read_u32_column(bytes, cursor)?, type_kind_from_raw)?;
    types.element = read_u32_column(bytes, cursor)?
        .into_iter()
        .map(TypeId)
        .collect();
    types.count = read_u32_column(bytes, cursor)?;
    types.module = read_u32_column(bytes, cursor)?
        .into_iter()
        .map(ModuleId)
        .collect();
    types.name = read_u32_column(bytes, cursor)?
        .into_iter()
        .map(StringId)
        .collect();
    types.size = read_u32_column(bytes, cursor)?;
    types.alignment = read_u32_column(bytes, cursor)?;
    types.bit_width = read_u32_column(bytes, cursor)?;
    types.flags = read_u32_column(bytes, cursor)?;
    Ok(())
}

fn decode_structs(
    bytes: &[u8],
    cursor: &mut usize,
    tables: &mut Tables,
) -> Result<(), DecodeError> {
    let structs = &mut tables.structs;
    structs.name = read_u32_column(bytes, cursor)?
        .into_iter()
        .map(StringId)
        .collect();
    structs.module = read_u32_column(bytes, cursor)?
        .into_iter()
        .map(ModuleId)
        .collect();
    structs.type_id = read_u32_column(bytes, cursor)?
        .into_iter()
        .map(TypeId)
        .collect();
    structs.field_start = read_u32_column(bytes, cursor)?;
    structs.field_count = read_u32_column(bytes, cursor)?;
    structs.size = read_u32_column(bytes, cursor)?;
    structs.alignment = read_u32_column(bytes, cursor)?;
    structs.flags = read_u32_column(bytes, cursor)?;
    structs.deinit = read_u32_column(bytes, cursor)?
        .into_iter()
        .map(FunctionId)
        .collect();
    structs.kind = map_raw(read_u32_column(bytes, cursor)?, container_kind_from_raw)?;
    Ok(())
}

fn decode_fields(bytes: &[u8], cursor: &mut usize, tables: &mut Tables) -> Result<(), DecodeError> {
    let fields = &mut tables.fields;
    fields.owner = read_u32_column(bytes, cursor)?
        .into_iter()
        .map(StructId)
        .collect();
    fields.name = read_u32_column(bytes, cursor)?
        .into_iter()
        .map(StringId)
        .collect();
    fields.field_type = read_u32_column(bytes, cursor)?
        .into_iter()
        .map(TypeId)
        .collect();
    fields.offset = read_u32_column(bytes, cursor)?;
    Ok(())
}

fn decode_functions(
    bytes: &[u8],
    cursor: &mut usize,
    tables: &mut Tables,
) -> Result<(), DecodeError> {
    let functions = &mut tables.functions;
    functions.name = read_u32_column(bytes, cursor)?
        .into_iter()
        .map(StringId)
        .collect();
    functions.module = read_u32_column(bytes, cursor)?
        .into_iter()
        .map(ModuleId)
        .collect();
    functions.owner = read_u32_column(bytes, cursor)?
        .into_iter()
        .map(StructId)
        .collect();
    functions.parameter_start = read_u32_column(bytes, cursor)?;
    functions.parameter_count = read_u32_column(bytes, cursor)?;
    functions.returns = read_u32_column(bytes, cursor)?
        .into_iter()
        .map(TypeId)
        .collect();
    functions.error_set = read_u32_column(bytes, cursor)?
        .into_iter()
        .map(StructId)
        .collect();
    functions.flags = read_u32_column(bytes, cursor)?;
    functions.line = read_u32_column(bytes, cursor)?;
    functions.body = read_u32_column(bytes, cursor)?
        .into_iter()
        .map(crate::handles::ExpressionId)
        .collect();

    let parameters = &mut tables.parameters;
    parameters.owner = read_u32_column(bytes, cursor)?
        .into_iter()
        .map(FunctionId)
        .collect();
    parameters.name = read_u32_column(bytes, cursor)?
        .into_iter()
        .map(StringId)
        .collect();
    parameters.parameter_type = read_u32_column(bytes, cursor)?
        .into_iter()
        .map(TypeId)
        .collect();
    parameters.flags = read_u32_column(bytes, cursor)?;
    Ok(())
}

fn decode_allocator_and_calls(
    bytes: &[u8],
    cursor: &mut usize,
    tables: &mut Tables,
) -> Result<(), DecodeError> {
    let sources = &mut tables.allocator_sources;
    sources.kind = map_raw(
        read_u32_column(bytes, cursor)?,
        allocator_source_kind_from_raw,
    )?;
    sources.function = read_u32_column(bytes, cursor)?
        .into_iter()
        .map(FunctionId)
        .collect();
    sources.parameter_index = read_u32_column(bytes, cursor)?;

    let calls = &mut tables.calls;
    calls.caller = read_u32_column(bytes, cursor)?
        .into_iter()
        .map(FunctionId)
        .collect();
    calls.callee = read_u32_column(bytes, cursor)?
        .into_iter()
        .map(FunctionId)
        .collect();

    let arguments = &mut tables.call_arguments;
    arguments.call = read_u32_column(bytes, cursor)?
        .into_iter()
        .map(CallId)
        .collect();
    arguments.parameter_index = read_u32_column(bytes, cursor)?;
    arguments.source = read_u32_column(bytes, cursor)?
        .into_iter()
        .map(AllocatorSourceId)
        .collect();
    Ok(())
}

fn decode_memory(bytes: &[u8], cursor: &mut usize, tables: &mut Tables) -> Result<(), DecodeError> {
    let operations = &mut tables.memory_operations;
    operations.function = read_u32_column(bytes, cursor)?
        .into_iter()
        .map(FunctionId)
        .collect();
    operations.kind = map_raw(
        read_u32_column(bytes, cursor)?,
        memory_operation_kind_from_raw,
    )?;
    operations.allocator = read_u32_column(bytes, cursor)?
        .into_iter()
        .map(AllocatorSourceId)
        .collect();
    operations.place = map_raw(read_u32_column(bytes, cursor)?, place_kind_from_raw)?;
    operations.place_field = read_u32_column(bytes, cursor)?
        .into_iter()
        .map(FieldId)
        .collect();

    let expressions = &mut tables.expressions;
    expressions.kind = map_raw(read_u32_column(bytes, cursor)?, expression_kind_from_raw)?;
    expressions.text = read_u32_column(bytes, cursor)?
        .into_iter()
        .map(StringId)
        .collect();
    expressions.parameter = read_u32_column(bytes, cursor)?;
    expressions.result = read_u32_column(bytes, cursor)?
        .into_iter()
        .map(TypeId)
        .collect();
    expressions.field = read_u32_column(bytes, cursor)?
        .into_iter()
        .map(FieldId)
        .collect();
    expressions.line = read_u32_column(bytes, cursor)?;
    expressions.child_start = read_u32_column(bytes, cursor)?;
    expressions.child_count = read_u32_column(bytes, cursor)?;
    expressions.children = read_u32_column(bytes, cursor)?
        .into_iter()
        .map(crate::handles::ExpressionId)
        .collect();

    let assignments = &mut tables.field_assignments;
    assignments.field = read_u32_column(bytes, cursor)?
        .into_iter()
        .map(FieldId)
        .collect();
    assignments.function = read_u32_column(bytes, cursor)?
        .into_iter()
        .map(FunctionId)
        .collect();
    assignments.source = map_raw(read_u32_column(bytes, cursor)?, assignment_source_from_raw)?;
    assignments.memory_operation = read_u32_column(bytes, cursor)?
        .into_iter()
        .map(MemoryOperationId)
        .collect();
    assignments.expression = read_u32_column(bytes, cursor)?
        .into_iter()
        .map(crate::handles::ExpressionId)
        .collect();
    assignments.line = read_u32_column(bytes, cursor)?;
    Ok(())
}

fn map_raw<T>(
    values: Vec<u32>,
    convert: impl Fn(u32) -> Result<T, DecodeError>,
) -> Result<Vec<T>, DecodeError> {
    values.into_iter().map(convert).collect()
}

pub fn decode(bytes: &[u8]) -> Result<Tables, DecodeError> {
    if bytes.len() < MAGIC.len() || bytes[..MAGIC.len()] != MAGIC {
        return Err(DecodeError::BadMagic);
    }
    let mut cursor = MAGIC.len();
    let version = read_u32(bytes, &mut cursor)?;
    if version != VERSION {
        return Err(DecodeError::UnsupportedVersion(version));
    }
    let mut tables = empty_tables();
    tables.target = StringId(read_u32(bytes, &mut cursor)?);
    tables.strings.bytes = read_byte_column(bytes, &mut cursor)?;
    tables.strings.offsets = read_u32_column(bytes, &mut cursor)?;
    decode_modules(bytes, &mut cursor, &mut tables)?;
    decode_types(bytes, &mut cursor, &mut tables)?;
    decode_structs(bytes, &mut cursor, &mut tables)?;
    decode_fields(bytes, &mut cursor, &mut tables)?;
    decode_functions(bytes, &mut cursor, &mut tables)?;
    decode_allocator_and_calls(bytes, &mut cursor, &mut tables)?;
    decode_memory(bytes, &mut cursor, &mut tables)?;
    if cursor != bytes.len() {
        return Err(DecodeError::TrailingBytes { offset: cursor });
    }
    Ok(tables)
}
