use crate::handles::{
    AllocatorSourceId, CallId, ExpressionId, FieldId, FunctionId, MemoryOperationId, ModuleId,
    StringId, StructId, TypeId,
};

/// The Zig files the program is made of. A Zig file is a struct, so a module
/// here becomes a Rust module and a declaration keeps the namespace it was
/// written in. Row zero is the root, whose declarations sit at the top level
/// the way the root file's do in Zig.
pub const ROOT_MODULE: ModuleId = ModuleId(0);

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct Modules {
    pub name: Vec<StringId>,
    pub path: Vec<StringId>,
    /// An `@import` the crawl could not turn into a file, kept so the report
    /// can say the program was read with a hole in it.
    pub unresolved_start: Vec<u32>,
    pub unresolved_count: Vec<u32>,
}

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct UnresolvedImports {
    pub owner: Vec<ModuleId>,
    pub name: Vec<StringId>,
}

pub const TYPE_FLAG_SIGNED: u32 = 1 << 0;

pub const STRUCT_FLAG_EXTERN: u32 = 1 << 0;

/// What a container is. A struct's members are fields, an enum's are variants
/// with no payload, and a union's are variants that carry one.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ContainerKind {
    Struct = 0,
    Enum = 1,
    Union = 2,
    ErrorSet = 3,
}

pub const PARAMETER_FLAG_ALLOCATOR: u32 = 1 << 0;
/// The parameter is a `*T` the callee may write through. A `*const T` is not.
pub const PARAMETER_FLAG_MUTABLE: u32 = 1 << 1;

