pub mod program;

use program::{Function, Layout, Program};
use zag_facts::build::{
    declare_field, declare_function, declare_parameter, intern, push_allocator_source, push_call,
    push_call_argument, push_expression, push_field_assignment_with, push_integer_type,
    push_memory_operation, push_opaque_type, push_pointer_type, push_slice_type, push_string,
    push_struct, push_struct_type, set_struct_deinit, set_struct_kind,
};
use zag_facts::handles::{
    ExpressionId, FieldId, FunctionId, MemoryOperationId, NO_INDEX, StringId, StructId, TypeId,
};
use zag_facts::tables::{
    AllocatorSourceKind, AssignmentSource, ContainerKind, ExpressionKind, MemoryOperationKind,
    PARAMETER_FLAG_ALLOCATOR, PlaceKind, STRUCT_FLAG_EXTERN, Tables, empty_tables,
};

const ALLOCATING: [&str; 6] = [
    "dupe",
    "dupeZ",
    "alloc",
    "allocSentinel",
    "create",
    "realloc",
];
const FREEING: [&str; 2] = ["free", "destroy"];
const GLOBAL_ALLOCATORS: [&str; 4] = [
    "page_allocator",
    "c_allocator",
    "smp_allocator",
    "raw_c_allocator",
];

fn last_segment(text: &str) -> &str {
    text.rsplit('.').next().unwrap_or(text)
}

/// The expression a method is called on. Whatever precedes it, `try` most of
/// the time, is not part of the receiver.
fn receiver(text: &str) -> &str {
    let Some(at) = text.rfind('.') else {
        return "";
    };
    text[..at].split_whitespace().last().unwrap_or("")
}

fn strip_error_union(text: &str) -> &str {
    match text.find('!') {
        Some(at) => text[at + 1..].trim(),
        None => text.trim(),
    }
}

pub struct Resolver {
    names: Vec<(String, TypeId)>,
    void: TypeId,
}

fn scalar_type(tables: &mut Tables, text: &str) -> Option<TypeId> {
    let signed = text.starts_with('i');
    if (signed || text.starts_with('u')) && text.len() > 1 {
        if let Ok(width) = text[1..].parse::<u32>() {
            return Some(push_integer_type(tables, width, signed));
        }
        if &text[1..] == "size" {
            return Some(push_integer_type(tables, 64, signed));
        }
    }
    None
}

fn resolve(tables: &mut Tables, resolver: &mut Resolver, text: &str) -> TypeId {
    let text = strip_error_union(text).trim();
    let text = text.strip_prefix("?").unwrap_or(text);
    if let Some(rest) = text.strip_prefix("[]") {
        let element = resolve(tables, resolver, rest.trim_start_matches("const ").trim());
        return push_slice_type(tables, element);
    }
    if let Some(rest) = text.strip_prefix('*') {
        let element = resolve(tables, resolver, rest.trim_start_matches("const ").trim());
        return push_pointer_type(tables, element);
    }
    if let Some(kind) = scalar_type(tables, text) {
        return kind;
    }
    match text {
        "bool" => {
            let name = push_string(&mut tables.strings, b"bool");
            return push_named_type(tables, name, zag_facts::tables::TypeKind::Bool);
        }
        "void" | "-" => return resolver.void,
        _ => {}
    }
    let simple = last_segment(text);
    if let Some((_, kind)) = resolver.names.iter().find(|(name, _)| name == simple) {
        return *kind;
    }
    let name = push_string(&mut tables.strings, simple.as_bytes());
    let kind = push_opaque_type(tables, name);
    resolver.names.push((simple.to_string(), kind));
    kind
}

fn push_named_type(
    tables: &mut Tables,
    name: zag_facts::StringId,
    kind: zag_facts::tables::TypeKind,
) -> TypeId {
    let types = &mut tables.types;
    types.kind.push(kind);
    types.element.push(TypeId(NO_INDEX));
    types.name.push(name);
    types.size.push(1);
    types.alignment.push(1);
    types.bit_width.push(1);
    types.flags.push(0);
    TypeId(types.kind.len() as u32 - 1)
}

fn layout_for<'a>(program: &'a Program, name: &str) -> Option<&'a Layout> {
    program
        .layouts
        .iter()
        .find(|(owner, _)| owner == name)
        .map(|(_, layout)| layout)
}

