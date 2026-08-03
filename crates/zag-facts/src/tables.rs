use crate::handles::{
    AllocatorSourceId, CallId, FieldId, FunctionId, MemoryOperationId, StringId, StructId, TypeId,
};

pub const TYPE_FLAG_SIGNED: u32 = 1 << 0;

pub const STRUCT_FLAG_EXTERN: u32 = 1 << 0;

pub const PARAMETER_FLAG_ALLOCATOR: u32 = 1 << 0;

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TypeKind {
    Void = 0,
    Integer = 1,
    Bool = 2,
    Slice = 3,
    Pointer = 4,
    Struct = 5,
    Opaque = 6,
}

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AllocatorSourceKind {
    Global = 0,
    Arena = 1,
    Parameter = 2,
    Unknown = 3,
}

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MemoryOperationKind {
    Allocate = 0,
    Free = 1,
}

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PlaceKind {
    FieldOfParameter = 0,
    Local = 1,
    Unknown = 2,
}

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AssignmentSource {
    Allocation = 0,
    Parameter = 1,
    StaticLiteral = 2,
    Unknown = 3,
}

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct Strings {
    pub bytes: Vec<u8>,
    pub offsets: Vec<u32>,
}

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct Types {
    pub kind: Vec<TypeKind>,
    pub element: Vec<TypeId>,
    pub name: Vec<StringId>,
    pub size: Vec<u32>,
    pub alignment: Vec<u32>,
    pub bit_width: Vec<u32>,
    pub flags: Vec<u32>,
}

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct Structs {
    pub name: Vec<StringId>,
    pub type_id: Vec<TypeId>,
    pub field_start: Vec<u32>,
    pub field_count: Vec<u32>,
    pub size: Vec<u32>,
    pub alignment: Vec<u32>,
    pub flags: Vec<u32>,
    pub deinit: Vec<FunctionId>,
}

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct Fields {
    pub owner: Vec<StructId>,
    pub name: Vec<StringId>,
    pub field_type: Vec<TypeId>,
    pub offset: Vec<u32>,
}

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct Functions {
    pub name: Vec<StringId>,
    pub owner: Vec<StructId>,
    pub parameter_start: Vec<u32>,
    pub parameter_count: Vec<u32>,
}

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct Parameters {
    pub owner: Vec<FunctionId>,
    pub name: Vec<StringId>,
    pub parameter_type: Vec<TypeId>,
    pub flags: Vec<u32>,
}

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct AllocatorSources {
    pub kind: Vec<AllocatorSourceKind>,
    pub function: Vec<FunctionId>,
    pub parameter_index: Vec<u32>,
}

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct Calls {
    pub caller: Vec<FunctionId>,
    pub callee: Vec<FunctionId>,
}

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct CallArguments {
    pub call: Vec<CallId>,
    pub parameter_index: Vec<u32>,
    pub source: Vec<AllocatorSourceId>,
}

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct MemoryOperations {
    pub function: Vec<FunctionId>,
    pub kind: Vec<MemoryOperationKind>,
    pub allocator: Vec<AllocatorSourceId>,
    pub place: Vec<PlaceKind>,
    pub place_field: Vec<FieldId>,
}

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct FieldAssignments {
    pub field: Vec<FieldId>,
    pub function: Vec<FunctionId>,
    pub source: Vec<AssignmentSource>,
    pub memory_operation: Vec<MemoryOperationId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tables {
    pub target: StringId,
    pub strings: Strings,
    pub types: Types,
    pub structs: Structs,
    pub fields: Fields,
    pub functions: Functions,
    pub parameters: Parameters,
    pub allocator_sources: AllocatorSources,
    pub calls: Calls,
    pub call_arguments: CallArguments,
    pub memory_operations: MemoryOperations,
    pub field_assignments: FieldAssignments,
}

pub fn empty_tables() -> Tables {
    Tables {
        target: StringId(crate::handles::NO_INDEX),
        strings: Strings::default(),
        types: Types::default(),
        structs: Structs::default(),
        fields: Fields::default(),
        functions: Functions::default(),
        parameters: Parameters::default(),
        allocator_sources: AllocatorSources::default(),
        calls: Calls::default(),
        call_arguments: CallArguments::default(),
        memory_operations: MemoryOperations::default(),
        field_assignments: FieldAssignments::default(),
    }
}

pub fn string_count(strings: &Strings) -> usize {
    strings.offsets.len().saturating_sub(1)
}

pub fn string_bytes(strings: &Strings, id: StringId) -> &[u8] {
    let index = id.0 as usize;
    if index + 1 >= strings.offsets.len() {
        return &[];
    }
    let start = strings.offsets[index] as usize;
    let end = strings.offsets[index + 1] as usize;
    if start > end || end > strings.bytes.len() {
        return &[];
    }
    &strings.bytes[start..end]
}

pub fn type_count(types: &Types) -> usize {
    types.kind.len()
}

pub fn struct_count(structs: &Structs) -> usize {
    structs.name.len()
}

pub fn field_count(fields: &Fields) -> usize {
    fields.owner.len()
}

pub fn function_count(functions: &Functions) -> usize {
    functions.name.len()
}

pub fn parameter_count(parameters: &Parameters) -> usize {
    parameters.owner.len()
}

pub fn call_count(calls: &Calls) -> usize {
    calls.caller.len()
}

pub fn memory_operation_count(operations: &MemoryOperations) -> usize {
    operations.function.len()
}

pub fn struct_fields(structs: &Structs, id: StructId) -> std::ops::Range<usize> {
    let index = id.0 as usize;
    let (Some(&start), Some(&count)) = (
        structs.field_start.get(index),
        structs.field_count.get(index),
    ) else {
        return 0..0;
    };
    let start = start as usize;
    start..start.saturating_add(count as usize)
}

pub fn function_parameters(functions: &Functions, id: FunctionId) -> std::ops::Range<usize> {
    let index = id.0 as usize;
    let (Some(&start), Some(&count)) = (
        functions.parameter_start.get(index),
        functions.parameter_count.get(index),
    ) else {
        return 0..0;
    };
    let start = start as usize;
    start..start.saturating_add(count as usize)
}

pub fn is_reference_type(types: &Types, id: TypeId) -> bool {
    let index = id.0 as usize;
    if index >= types.kind.len() {
        return false;
    }
    matches!(types.kind[index], TypeKind::Slice | TypeKind::Pointer)
}