/// The Zig function returns an error union, so the port returns a `Result`.
pub const FUNCTION_FLAG_FALLIBLE: u32 = 1 << 0;

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
    Optional = 7,
    Array = 8,
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
    /// How many elements an array holds. Zero for everything else.
    pub count: Vec<u32>,
    /// Which module named this type, which decides how a reference from
    /// another module has to spell it. The root for anything unnamed.
    pub module: Vec<ModuleId>,
    pub name: Vec<StringId>,
    pub size: Vec<u32>,
    pub alignment: Vec<u32>,
    pub bit_width: Vec<u32>,
    pub flags: Vec<u32>,
}

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct Structs {
    pub name: Vec<StringId>,
    pub module: Vec<ModuleId>,
    pub type_id: Vec<TypeId>,
    pub field_start: Vec<u32>,
    pub field_count: Vec<u32>,
    pub size: Vec<u32>,
    pub alignment: Vec<u32>,
    pub flags: Vec<u32>,
    pub deinit: Vec<FunctionId>,
    pub kind: Vec<ContainerKind>,
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
    pub module: Vec<ModuleId>,
    pub owner: Vec<StructId>,
    pub parameter_start: Vec<u32>,
    pub parameter_count: Vec<u32>,
    /// What the function returns once any error union is stripped off. Absent
    /// where the frontend could not resolve it.
    pub returns: Vec<TypeId>,
    /// The error set the Zig named, where it named one. A `!T` infers its set
    /// from the body and names nothing, which leaves this absent.
    pub error_set: Vec<StructId>,
    pub flags: Vec<u32>,
    /// One-based line in the module that declares it, so the report can say
    /// where rather than only what. Zero where nothing recorded one.
    pub line: Vec<u32>,
    /// The block the function body is, absent where nothing read one. A
    /// statement is an expression here, because that is what it is in both
    /// languages and two vocabularies for one thing help nobody.
    pub body: Vec<ExpressionId>,
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

/// What a field is set to, in the shapes the port can write out. Anything
/// else is `Unsupported`, which is how a body that cannot be ported yet says
/// so rather than being guessed at.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ExpressionKind {
    Literal = 0,
    Parameter = 1,
    Length = 2,
    Cast = 3,
    Allocation = 4,
    StructLiteral = 5,
    Unsupported = 6,
    Null = 7,
    /// A name the body mentions, which is a parameter or a local.
    Identifier = 8,
    /// `x.field`, with the thing on the left as the only child.
    Field = 9,
    /// Two children and the operator in `text`.
    Binary = 10,
    /// One child and the operator in `text`.
    Unary = 11,
    Index = 12,
    /// The callee in `text` and the arguments as children.
    Call = 13,
    /// The condition, what to do, and optionally what to do instead. Rust and
    /// Zig both make this an expression, so nothing has to be reshaped.
    Branch = 14,
    Block = 15,
    Return = 16,
    /// The name in `text` and the value as the only child.
    Let = 17,
    Assign = 18,
    Group = 19,
    /// Zig's `try`, which is Rust's `?`.
    Question = 20,
}

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct Expressions {
    pub kind: Vec<ExpressionKind>,
    pub text: Vec<StringId>,
    pub parameter: Vec<u32>,
    pub result: Vec<TypeId>,
    pub child_start: Vec<u32>,
    pub child_count: Vec<u32>,
    pub children: Vec<ExpressionId>,
    pub field: Vec<FieldId>,
    /// One-based line of the Zig this came from. Zero for an expression the
    /// port synthesised rather than read.
    pub line: Vec<u32>,
}

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct FieldAssignments {
    pub field: Vec<FieldId>,
    pub function: Vec<FunctionId>,
    pub source: Vec<AssignmentSource>,
    pub memory_operation: Vec<MemoryOperationId>,
    pub expression: Vec<ExpressionId>,
    /// One-based line of the Zig that writes the field.
    pub line: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tables {
    pub target: StringId,
    pub strings: Strings,
    pub modules: Modules,
    pub unresolved_imports: UnresolvedImports,
    pub types: Types,
    pub structs: Structs,
    pub fields: Fields,
    pub functions: Functions,
    pub parameters: Parameters,
    pub allocator_sources: AllocatorSources,
    pub calls: Calls,
    pub call_arguments: CallArguments,
    pub memory_operations: MemoryOperations,
    pub expressions: Expressions,
    pub field_assignments: FieldAssignments,
}

/// Every declaration names a module, so the root exists from the start and a
/// program that is one file simply never gains a second.
pub fn empty_tables() -> Tables {
    Tables {
        target: StringId(crate::handles::NO_INDEX),
        strings: Strings::default(),
        modules: Modules {
            name: vec![StringId(crate::handles::NO_INDEX)],
            path: vec![StringId(crate::handles::NO_INDEX)],
            unresolved_start: vec![0],
            unresolved_count: vec![0],
        },
        unresolved_imports: UnresolvedImports::default(),
        types: Types::default(),
        structs: Structs::default(),
        fields: Fields::default(),
        functions: Functions::default(),
        parameters: Parameters::default(),
        allocator_sources: AllocatorSources::default(),
        calls: Calls::default(),
        call_arguments: CallArguments::default(),
        memory_operations: MemoryOperations::default(),
        expressions: Expressions::default(),
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

pub fn module_count(modules: &Modules) -> usize {
    modules.name.len()
}

/// Whether the program is more than one Zig file. A program that is one file
/// has one namespace, so its port needs no module tree to keep names apart.
pub fn has_submodules(modules: &Modules) -> bool {
    module_count(modules) > 1
}

/// Where a declaration sits in the Zig, as a module path and a line, when both
/// are known. A hand-built table records neither, so a caller gets nothing
/// rather than a line with no file to go with it.
pub fn source_location(tables: &Tables, module: ModuleId, line: u32) -> Option<(&[u8], u32)> {
    if line == 0 {
        return None;
    }
    let path = tables.modules.path.get(module.0 as usize).copied()?;
    if path.0 == crate::handles::NO_INDEX {
        return None;
    }
    let path = string_bytes(&tables.strings, path);
    (!path.is_empty()).then_some((path, line))
}

/// Where the function was written, which is also where anything inside it was.
pub fn function_location(tables: &Tables, function: FunctionId, line: u32) -> Option<(&[u8], u32)> {
    let module = tables
        .functions
        .module
        .get(function.0 as usize)
        .copied()
        .unwrap_or(ROOT_MODULE);
    source_location(tables, module, line)
}

pub fn expression_children(expressions: &Expressions, row: usize) -> std::ops::Range<usize> {
    let (Some(&start), Some(&count)) = (
        expressions.child_start.get(row),
        expressions.child_count.get(row),
    ) else {
        return 0..0;
    };
    let start = start as usize;
    start..start.saturating_add(count as usize)
}

pub fn module_unresolved(modules: &Modules, id: ModuleId) -> std::ops::Range<usize> {
    let index = id.0 as usize;
    let (Some(&start), Some(&count)) = (
        modules.unresolved_start.get(index),
        modules.unresolved_count.get(index),
    ) else {
        return 0..0;
    };
    let start = start as usize;
    start..start.saturating_add(count as usize)
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

/// Whether the type is one that owns memory somewhere else. An optional is
/// asked about whatever it wraps, so `?[]const u8` is a reference and `?u32`
/// is not.
pub fn is_reference_type(types: &Types, id: TypeId) -> bool {
    let mut current = id;
    for _ in 0..8 {
        let index = current.0 as usize;
        let Some(kind) = types.kind.get(index) else {
            return false;
        };
        match kind {
            TypeKind::Slice | TypeKind::Pointer => return true,
            TypeKind::Optional => {
                let Some(element) = types.element.get(index).copied() else {
                    return false;
                };
                current = element;
            }
            _ => return false,
        }
    }
    false
}