fn container_kind(keyword: &str) -> ContainerKind {
    match keyword {
        "enum" => ContainerKind::Enum,
        "union" => ContainerKind::Union,
        "error" => ContainerKind::ErrorSet,
        _ => ContainerKind::Struct,
    }
}

fn is_allocator(declared: &str) -> bool {
    declared.contains("Allocator")
}

/// Follows one level of local indirection, which is what makes a field set
/// from a local that a call filled read as the call that filled it.
fn through_locals<'a>(function: &'a Function, text: &'a str) -> &'a str {
    for (name, value) in &function.locals {
        if name == text {
            return value;
        }
    }
    text
}

fn allocating_call(text: &str) -> Option<&str> {
    let mut cursor = text;
    while let Some(open) = cursor.find('(') {
        let callee = cursor[..open].trim();
        if ALLOCATING.contains(&last_segment(callee)) {
            return Some(receiver(callee));
        }
        cursor = &cursor[open + 1..];
    }
    None
}

fn is_literal(text: &str) -> bool {
    text.starts_with('"')
        || text.starts_with("&.{")
        || text.starts_with(".{")
        || text.starts_with("0x")
        || text
            .chars()
            .next()
            .is_some_and(|byte| byte.is_ascii_digit())
}

struct Built {
    structs: Vec<(String, StructId)>,
    functions: Vec<(String, FunctionId)>,
}

fn declare_containers(
    tables: &mut Tables,
    resolver: &mut Resolver,
    program: &Program,
) -> Vec<(String, StructId)> {
    for container in &program.containers {
        let layout = layout_for(program, &container.name)
            .cloned()
            .unwrap_or_default();
        let name = push_string(&mut tables.strings, container.name.as_bytes());
        let kind = push_struct_type(tables, name, layout.size, layout.alignment.max(1));
        resolver.names.push((container.name.clone(), kind));
    }
    let mut declared = Vec::new();
    for container in &program.containers {
        let layout = layout_for(program, &container.name)
            .cloned()
            .unwrap_or_default();
        let flags = if layout.is_extern {
            STRUCT_FLAG_EXTERN
        } else {
            0
        };
        let name = push_string(&mut tables.strings, container.name.as_bytes());
        let kind = resolver
            .names
            .iter()
            .find(|(entry, _)| *entry == container.name)
            .map(|(_, kind)| *kind)
            .unwrap_or(TypeId(NO_INDEX));
        let owner = push_struct(
            tables,
            name,
            kind,
            layout.size,
            layout.alignment.max(1),
            flags,
        );
        set_struct_kind(tables, owner, container_kind(&container.kind));
        for member in &container.members {
            let member_type = resolve(tables, resolver, &member.declared);
            let offset = layout
                .offsets
                .iter()
                .find(|(field, _)| *field == member.name)
                .map(|(_, offset)| *offset)
                .unwrap_or(0);
            declare_field(tables, owner, member.name.as_bytes(), member_type, offset);
        }
        declared.push((container.name.clone(), owner));
    }
    declared
}

fn declare_functions(
    tables: &mut Tables,
    resolver: &mut Resolver,
    program: &Program,
    structs: &[(String, StructId)],
) -> Vec<(String, FunctionId)> {
    let mut declared = Vec::new();
    for function in &program.functions {
        let owner = structs
            .iter()
            .find(|(name, _)| *name == function.owner)
            .map(|(_, owner)| *owner)
            .unwrap_or(StructId(NO_INDEX));
        let handle = declare_function(tables, function.name.as_bytes(), owner);
        for parameter in &function.parameters {
            let kind = resolve(tables, resolver, &parameter.declared);
            let flags = if is_allocator(&parameter.declared) {
                PARAMETER_FLAG_ALLOCATOR
            } else {
                0
            };
            declare_parameter(tables, handle, parameter.name.as_bytes(), kind, flags);
        }
        if function.name == "deinit" && owner.0 != NO_INDEX {
            set_struct_deinit(tables, owner, handle);
        }
        declared.push((function.name.clone(), handle));
    }
    declared
}

pub fn build(program: &Program, target: &str) -> Tables {
    let mut tables = empty_tables();
    tables.target = intern(&mut tables.strings, target.as_bytes());
    let void_name = push_string(&mut tables.strings, b"void");
    let void = push_named_type(&mut tables, void_name, zag_facts::tables::TypeKind::Void);
    let mut resolver = Resolver {
        names: Vec::new(),
        void,
    };
    let structs = declare_containers(&mut tables, &mut resolver, program);
    let functions = declare_functions(&mut tables, &mut resolver, program, &structs);
    let built = Built { structs, functions };
    let sources = declare_allocator_sources(&mut tables, program, &built);
    declare_calls(&mut tables, program, &built, &sources);
    declare_memory(&mut tables, program, &built, &sources);
    tables
}

struct Sources {
    global: zag_facts::AllocatorSourceId,
    arena: zag_facts::AllocatorSourceId,
    unknown: zag_facts::AllocatorSourceId,
    parameters: Vec<(String, usize, zag_facts::AllocatorSourceId)>,
}

fn declare_allocator_sources(tables: &mut Tables, program: &Program, built: &Built) -> Sources {
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
    let unknown = push_allocator_source(
        tables,
        AllocatorSourceKind::Unknown,
        FunctionId(NO_INDEX),
        NO_INDEX,
    );
    let mut parameters = Vec::new();
    for function in &program.functions {
        let Some((_, handle)) = built
            .functions
            .iter()
            .find(|(name, _)| *name == function.name)
        else {
            continue;
        };
        for (index, parameter) in function.parameters.iter().enumerate() {
            if !is_allocator(&parameter.declared) {
                continue;
            }
            let source = push_allocator_source(
                tables,
                AllocatorSourceKind::Parameter,
                *handle,
                index as u32,
            );
            parameters.push((function.name.clone(), index, source));
        }
    }
    Sources {
        global,
        arena,
        unknown,
        parameters,
    }
}

fn classify_allocator(
    function: &Function,
    sources: &Sources,
    text: &str,
) -> zag_facts::AllocatorSourceId {
    if GLOBAL_ALLOCATORS.iter().any(|global| text.contains(global)) {
        return sources.global;
    }
    if let Some(stripped) = text.strip_suffix(".allocator()") {
        let origin = through_locals(function, stripped);
        if origin.contains("ArenaAllocator") {
            return sources.arena;
        }
    }
    let bare = text.trim();
    for (index, parameter) in function.parameters.iter().enumerate() {
        if parameter.name == bare {
            if let Some((_, _, source)) = sources
                .parameters
                .iter()
                .find(|(owner, at, _)| *owner == function.name && *at == index)
            {
                return *source;
            }
        }
    }
    let origin = through_locals(function, bare);
    if origin != bare {
        if GLOBAL_ALLOCATORS
            .iter()
            .any(|global| origin.contains(global))
        {
            return sources.global;
        }
        if origin.contains("ArenaAllocator") {
            return sources.arena;
        }
    }
    sources.unknown
}

fn declare_calls(tables: &mut Tables, program: &Program, built: &Built, sources: &Sources) {
    for function in &program.functions {
        let Some((_, caller)) = built
            .functions
            .iter()
            .find(|(name, _)| *name == function.name)
        else {
            continue;
        };
        for call in &function.calls {
            let target = last_segment(&call.callee);
            let Some((_, callee)) = built.functions.iter().find(|(name, _)| name == target) else {
                continue;
            };
            let handle = push_call(tables, *caller, *callee);
            let Some(target_program) = program.functions.iter().find(|entry| entry.name == target)
            else {
                continue;
            };
            for (index, parameter) in target_program.parameters.iter().enumerate() {
                if !is_allocator(&parameter.declared) {
                    continue;
                }
                // A method call carries its receiver as the first argument, so
                // the written arguments start one later than the parameters do.
                let shift = usize::from(
                    call.callee.contains('.')
                        && target_program.parameters.len() > call.arguments.len(),
                );
                let Some(text) = call.arguments.get(index.wrapping_sub(shift)) else {
                    continue;
                };
                let source = classify_allocator(function, sources, text);
                push_call_argument(tables, handle, index as u32, source);
            }
        }
    }
}

fn field_of(tables: &Tables, built: &Built, owner: &str, field: &str) -> Option<FieldId> {
    let (_, handle) = built.structs.iter().find(|(name, _)| name == owner)?;
    zag_facts::tables::struct_fields(&tables.structs, *handle)
        .find(|row| {
            zag_facts::tables::string_bytes(&tables.strings, tables.fields.name[*row])
                == field.as_bytes()
        })
        .map(|row| FieldId(row as u32))
}

/// A root literal fills the function's return type. A nested one fills the type
/// of the field it sits in, which is how a header inside a packet is attributed
/// to the header.
fn owner_of_initialiser(
    program: &Program,
    function: &Function,
    initialiser: &program::Initialiser,
) -> Option<String> {
    let Some(parent) = initialiser.parent else {
        let returned = last_segment(strip_error_union(&function.returns)).to_string();
        if declares(program, &returned, &initialiser.field) {
            return Some(returned);
        }
        return unique_holder(program, &initialiser.field);
    };
    let holder = function
        .initialisers
        .iter()
        .find(|entry| entry.node == parent)?;
    let holding = owner_of_initialiser(program, function, holder)?;
    let container = program
        .containers
        .iter()
        .find(|container| container.name == holding)?;
    let member = container
        .members
        .iter()
        .find(|member| member.name == holder.field)?;
    let named = last_segment(&member.declared).to_string();
    if declares(program, &named, &initialiser.field) {
        return Some(named);
    }
    unique_holder(program, &initialiser.field)
}

fn declares(program: &Program, container: &str, field: &str) -> bool {
    program.containers.iter().any(|entry| {
        entry.name == container && entry.members.iter().any(|member| member.name == field)
    })
}

/// A literal written somewhere other than the return value, into a slot in an
/// array for instance, names no type. While one container declares the field,
/// that container is the answer; while several do, there is no answer to give.
fn unique_holder(program: &Program, field: &str) -> Option<String> {
    let mut holders = program
        .containers
        .iter()
        .filter(|container| container.members.iter().any(|member| member.name == field))
        .map(|container| container.name.clone());
    let only = holders.next()?;
    holders.next().is_none().then_some(only)
}

/// Translates the expression a field is set to into the small set of shapes
/// the port can write. Anything outside that set is `Unsupported`, which stops
/// the function's body being written rather than producing a guess.
fn translate(
    tables: &mut Tables,
    program: &Program,
    built: &Built,
    function: &Function,
    holder: Option<&program::Initialiser>,
    result: TypeId,
    text: &str,
) -> ExpressionId {
    let trimmed = text.trim().trim_start_matches("try ").trim();
    let unsupported = |tables: &mut Tables| {
        push_expression(
            tables,
            ExpressionKind::Unsupported,
            StringId(NO_INDEX),
            NO_INDEX,
            result,
            FieldId(NO_INDEX),
            &[],
        )
    };

    if let Some(argument) = allocated_from(trimmed)
        && let Some(index) = parameter_index(function, argument)
    {
        return push_expression(
            tables,
            ExpressionKind::Allocation,
            StringId(NO_INDEX),
            index,
            result,
            FieldId(NO_INDEX),
            &[],
        );
    }
    if let Some(inner) = trimmed
        .strip_prefix("@intCast(")
        .and_then(|rest| rest.strip_suffix(')'))
    {
        let child = translate(tables, program, built, function, holder, result, inner);
        return push_expression(
            tables,
            ExpressionKind::Cast,
            StringId(NO_INDEX),
            NO_INDEX,
            result,
            FieldId(NO_INDEX),
            &[child],
        );
    }
    if let Some(holder) = trimmed.strip_suffix(".len")
        && let Some(index) = parameter_index(function, holder)
    {
        return push_expression(
            tables,
            ExpressionKind::Length,
            StringId(NO_INDEX),
            index,
            result,
            FieldId(NO_INDEX),
            &[],
        );
    }
    if let Some(index) = parameter_index(function, trimmed) {
        return push_expression(
            tables,
            ExpressionKind::Parameter,
            StringId(NO_INDEX),
            index,
            result,
            FieldId(NO_INDEX),
            &[],
        );
    }
    if trimmed.starts_with(".{") {
        let Some(holder) = holder else {
            return unsupported(tables);
        };
        let nested: Vec<&program::Initialiser> = function
            .initialisers
            .iter()
            .filter(|entry| entry.parent == Some(holder.node))
            .collect();
        if nested.is_empty() {
            return unsupported(tables);
        }
        let mut children = Vec::new();
        for entry in &nested {
            let Some(owner) = owner_of_initialiser(program, function, entry) else {
                return unsupported(tables);
            };
            let Some(field) = field_of(tables, built, &owner, &entry.field) else {
                return unsupported(tables);
            };
            let kind = tables
                .fields
                .field_type
                .get(field.0 as usize)
                .copied()
                .unwrap_or(TypeId(NO_INDEX));
            let value = translate(
                tables,
                program,
                built,
                function,
                Some(entry),
                kind,
                &entry.value,
            );
            children.push(push_expression(
                tables,
                ExpressionKind::StructLiteral,
                StringId(NO_INDEX),
                NO_INDEX,
                kind,
                field,
                &[value],
            ));
        }
        return push_expression(
            tables,
            ExpressionKind::StructLiteral,
            StringId(NO_INDEX),
            NO_INDEX,
            result,
            FieldId(NO_INDEX),
            &children,
        );
    }
    if is_literal(trimmed) && !trimmed.starts_with('"') {
        let text = push_string(&mut tables.strings, trimmed.as_bytes());
        return push_expression(
            tables,
            ExpressionKind::Literal,
            text,
            NO_INDEX,
            result,
            FieldId(NO_INDEX),
            &[],
        );
    }
    unsupported(tables)
}

fn parameter_index(function: &Function, name: &str) -> Option<u32> {
    function
        .parameters
        .iter()
        .position(|parameter| parameter.name == name.trim())
        .map(|index| index as u32)
}

/// The argument an allocating call copies from, which is what the port hands
/// to `Box::from`.
fn allocated_from(text: &str) -> Option<&str> {
    let open = text.find('(')?;
    if !ALLOCATING.contains(&last_segment(text[..open].trim())) {
        return None;
    }
    let inside = text[open + 1..].strip_suffix(')')?;
    inside.split(',').next_back().map(str::trim)
}

fn declare_memory(tables: &mut Tables, program: &Program, built: &Built, sources: &Sources) {
    for function in &program.functions {
        let Some((_, handle)) = built
            .functions
            .iter()
            .find(|(name, _)| *name == function.name)
        else {
            continue;
        };
        for initialiser in &function.initialisers {
            let Some(owner) = owner_of_initialiser(program, function, initialiser) else {
                continue;
            };
            let Some(field) = field_of(tables, built, &owner, &initialiser.field) else {
                continue;
            };
            let kind = tables
                .fields
                .field_type
                .get(field.0 as usize)
                .copied()
                .unwrap_or(TypeId(NO_INDEX));
            let expression = translate(
                tables,
                program,
                built,
                function,
                Some(initialiser),
                kind,
                &initialiser.value,
            );
            let resolved = through_locals(function, &initialiser.value);
            if let Some(allocator) = allocating_call(resolved) {
                let source = classify_allocator(function, sources, allocator);
                let operation = push_memory_operation(
                    tables,
                    *handle,
                    MemoryOperationKind::Allocate,
                    source,
                    PlaceKind::FieldOfParameter,
                    field,
                );
                push_field_assignment_with(
                    tables,
                    field,
                    *handle,
                    AssignmentSource::Allocation,
                    operation,
                    expression,
                );
                continue;
            }
            let source = if function
                .parameters
                .iter()
                .any(|parameter| parameter.name == initialiser.value)
            {
                AssignmentSource::Parameter
            } else if is_literal(&initialiser.value) {
                AssignmentSource::StaticLiteral
            } else {
                continue;
            };
            push_field_assignment_with(
                tables,
                field,
                *handle,
                source,
                MemoryOperationId(NO_INDEX),
                expression,
            );
        }

        for call in &function.calls {
            if !FREEING.contains(&last_segment(&call.callee)) {
                continue;
            }
            let Some(text) = call.arguments.first() else {
                continue;
            };
            let Some(field) = freed_field(tables, program, built, function, text) else {
                continue;
            };
            let source = classify_allocator(function, sources, receiver(&call.callee));
            push_memory_operation(
                tables,
                *handle,
                MemoryOperationKind::Free,
                source,
                PlaceKind::FieldOfParameter,
                field,
            );
        }
    }
}

fn freed_field(
    tables: &Tables,
    program: &Program,
    built: &Built,
    function: &Function,
    text: &str,
) -> Option<FieldId> {
    let (holder, field) = text.rsplit_once('.')?;
    let declared = function
        .parameters
        .iter()
        .find(|parameter| parameter.name == holder)
        .map(|parameter| parameter.declared.clone())
        .or_else(|| {
            function
                .locals
                .iter()
                .find(|(name, _)| name == holder)
                .map(|(_, value)| value.clone())
        })?;
    let owner = last_segment(declared.trim_start_matches(['*', '?']).trim());
    let owner = program
        .containers
        .iter()
        .find(|container| container.name == owner)
        .map(|container| container.name.clone())?;
    field_of(tables, built, &owner, field)
}
