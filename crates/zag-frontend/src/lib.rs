pub mod program;
pub mod project;

use program::{Function, Layout, Program};
use std::collections::BTreeMap;
use zag_facts::build::{
    declare_artifact, declare_field, declare_function, declare_module, declare_parameter, intern,
    name_root_module, push_allocator_source, push_array_type, push_body_expression, push_call,
    push_call_argument, push_expression, push_field_assignment_at, push_float_type,
    push_integer_type, push_memory_operation, push_opaque_type, push_optional_type,
    push_pointer_type, push_slice_type, push_string, push_struct, push_struct_type,
    push_unresolved_import, set_expression_line, set_function_body, set_function_line,
    set_function_module, set_function_signature, set_struct_deinit, set_struct_kind,
    set_struct_module, set_type_flags, set_type_module,
};
use zag_facts::handles::{
    ExpressionId, FieldId, FunctionId, MemoryOperationId, ModuleId, NO_INDEX, StringId, StructId,
    TypeId,
};
use zag_facts::tables::{
    AllocatorSourceKind, AssignmentSource, ContainerKind, ExpressionKind, MemoryOperationKind,
    PARAMETER_FLAG_ALLOCATOR, PARAMETER_FLAG_MUTABLE, PlaceKind, ROOT_MODULE, STRUCT_FLAG_EXTERN,
    TYPE_FLAG_OVER_ALIGNED, Tables, empty_tables,
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
/// Calls that change the length of an allocation already made. A field one of
/// these reaches is owned with a length that moves, which Rust spells as a
/// vector rather than a boxed slice.
const RESIZING: [&str; 3] = ["realloc", "reallocAdvanced", "resize"];
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

/// Names to handles, in the two ways a name gets looked up. Scanning a flat
/// list instead would be linear per name and so quadratic over a program,
/// which does not show up on an example and does not survive a real codebase.
struct Index<Handle> {
    scoped: BTreeMap<(ModuleId, String), Handle>,
    /// The first declaration of a name anywhere, which is the answer when the
    /// module a name was qualified with is not one the crawl reached.
    anywhere: BTreeMap<String, Handle>,
}

fn empty_index<Handle>() -> Index<Handle> {
    Index {
        scoped: BTreeMap::new(),
        anywhere: BTreeMap::new(),
    }
}

fn remember<Handle: Copy>(index: &mut Index<Handle>, module: ModuleId, name: &str, handle: Handle) {
    index.scoped.insert((module, name.to_string()), handle);
    index
        .anywhere
        .entry(name.to_string())
        .or_insert_with(|| handle);
}

pub struct Resolver {
    names: Index<TypeId>,
    void: TypeId,
}

/// Where a name is being read from. A bare name means this module, and a
/// qualified one means whichever module the qualifier was imported as, which
/// is the whole of cross-module name resolution.
struct Scope<'a> {
    module: ModuleId,
    imports: &'a [(String, ModuleId)],
}

/// Splits `store.Entry` into the module `store` names here and `Entry`. A name
/// with no qualifier, or one whose qualifier is not an import, belongs to the
/// module doing the reading.
fn qualified<'a>(scope: &Scope, text: &'a str) -> (Option<ModuleId>, &'a str) {
    let simple = last_segment(text);
    let Some(qualifier) = text.strip_suffix(simple).and_then(|rest| {
        let trimmed = rest.strip_suffix('.')?;
        Some(last_segment(trimmed))
    }) else {
        return (Some(scope.module), simple);
    };
    match scope
        .imports
        .iter()
        .find(|(alias, _)| alias == qualifier)
        .map(|(_, module)| *module)
    {
        Some(module) => (Some(module), simple),
        // A qualifier that is not an import is something like `std.mem`, which
        // names nothing the port declares, so any module may answer.
        None => (None, simple),
    }
}

/// Prefers the module the name was written in, then the module it was
/// qualified with, then anywhere. The fallback is what lets a type named
/// through an alias the crawl never saw still resolve to the one declaration
/// that carries it.
fn look_up<Handle: Copy>(index: &Index<Handle>, scope: &Scope, text: &str) -> Option<Handle> {
    let (module, simple) = qualified(scope, text);
    if let Some(module) = module
        && let Some(handle) = index.scoped.get(&(module, simple.to_string()))
    {
        return Some(*handle);
    }
    index.anywhere.get(simple).copied()
}

/// Strips what a slice or a pointer may carry in front of its element type.
/// `const` says nothing the port needs, and `align(N)` says something it cannot
/// write, so the caller is told the request was there and drops it rather than
/// resolving a type named after the qualifier.
fn strip_qualifiers(text: &str) -> (&str, bool) {
    let mut rest = text.trim();
    let mut aligned = false;
    loop {
        if let Some(next) = rest.strip_prefix("const ") {
            rest = next.trim_start();
            continue;
        }
        if let Some(next) = rest.strip_prefix("align(")
            && let Some(close) = next.find(')')
        {
            rest = next[close + 1..].trim_start();
            aligned = true;
            continue;
        }
        return (rest, aligned);
    }
}

fn scalar_type(tables: &mut Tables, text: &str) -> Option<TypeId> {
    if let Some(width) = text.strip_prefix('f')
        && let Ok(width) = width.parse::<u32>()
    {
        return Some(push_float_type(tables, width));
    }
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

fn resolve(tables: &mut Tables, resolver: &mut Resolver, scope: &Scope, text: &str) -> TypeId {
    let text = strip_error_union(text).trim();
    if let Some(rest) = text.strip_prefix('?') {
        let element = resolve(tables, resolver, scope, rest);
        return push_optional_type(tables, element);
    }
    if let Some(rest) = text.strip_prefix("[]") {
        let (inner, aligned) = strip_qualifiers(rest);
        let element = resolve(tables, resolver, scope, inner);
        let slice = push_slice_type(tables, element);
        if aligned {
            set_type_flags(tables, slice, TYPE_FLAG_OVER_ALIGNED);
        }
        return slice;
    }
    // `[N]T` is a fixed array, and the count is the only part of it Rust needs
    // that the element type does not already carry.
    if let Some(rest) = text.strip_prefix('[')
        && let Some((count, element)) = rest.split_once(']')
        && let Ok(count) = count.trim().parse::<u32>()
    {
        let element = resolve(
            tables,
            resolver,
            scope,
            element.trim_start_matches("const ").trim(),
        );
        let size = tables
            .types
            .size
            .get(element.0 as usize)
            .copied()
            .unwrap_or(0)
            .saturating_mul(count);
        return push_array_type(tables, element, count, size);
    }
    if let Some(rest) = text.strip_prefix('*') {
        let (inner, aligned) = strip_qualifiers(rest);
        let element = resolve(tables, resolver, scope, inner);
        let pointer = push_pointer_type(tables, element);
        if aligned {
            set_type_flags(tables, pointer, TYPE_FLAG_OVER_ALIGNED);
        }
        return pointer;
    }
    if let Some(kind) = scalar_type(tables, text) {
        return kind;
    }
    match text {
        "bool" => {
            let name = push_string(&mut tables.strings, b"bool");
            return push_named_type(
                tables,
                name,
                zag_facts::tables::TypeKind::Bool,
                scope.module,
            );
        }
        "void" | "-" => return resolver.void,
        _ => {}
    }
    if let Some(kind) = look_up(&resolver.names, scope, text) {
        return kind;
    }
    let simple = last_segment(text);
    let name = push_string(&mut tables.strings, simple.as_bytes());
    let kind = push_opaque_type(tables, name);
    set_type_module(tables, kind, scope.module);
    remember(&mut resolver.names, scope.module, simple, kind);
    kind
}

fn push_named_type(
    tables: &mut Tables,
    name: zag_facts::StringId,
    kind: zag_facts::tables::TypeKind,
    module: ModuleId,
) -> TypeId {
    let types = &mut tables.types;
    types.kind.push(kind);
    types.element.push(TypeId(NO_INDEX));
    types.count.push(0);
    types.module.push(module);
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

/// A `*T` the callee may write through. A `*const T` gives back a shared
/// reference instead, which is the difference between `&mut self` and `&self`.
fn is_mutable_pointer(declared: &str) -> bool {
    declared
        .trim()
        .strip_prefix('*')
        .is_some_and(|rest| !rest.trim_start().starts_with("const "))
}

/// The return type, split from the error set the Zig wrote in front of it. A
/// `!T` names no set, which leaves the function fallible with nothing the port
/// can spell, and `Set!T` names one the port can use.
fn declare_signature(
    tables: &mut Tables,
    resolver: &mut Resolver,
    scope: &Scope,
    built: &Built,
    function: FunctionId,
    declared: &str,
) {
    let declared = declared.trim();
    let (failure, returns) = match declared.split_once('!') {
        Some((failure, returns)) => (failure.trim(), returns.trim()),
        None => ("", declared),
    };
    let fallible = declared.contains('!');
    let error_set = if failure.is_empty() {
        StructId(NO_INDEX)
    } else {
        look_up(&built.structs, scope, failure).unwrap_or(StructId(NO_INDEX))
    };
    let returns = resolve(tables, resolver, scope, returns);
    set_function_signature(tables, function, returns, error_set, fallible);
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
    text == "null"
        || text.starts_with('"')
        || text.starts_with("&.{")
        || text.starts_with(".{")
        || text.starts_with("0x")
        || text
            .chars()
            .next()
            .is_some_and(|byte| byte.is_ascii_digit())
}

struct Built {
    structs: Index<StructId>,
    functions: Index<FunctionId>,
}

fn declare_containers(
    tables: &mut Tables,
    resolver: &mut Resolver,
    scope: &Scope,
    program: &Program,
) {
    for container in &program.containers {
        let layout = layout_for(program, &container.name)
            .cloned()
            .unwrap_or_default();
        let name = push_string(&mut tables.strings, container.name.as_bytes());
        let kind = push_struct_type(tables, name, layout.size, layout.alignment.max(1));
        set_type_module(tables, kind, scope.module);
        remember(&mut resolver.names, scope.module, &container.name, kind);
    }
}

fn declare_members(
    tables: &mut Tables,
    resolver: &mut Resolver,
    scope: &Scope,
    program: &Program,
    built: &mut Built,
) {
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
            .scoped
            .get(&(scope.module, container.name.clone()))
            .copied()
            .unwrap_or(TypeId(NO_INDEX));
        let owner = push_struct(
            tables,
            name,
            kind,
            layout.size,
            layout.alignment.max(1),
            flags,
        );
        set_struct_module(tables, owner, scope.module);
        set_struct_kind(tables, owner, container_kind(&container.kind));
        for member in &container.members {
            let member_type = resolve(tables, resolver, scope, &member.declared);
            let offset = layout
                .offsets
                .iter()
                .find(|(field, _)| *field == member.name)
                .map(|(_, offset)| *offset)
                .unwrap_or(0);
            declare_field(tables, owner, member.name.as_bytes(), member_type, offset);
        }
        remember(&mut built.structs, scope.module, &container.name, owner);
    }
}

fn declare_functions(
    tables: &mut Tables,
    resolver: &mut Resolver,
    scope: &Scope,
    program: &Program,
    built: &mut Built,
) {
    for function in &program.functions {
        let owner = look_up(&built.structs, scope, &function.owner).unwrap_or(StructId(NO_INDEX));
        let handle = declare_function(tables, function.name.as_bytes(), owner);
        set_function_module(tables, handle, scope.module);
        set_function_line(tables, handle, function.line);
        for parameter in &function.parameters {
            let kind = resolve(tables, resolver, scope, &parameter.declared);
            let mut flags = 0;
            if is_allocator(&parameter.declared) {
                flags |= PARAMETER_FLAG_ALLOCATOR;
            }
            if is_mutable_pointer(&parameter.declared) {
                flags |= PARAMETER_FLAG_MUTABLE;
            }
            declare_parameter(tables, handle, parameter.name.as_bytes(), kind, flags);
        }
        declare_signature(tables, resolver, scope, built, handle, &function.returns);
        if function.name == "deinit" && owner.0 != NO_INDEX {
            set_struct_deinit(tables, owner, handle);
        }
        remember(&mut built.functions, scope.module, &function.name, handle);
    }
}

pub fn build(program: &Program, target: &str) -> Tables {
    build_project(&project::single(program.clone()), target)
}

/// Every module the crawl found, merged into one fact database.
///
/// The order matters and is fixed by the caller: modules arrive with the root
/// first and the rest sorted by path, and handles are handed out in that order,
/// so the same directory always gives byte-identical tables.
///
/// Containers are declared for every module before any member is, because a
/// field in one module can name a type in another and the type has to exist
/// before the field that mentions it.
pub fn build_project(project: &project::Project, target: &str) -> Tables {
    let modules = &project.modules;
    let mut tables = empty_tables();
    tables.target = intern(&mut tables.strings, target.as_bytes());

    let mut handles: Vec<ModuleId> = Vec::new();
    // Indexed rather than scanned, because every module resolves every import
    // it wrote and a scan here is quadratic in the size of the program.
    let mut by_name: BTreeMap<&str, ModuleId> = BTreeMap::new();
    for (index, module) in modules.iter().enumerate() {
        let handle = if index == 0 {
            name_root_module(&mut tables, module.name.as_bytes(), module.path.as_bytes());
            ROOT_MODULE
        } else {
            declare_module(&mut tables, module.name.as_bytes(), module.path.as_bytes())
        };
        by_name.entry(module.name.as_str()).or_insert(handle);
        handles.push(handle);
    }
    for (module, handle) in modules.iter().zip(handles.iter()) {
        for text in &module.unresolved {
            push_unresolved_import(&mut tables, *handle, text.as_bytes());
        }
    }
    for artifact in &project.artifacts {
        let root = artifact
            .root
            .as_deref()
            .and_then(|name| by_name.get(name).copied())
            .unwrap_or(ModuleId(NO_INDEX));
        declare_artifact(&mut tables, artifact.name.as_bytes(), root, artifact.kind);
    }

    let imports: Vec<Vec<(String, ModuleId)>> = modules
        .iter()
        .map(|module| {
            module
                .imports
                .iter()
                .filter_map(|(alias, target)| {
                    by_name
                        .get(target.as_str())
                        .map(|handle| (alias.clone(), *handle))
                })
                .collect()
        })
        .collect();

    let void_name = push_string(&mut tables.strings, b"void");
    let void = push_named_type(
        &mut tables,
        void_name,
        zag_facts::tables::TypeKind::Void,
        ROOT_MODULE,
    );
    let mut resolver = Resolver {
        names: empty_index(),
        void,
    };
    let scopes: Vec<Scope> = handles
        .iter()
        .zip(imports.iter())
        .map(|(module, imports)| Scope {
            module: *module,
            imports,
        })
        .collect();

    let mut built = Built {
        structs: empty_index(),
        functions: empty_index(),
    };
    for (module, scope) in modules.iter().zip(scopes.iter()) {
        declare_containers(&mut tables, &mut resolver, scope, &module.program);
    }
    for (module, scope) in modules.iter().zip(scopes.iter()) {
        declare_members(
            &mut tables,
            &mut resolver,
            scope,
            &module.program,
            &mut built,
        );
    }
    for (module, scope) in modules.iter().zip(scopes.iter()) {
        declare_functions(
            &mut tables,
            &mut resolver,
            scope,
            &module.program,
            &mut built,
        );
    }

    let sources = declare_allocator_sources(&mut tables, modules, &built, &scopes);
    let catalogue = catalogue(modules);
    for (module, scope) in modules.iter().zip(scopes.iter()) {
        declare_calls(
            &mut tables,
            &catalogue,
            &module.program,
            &built,
            scope,
            &sources,
        );
    }
    for (module, scope) in modules.iter().zip(scopes.iter()) {
        declare_memory(
            &mut tables,
            &catalogue,
            &module.program,
            &built,
            scope,
            &sources,
        );
    }
    for (module, scope) in modules.iter().zip(scopes.iter()) {
        for function in &module.program.functions {
            let Some(handle) = look_up(&built.functions, scope, &function.name) else {
                continue;
            };
            declare_body(&mut tables, &built, scope, function, handle);
        }
    }
    tables
}

struct Sources {
    global: zag_facts::AllocatorSourceId,
    arena: zag_facts::AllocatorSourceId,
    unknown: zag_facts::AllocatorSourceId,
    /// Keyed by the function handle rather than its name, because two modules
    /// may each declare an `init` and they do not share an allocator.
    parameters: Vec<(FunctionId, usize, zag_facts::AllocatorSourceId)>,
}

fn declare_allocator_sources(
    tables: &mut Tables,
    modules: &[project::SourceModule],
    built: &Built,
    scopes: &[Scope],
) -> Sources {
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
    for (module, scope) in modules.iter().zip(scopes.iter()) {
        for function in &module.program.functions {
            let Some(handle) = look_up(&built.functions, scope, &function.name) else {
                continue;
            };
            for (index, parameter) in function.parameters.iter().enumerate() {
                if !is_allocator(&parameter.declared) {
                    continue;
                }
                let source = push_allocator_source(
                    tables,
                    AllocatorSourceKind::Parameter,
                    handle,
                    index as u32,
                );
                parameters.push((handle, index, source));
            }
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
    owner: FunctionId,
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
        if parameter.name == bare
            && let Some((_, _, source)) = sources
                .parameters
                .iter()
                .find(|(holder, at, _)| *holder == owner && *at == index)
        {
            return *source;
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

/// The declaration a call names. `store.open` is the `open` in the store
/// module, and a bare `open` is the one in the module doing the calling.
fn callee_of<'a>(catalogue: &Catalogue<'a>, scope: &Scope, callee: &str) -> Option<&'a Function> {
    let (module, simple) = qualified(scope, callee);
    if let Some(module) = module
        && let Some(found) = catalogue
            .functions
            .scoped
            .get(&(module, simple.to_string()))
    {
        return Some(found);
    }
    catalogue.functions.anywhere.get(simple).copied()
}

fn declare_calls(
    tables: &mut Tables,
    catalogue: &Catalogue,
    program: &Program,
    built: &Built,
    scope: &Scope,
    sources: &Sources,
) {
    for function in &program.functions {
        let Some(caller) = look_up(&built.functions, scope, &function.name) else {
            continue;
        };
        for call in &function.calls {
            let Some(callee) = look_up(&built.functions, scope, &call.callee) else {
                continue;
            };
            let handle = push_call(tables, caller, callee);
            let Some(target_program) = callee_of(catalogue, scope, &call.callee) else {
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
                let source = classify_allocator(function, caller, sources, text);
                push_call_argument(tables, handle, index as u32, source);
            }
        }
    }
}

fn field_of(
    tables: &Tables,
    built: &Built,
    scope: &Scope,
    owner: &str,
    field: &str,
) -> Option<FieldId> {
    let handle = look_up(&built.structs, scope, owner)?;
    zag_facts::tables::struct_fields(&tables.structs, handle)
        .find(|row| {
            zag_facts::tables::string_bytes(&tables.strings, tables.fields.name[*row])
                == field.as_bytes()
        })
        .map(|row| FieldId(row as u32))
}

/// What the whole program declares, indexed the ways the passes ask for it. A
/// struct literal in one file fills a type declared in another, so the search
/// that attributes it cannot stop at the file it was written in, and scanning
/// every container per literal would be quadratic.
struct Catalogue<'a> {
    containers: BTreeMap<String, &'a program::Container>,
    /// For each field name, how many containers declare it and the first that
    /// does. A field only one container declares needs no other evidence to be
    /// attributed, and a field several declare cannot be attributed at all.
    holders: BTreeMap<String, (usize, String)>,
    functions: Index<&'a Function>,
}

fn catalogue(modules: &'_ [project::SourceModule]) -> Catalogue<'_> {
    let mut containers = BTreeMap::new();
    let mut holders: BTreeMap<String, (usize, String)> = BTreeMap::new();
    let mut functions = empty_index();
    for (index, module) in modules.iter().enumerate() {
        let handle = ModuleId(index as u32);
        for container in &module.program.containers {
            containers
                .entry(container.name.clone())
                .or_insert(container);
            for member in &container.members {
                let entry = holders
                    .entry(member.name.clone())
                    .or_insert((0, container.name.clone()));
                entry.0 += 1;
            }
        }
        for function in &module.program.functions {
            remember(&mut functions, handle, &function.name, function);
        }
    }
    Catalogue {
        containers,
        holders,
        functions,
    }
}

/// A root literal fills the function's return type. A nested one fills the type
/// of the field it sits in, which is how a header inside a packet is attributed
/// to the header.
fn owner_of_initialiser(
    catalogue: &Catalogue,
    function: &Function,
    initialiser: &program::Initialiser,
) -> Option<String> {
    let Some(parent) = initialiser.parent else {
        let returned = last_segment(strip_error_union(&function.returns)).to_string();
        if declares(catalogue, &returned, &initialiser.field) {
            return Some(returned);
        }
        return unique_holder(catalogue, &initialiser.field);
    };
    let holder = function
        .initialisers
        .iter()
        .find(|entry| entry.node == parent)?;
    let holding = owner_of_initialiser(catalogue, function, holder)?;
    let container = catalogue.containers.get(&holding)?;
    let member = container
        .members
        .iter()
        .find(|member| member.name == holder.field)?;
    let named = last_segment(&member.declared).to_string();
    if declares(catalogue, &named, &initialiser.field) {
        return Some(named);
    }
    unique_holder(catalogue, &initialiser.field)
}

fn declares(catalogue: &Catalogue, container: &str, field: &str) -> bool {
    catalogue
        .containers
        .get(container)
        .is_some_and(|entry| entry.members.iter().any(|member| member.name == field))
}

/// A literal written somewhere other than the return value, into a slot in an
/// array for instance, names no type. While one container declares the field,
/// that container is the answer; while several do, there is no answer to give.
fn unique_holder(catalogue: &Catalogue, field: &str) -> Option<String> {
    let (holders, only) = catalogue.holders.get(field)?;
    (*holders == 1).then(|| only.clone())
}

/// Translates the expression a field is set to into the small set of shapes
/// the port can write. Anything outside that set is `Unsupported`, which stops
/// the function's body being written rather than producing a guess.
fn translate(
    tables: &mut Tables,
    catalogue: &Catalogue,
    built: &Built,
    scope: &Scope,
    function: &Function,
    holder: Option<&program::Initialiser>,
    result: TypeId,
    text: &str,
) -> ExpressionId {
    let trimmed = text.trim().trim_start_matches("try ").trim();
    // The Zig is kept rather than discarded, so the report can say what it
    // could not read rather than only that it could not.
    let unsupported = |tables: &mut Tables| {
        let text = push_string(&mut tables.strings, trimmed.as_bytes());
        push_expression(
            tables,
            ExpressionKind::Unsupported,
            text,
            NO_INDEX,
            result,
            FieldId(NO_INDEX),
            &[],
        )
    };

    if trimmed == "null" {
        return push_expression(
            tables,
            ExpressionKind::Null,
            StringId(NO_INDEX),
            NO_INDEX,
            result,
            FieldId(NO_INDEX),
            &[],
        );
    }
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
        let child = translate(
            tables, catalogue, built, scope, function, holder, result, inner,
        );
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
            let Some(owner) = owner_of_initialiser(catalogue, function, entry) else {
                return unsupported(tables);
            };
            let Some(field) = field_of(tables, built, scope, &owner, &entry.field) else {
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
                catalogue,
                built,
                scope,
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

/// Turns the flat node list the parser hands over into the expression tree the
/// tables hold. Children arrive before parents, so one pass in order is enough
/// and a node always finds what it refers to already built.
///
/// A shape with no expression kind of its own becomes `Unsupported` carrying
/// the Zig it stands for. That is what stops a body being half ported: one
/// unsupported node anywhere and the emitter writes none of it.
fn declare_body(
    tables: &mut Tables,
    built_names: &Built,
    scope: &Scope,
    function: &Function,
    handle: FunctionId,
) {
    if function.body.is_empty() {
        return;
    }
    let returns = returned_struct(tables, built_names, scope, handle);
    let fallible = tables
        .functions
        .flags
        .get(handle.0 as usize)
        .is_some_and(|flags| flags & zag_facts::tables::FUNCTION_FLAG_FALLIBLE != 0);
    let optional_return = tables
        .functions
        .returns
        .get(handle.0 as usize)
        .is_some_and(|kind| {
            tables.types.kind.get(kind.0 as usize) == Some(&zag_facts::tables::TypeKind::Optional)
        });
    // The parameters are the types the body starts with, and everything it
    // binds is derived from them.
    let mut locals: BTreeMap<String, TypeId> = BTreeMap::new();
    for row in zag_facts::tables::function_parameters(&tables.functions, handle) {
        if let (Some(name), Some(kind)) = (
            tables.parameters.name.get(row).copied(),
            tables.parameters.parameter_type.get(row).copied(),
        ) {
            let name =
                String::from_utf8_lossy(zag_facts::tables::string_bytes(&tables.strings, name))
                    .into_owned();
            locals.insert(name, kind);
        }
    }
    let mut built: Vec<(u32, ExpressionId)> = Vec::new();
    for node in &function.nodes {
        let owner = scrutinee_container(
            tables,
            function,
            handle,
            built_names,
            scope,
            &function.nodes,
            node.left,
        );
        let signature = Signature {
            owner,
            returns,
            optional_return,
            fallible,
        };
        let expression = translate_node(
            tables,
            built_names,
            scope,
            node,
            &built,
            &signature,
            &mut locals,
        );
        built.push((node.node, expression));
    }
    let statements: Vec<ExpressionId> = function
        .body
        .iter()
        .filter_map(|node| built.iter().find(|(at, _)| at == node).map(|(_, id)| *id))
        .collect();
    if statements.len() != function.body.len() {
        return;
    }
    let block = push_body_expression(
        tables,
        ExpressionKind::Block,
        StringId(NO_INDEX),
        function.line,
        &statements,
    );
    set_function_body(tables, handle, block);
}

fn built_child(built: &[(u32, ExpressionId)], node: Option<u32>) -> Option<ExpressionId> {
    let node = node?;
    built.iter().find(|(at, _)| *at == node).map(|(_, id)| *id)
}

/// A call the port can write, which is one whose callee the tables declare.
/// The allocator arguments go, because the ported signature does not take one.
///
/// `std.mem.eql(u8, a, b)` is `a == b` and is recognised by its spelling, the
/// same way an allocation is. A call to anything else the tables do not know
/// is left unsupported rather than guessed at.
fn translate_call(
    tables: &mut Tables,
    built: &Built,
    scope: &Scope,
    node: &program::Node,
    operands: &[ExpressionId],
) -> ExpressionId {
    if node.text == "std.mem.eql" && operands.len() == 3 {
        let equals = push_string(&mut tables.strings, b"==");
        return push_body_expression(
            tables,
            ExpressionKind::Binary,
            equals,
            node.line,
            &operands[1..],
        );
    }
    let unsupported = |tables: &mut Tables| {
        let text = push_string(&mut tables.strings, node.text.as_bytes());
        push_body_expression(tables, ExpressionKind::Unsupported, text, node.line, &[])
    };
    // A qualifier that is not an import is a receiver, and a method call needs
    // the receiver spelled, which the callee text alone does not give.
    let (module, _) = qualified(scope, &node.text);
    if module.is_none() {
        return unsupported(tables);
    }
    let Some(callee) = look_up(&built.functions, scope, &node.text) else {
        return unsupported(tables);
    };
    // Zig coerces a bare integer literal to whatever the parameter is, so
    // `splat(0)` passes a float. Rust does not, and the body carries no types,
    // but the callee resolved, so the parameter it lands on says which literal
    // was meant.
    let landings: Vec<(ExpressionId, TypeId)> =
        zag_facts::tables::function_parameters(&tables.functions, callee)
            .zip(operands.iter())
            .filter_map(|(row, operand)| {
                Some((*operand, *tables.parameters.parameter_type.get(row)?))
            })
            .collect();
    for (operand, declared) in landings {
        if tables.types.kind.get(declared.0 as usize) != Some(&zag_facts::tables::TypeKind::Float) {
            continue;
        }
        if tables.expressions.kind.get(operand.0 as usize) != Some(&ExpressionKind::Literal) {
            continue;
        }
        let Some(text) = tables.expressions.text.get(operand.0 as usize).copied() else {
            continue;
        };
        let mut widened = zag_facts::tables::string_bytes(&tables.strings, text).to_vec();
        if widened.is_empty() || !widened.iter().all(|byte| byte.is_ascii_digit()) {
            continue;
        }
        widened.extend_from_slice(b".0");
        let spelled = push_string(&mut tables.strings, &widened);
        if let Some(slot) = tables.expressions.text.get_mut(operand.0 as usize) {
            *slot = spelled;
        }
    }
    let kept: Vec<ExpressionId> = zag_facts::tables::function_parameters(&tables.functions, callee)
        .zip(operands.iter())
        .filter(|(row, _)| {
            tables
                .parameters
                .flags
                .get(*row)
                .is_some_and(|flags| flags & PARAMETER_FLAG_ALLOCATOR == 0)
        })
        .map(|(_, operand)| *operand)
        .collect();
    let expression = push_body_expression(
        tables,
        ExpressionKind::Call,
        StringId(NO_INDEX),
        node.line,
        &kept,
    );
    // The callee is a handle rather than a name, so the emitter spells it with
    // whatever path reaches it from where the call is written.
    if let Some(slot) = tables.expressions.parameter.get_mut(expression.0 as usize) {
        *slot = callee.0;
    }
    expression
}

/// The container a switch is over, when the thing being switched on is a
/// parameter whose declared type the tables carry. Without that, a bare `.red`
/// names nothing the port can spell, so the switch is left alone.
fn scrutinee_container(
    tables: &Tables,
    function: &Function,
    handle: FunctionId,
    built_names: &Built,
    scope: &Scope,
    nodes: &[program::Node],
    scrutinee: Option<u32>,
) -> Option<StructId> {
    let scrutinee = scrutinee?;
    let node = nodes.iter().find(|node| node.node == scrutinee)?;
    if node.kind != "identifier" {
        return None;
    }
    let declared = function
        .parameters
        .iter()
        .find(|parameter| parameter.name == node.text)?;
    let named = last_segment(declared.declared.trim_start_matches(['*', '?']).trim());
    let _ = handle;
    look_up(&built_names.structs, scope, named).filter(|owner| {
        matches!(
            tables.structs.kind.get(owner.0 as usize),
            Some(ContainerKind::Enum) | Some(ContainerKind::Union) | Some(ContainerKind::ErrorSet)
        )
    })
}

/// A Zig `.red` written as a match arm, spelled the way Rust spells it. A
/// variant that carries a payload binds the capture, and one that carries
/// nothing takes no parentheses.
fn arm_pattern(tables: &Tables, owner: StructId, pattern: &str, capture: &str) -> Option<Vec<u8>> {
    if pattern == "else" {
        return Some(b"_".to_vec());
    }
    let wanted = pattern.trim().trim_start_matches('.');
    let row = zag_facts::tables::struct_fields(&tables.structs, owner).find(|row| {
        zag_facts::tables::string_bytes(&tables.strings, tables.fields.name[*row])
            == wanted.as_bytes()
    })?;
    let mut out =
        zag_facts::tables::string_bytes(&tables.strings, tables.structs.name[owner.0 as usize])
            .to_vec();
    out.extend_from_slice(b"::");
    out.extend_from_slice(&pascal_case(wanted.as_bytes()));
    // Only a union's variants carry anything, which is the same rule the enum
    // lowering uses to decide whether to write a payload at all.
    let carries = tables.structs.kind.get(owner.0 as usize) == Some(&ContainerKind::Union)
        && tables.fields.field_type.get(row).is_some_and(|kind| {
            tables.types.kind.get(kind.0 as usize) != Some(&zag_facts::tables::TypeKind::Void)
        });
    if carries && capture != "-" {
        out.push(b'(');
        out.extend_from_slice(capture.as_bytes());
        out.push(b')');
    } else if carries {
        out.extend_from_slice(b"(_)");
    } else if capture != "-" {
        // A capture on a variant that carries nothing has nothing to bind.
        return None;
    }
    Some(out)
}

/// Zig spells a variant in snake case and Rust spells it in Pascal case.
pub(crate) fn pascal_case(name: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(name.len());
    let mut capitalise = true;
    for byte in name {
        if *byte == b'_' {
            capitalise = true;
            continue;
        }
        out.push(if capitalise {
            byte.to_ascii_uppercase()
        } else {
            *byte
        });
        capitalise = false;
    }
    out
}

/// A Zig builtin that has a Rust method meaning the same thing, with no type
/// the port would have to invent. `@truncate` and `@intFromEnum` are not here
/// because both need a target type that the syntax does not carry.
fn builtin_method(name: &str) -> Option<(&'static str, usize)> {
    match name {
        "@min" => Some(("min", 2)),
        "@max" => Some(("max", 2)),
        "@abs" => Some(("abs", 1)),
        "@sqrt" => Some(("sqrt", 1)),
        "@floor" => Some(("floor", 1)),
        "@ceil" => Some(("ceil", 1)),
        "@round" => Some(("round", 1)),
        "@trunc" => Some(("trunc", 1)),
        "@exp" => Some(("exp", 1)),
        "@log" => Some(("ln", 1)),
        "@log2" => Some(("log2", 1)),
        "@log10" => Some(("log10", 1)),
        "@sin" => Some(("sin", 1)),
        "@cos" => Some(("cos", 1)),
        "@tan" => Some(("tan", 1)),
        _ => None,
    }
}

/// The struct a `return .{ ... }` is building, which is the one the function
/// says it gives back. A body expression carries no type, but this one does not
/// need to be inferred: the signature already wrote it down.
fn returned_struct(
    tables: &Tables,
    built: &Built,
    scope: &Scope,
    handle: FunctionId,
) -> Option<StructId> {
    let mut returns = tables.functions.returns.get(handle.0 as usize).copied()?;
    // `?T` is written `return .{ ... }` the same way `T` is, and the struct
    // being built is the one inside the option. The error union is already off
    // by the time the return type resolved.
    for _ in 0..8 {
        if tables.types.kind.get(returns.0 as usize) != Some(&zag_facts::tables::TypeKind::Optional)
        {
            break;
        }
        let Some(element) = tables.types.element.get(returns.0 as usize).copied() else {
            break;
        };
        if element.0 == NO_INDEX || element == returns {
            break;
        }
        returns = element;
    }
    let name = tables.types.name.get(returns.0 as usize).copied()?;
    if name.0 == NO_INDEX {
        return None;
    }
    let text = String::from_utf8_lossy(zag_facts::tables::string_bytes(&tables.strings, name))
        .into_owned();
    look_up(&built.structs, scope, &text)
}

/// Whether the expression is itself something that can fail, which is what
/// tells a value on the way out from an error union already on its way out.
fn returns_a_failure(tables: &Tables, built: &Built, expression: ExpressionId) -> bool {
    let index = expression.0 as usize;
    let called = match tables.expressions.kind.get(index).copied() {
        Some(ExpressionKind::Call) => tables
            .expressions
            .parameter
            .get(index)
            .copied()
            .filter(|callee| *callee != NO_INDEX)
            .map(FunctionId),
        Some(ExpressionKind::Method) => {
            let name = String::from_utf8_lossy(zag_facts::tables::string_bytes(
                &tables.strings,
                tables
                    .expressions
                    .text
                    .get(index)
                    .copied()
                    .unwrap_or(StringId(NO_INDEX)),
            ))
            .into_owned();
            method_handle(tables, built, expression, &name)
        }
        _ => None,
    };
    called.is_some_and(|handle| {
        tables
            .functions
            .flags
            .get(handle.0 as usize)
            .is_some_and(|flags| flags & zag_facts::tables::FUNCTION_FLAG_FALLIBLE != 0)
    })
}

/// The function a method call reaches, through the receiver's own type.
fn method_handle(
    tables: &Tables,
    _built: &Built,
    call: ExpressionId,
    name: &str,
) -> Option<FunctionId> {
    let receiver = zag_facts::tables::expression_children(&tables.expressions, call.0 as usize)
        .next()
        .and_then(|slot| tables.expressions.children.get(slot).copied())?;
    let kind = tables
        .expressions
        .result
        .get(receiver.0 as usize)
        .copied()
        .filter(|kind| kind.0 != NO_INDEX)?;
    let owner = struct_of(tables, kind, 0)?;
    (0..zag_facts::tables::function_count(&tables.functions))
        .find(|row| {
            tables.functions.owner.get(*row).copied() == Some(owner)
                && zag_facts::tables::string_bytes(&tables.strings, tables.functions.name[*row])
                    == name.as_bytes()
        })
        .map(|row| FunctionId(row as u32))
}

/// Whether the expression names a variant of an error set, which is what
/// decides between `Ok` and `Err` on the way out of a fallible function.
/// The struct a type reaches, through whatever wrappers it was written behind.
/// Rewrites a bare integer literal as a float where the expression beside it is
/// one. Zig reads the literal as whatever fits; Rust needs it written down.
/// What the enclosing function wrote down, which is where a body gets the types
/// its own expressions do not carry.
struct Signature {
    owner: Option<StructId>,
    returns: Option<StructId>,
    optional_return: bool,
    fallible: bool,
}

fn widen_beside(tables: &mut Tables, literal: ExpressionId, beside: ExpressionId) {
    if tables.expressions.kind.get(literal.0 as usize) != Some(&ExpressionKind::Literal) {
        return;
    }
    let float = tables
        .expressions
        .result
        .get(beside.0 as usize)
        .copied()
        .is_some_and(|kind| {
            tables.types.kind.get(kind.0 as usize) == Some(&zag_facts::tables::TypeKind::Float)
        });
    if !float {
        return;
    }
    let Some(text) = tables.expressions.text.get(literal.0 as usize).copied() else {
        return;
    };
    let mut widened = zag_facts::tables::string_bytes(&tables.strings, text).to_vec();
    if widened.is_empty() || !widened.iter().all(|byte| byte.is_ascii_digit()) {
        return;
    }
    widened.extend_from_slice(b".0");
    let spelled = push_string(&mut tables.strings, &widened);
    if let Some(slot) = tables.expressions.text.get_mut(literal.0 as usize) {
        *slot = spelled;
    }
}

fn struct_of(tables: &Tables, kind: TypeId, depth: u32) -> Option<StructId> {
    if depth >= 8 {
        return None;
    }
    if tables.types.kind.get(kind.0 as usize) == Some(&zag_facts::tables::TypeKind::Struct) {
        return (0..zag_facts::tables::struct_count(&tables.structs))
            .find(|row| tables.structs.type_id.get(*row).copied() == Some(kind))
            .map(|row| StructId(row as u32));
    }
    let element = tables.types.element.get(kind.0 as usize).copied()?;
    if element.0 == NO_INDEX || element == kind {
        return None;
    }
    struct_of(tables, element, depth + 1)
}

/// What `receiver.name(...)` gives back, found through the receiver's own type
/// rather than through the spelling of the call.
fn method_result(
    tables: &Tables,
    built: &Built,
    receiver: ExpressionId,
    name: &str,
) -> Option<TypeId> {
    let kind = tables
        .expressions
        .result
        .get(receiver.0 as usize)
        .copied()?;
    if kind.0 == NO_INDEX {
        return None;
    }
    let owner = struct_of(tables, kind, 0)?;
    let handle = built
        .functions
        .anywhere
        .iter()
        .find(|(key, handle)| {
            key.as_str() == name
                && tables.functions.owner.get(handle.0 as usize).copied() == Some(owner)
        })
        .map(|(_, handle)| *handle)?;
    tables
        .functions
        .returns
        .get(handle.0 as usize)
        .copied()
        .filter(|kind| kind.0 != NO_INDEX)
}

fn names_an_error(tables: &Tables, built: &Built, scope: &Scope, expression: ExpressionId) -> bool {
    if tables.expressions.kind.get(expression.0 as usize) != Some(&ExpressionKind::Identifier) {
        return false;
    }
    let Some(text) = tables.expressions.text.get(expression.0 as usize).copied() else {
        return false;
    };
    let text = String::from_utf8_lossy(zag_facts::tables::string_bytes(&tables.strings, text))
        .into_owned();
    let Some((base, _)) = text.split_once("::") else {
        return false;
    };
    look_up(&built.structs, scope, base).is_some_and(|owner| {
        tables.structs.kind.get(owner.0 as usize) == Some(&ContainerKind::ErrorSet)
    })
}

fn translate_node(
    tables: &mut Tables,
    built_names: &Built,
    scope: &Scope,
    node: &program::Node,
    built: &[(u32, ExpressionId)],
    signature: &Signature,
    locals: &mut BTreeMap<String, TypeId>,
) -> ExpressionId {
    let Signature {
        owner,
        returns,
        optional_return,
        fallible,
    } = *signature;
    let left = built_child(built, node.left);
    let right = built_child(built, node.right);
    let otherwise = built_child(built, node.otherwise);
    let operands: Vec<ExpressionId> = node
        .operands
        .iter()
        .filter_map(|operand| built_child(built, Some(*operand)))
        .collect();
    let complete = operands.len() == node.operands.len();

    let spelled = |tables: &mut Tables| push_string(&mut tables.strings, node.text.as_bytes());
    let unsupported = |tables: &mut Tables| {
        let text = push_string(&mut tables.strings, node.text.as_bytes());
        push_body_expression(tables, ExpressionKind::Unsupported, text, node.line, &[])
    };

    match node.kind.as_str() {
        "identifier" => {
            let text = spelled(tables);
            let named =
                push_body_expression(tables, ExpressionKind::Identifier, text, node.line, &[]);
            // A name the body already bound carries its type forward, which is
            // what lets a literal beside it be spelled as the right kind of
            // number.
            if let Some(kind) = locals.get(&node.text).copied()
                && let Some(slot) = tables.expressions.result.get_mut(named.0 as usize)
            {
                *slot = kind;
            }
            named
        }
        "literal" => {
            let text = spelled(tables);
            push_body_expression(tables, ExpressionKind::Literal, text, node.line, &[])
        }
        // Each field becomes a literal of its own carrying the field it fills,
        // and the whole thing carries the struct being built. The same shape a
        // constructor is written from, so the emitter needs no second way to
        // print one.
        "structliteral" => {
            let Some(container) = returns else {
                return unsupported(tables);
            };
            let kind = tables
                .structs
                .type_id
                .get(container.0 as usize)
                .copied()
                .unwrap_or(TypeId(NO_INDEX));
            // Every field resolves before any row is written, so a literal the
            // port cannot spell leaves nothing behind it.
            let mut members: Vec<(FieldId, ExpressionId)> = Vec::new();
            for (name, value) in &node.fields {
                let Some(value) = built_child(built, Some(*value)) else {
                    return unsupported(tables);
                };
                let Some(field) = zag_facts::tables::struct_fields(&tables.structs, container)
                    .find(|row| {
                        zag_facts::tables::string_bytes(&tables.strings, tables.fields.name[*row])
                            == name.as_bytes()
                    })
                    .map(|row| FieldId(row as u32))
                else {
                    return unsupported(tables);
                };
                members.push((field, value));
            }
            if members.is_empty() {
                return unsupported(tables);
            }
            let mut children = Vec::with_capacity(members.len());
            for (field, value) in members {
                let entry = push_expression(
                    tables,
                    ExpressionKind::StructLiteral,
                    StringId(NO_INDEX),
                    NO_INDEX,
                    TypeId(NO_INDEX),
                    field,
                    &[value],
                );
                set_expression_line(tables, entry, node.line);
                children.push(entry);
            }
            let expression = push_expression(
                tables,
                ExpressionKind::StructLiteral,
                StringId(NO_INDEX),
                NO_INDEX,
                kind,
                FieldId(NO_INDEX),
                &children,
            );
            set_expression_line(tables, expression, node.line);
            expression
        }
        // `Error.ZeroSized` names a variant rather than reading a field, which
        // Rust spells with a path. The base resolving to a container the
        // program declares is what tells the two apart.
        "field" => match left {
            Some(left) => {
                let base = tables
                    .expressions
                    .text
                    .get(left.0 as usize)
                    .copied()
                    .map(|text| {
                        String::from_utf8_lossy(zag_facts::tables::string_bytes(
                            &tables.strings,
                            text,
                        ))
                        .into_owned()
                    })
                    .unwrap_or_default();
                if tables.expressions.kind.get(left.0 as usize) == Some(&ExpressionKind::Identifier)
                    && !base.is_empty()
                    && look_up(&built_names.structs, scope, &base).is_some()
                {
                    let variant = pascal_case(node.text.as_bytes());
                    let path = format!("{base}::{}", String::from_utf8_lossy(&variant));
                    let named = push_string(&mut tables.strings, path.as_bytes());
                    return push_body_expression(
                        tables,
                        ExpressionKind::Identifier,
                        named,
                        node.line,
                        &[],
                    );
                }
                let text = spelled(tables);
                let read =
                    push_body_expression(tables, ExpressionKind::Field, text, node.line, &[left]);
                if let Some(kind) = tables.expressions.result.get(left.0 as usize).copied()
                    && kind.0 != NO_INDEX
                    && let Some(owner) = struct_of(tables, kind, 0)
                    && let Some(row) = zag_facts::tables::struct_fields(&tables.structs, owner)
                        .find(|row| {
                            zag_facts::tables::string_bytes(
                                &tables.strings,
                                tables.fields.name[*row],
                            ) == node.text.as_bytes()
                        })
                    && let Some(declared) = tables.fields.field_type.get(row).copied()
                    && let Some(slot) = tables.expressions.result.get_mut(read.0 as usize)
                {
                    *slot = declared;
                }
                read
            }
            None => unsupported(tables),
        },
        "binary" => match (left, right) {
            (Some(left), Some(right)) => {
                // Zig coerces a bare literal to whatever it is written beside,
                // so `size == 0` compares floats. Rust reads the literal as an
                // integer unless the side that carries a type says otherwise.
                widen_beside(tables, left, right);
                widen_beside(tables, right, left);
                let text = spelled(tables);
                let combined = push_body_expression(
                    tables,
                    ExpressionKind::Binary,
                    text,
                    node.line,
                    &[left, right],
                );
                let carried = [left, right].into_iter().find_map(|side| {
                    tables
                        .expressions
                        .result
                        .get(side.0 as usize)
                        .copied()
                        .filter(|kind| kind.0 != NO_INDEX)
                });
                if let Some(kind) = carried
                    && let Some(slot) = tables.expressions.result.get_mut(combined.0 as usize)
                {
                    *slot = kind;
                }
                combined
            }
            _ => unsupported(tables),
        },
        "unary" => match left {
            Some(left) => {
                let text = spelled(tables);
                push_body_expression(tables, ExpressionKind::Unary, text, node.line, &[left])
            }
            None => unsupported(tables),
        },
        "index" => match (left, right) {
            (Some(left), Some(right)) => push_body_expression(
                tables,
                ExpressionKind::Index,
                StringId(NO_INDEX),
                node.line,
                &[left, right],
            ),
            _ => unsupported(tables),
        },
        "group" => match left {
            Some(left) => push_body_expression(
                tables,
                ExpressionKind::Group,
                StringId(NO_INDEX),
                node.line,
                &[left],
            ),
            None => unsupported(tables),
        },
        "try" => match left {
            Some(left) => push_body_expression(
                tables,
                ExpressionKind::Question,
                StringId(NO_INDEX),
                node.line,
                &[left],
            ),
            None => unsupported(tables),
        },
        "method" if complete => match left {
            Some(receiver) => {
                let text = spelled(tables);
                let mut children = vec![receiver];
                children.extend(operands);
                let called = push_body_expression(
                    tables,
                    ExpressionKind::Method,
                    text,
                    node.line,
                    &children,
                );
                // The receiver's type says which struct the method belongs to,
                // so what it gives back is known without resolving the call by
                // name. That is what carries a type into whatever holds it.
                if let Some(kind) = method_result(tables, built_names, receiver, &node.text)
                    && let Some(slot) = tables.expressions.result.get_mut(called.0 as usize)
                {
                    *slot = kind;
                }
                called
            }
            None => unsupported(tables),
        },
        "builtin" if complete => match builtin_method(&node.text) {
            // `@min(a, b)` is `a.min(b)`, so the first argument becomes the
            // receiver and the rest stay arguments.
            Some((method, arity)) if operands.len() == arity && !operands.is_empty() => {
                let text = push_string(&mut tables.strings, method.as_bytes());
                push_body_expression(tables, ExpressionKind::Method, text, node.line, &operands)
            }
            _ if node.text == "@intCast" && operands.len() == 1 => {
                // Rust infers what it is converting to, so no type has to be
                // invented, and the conversion is checked rather than silent.
                let into = push_string(&mut tables.strings, b"try_into");
                let converted = push_body_expression(
                    tables,
                    ExpressionKind::Method,
                    into,
                    node.line,
                    &operands,
                );
                let unwrap = push_string(&mut tables.strings, b"unwrap");
                push_body_expression(
                    tables,
                    ExpressionKind::Method,
                    unwrap,
                    node.line,
                    &[converted],
                )
            }
            // `@as(T, x)` is `x as T`, and the type is the first argument, so
            // nothing has to be invented. A conversion whose target the syntax
            // does not carry waits for one: it becomes a cast with no type and
            // is only spellable once an `@as` around it supplies one.
            _ if node.text == "@as" && operands.len() == 2 => {
                let named = tables
                    .expressions
                    .text
                    .get(operands[0].0 as usize)
                    .copied()
                    .unwrap_or(StringId(NO_INDEX));
                let value = operands[1];
                if tables.expressions.kind.get(value.0 as usize) == Some(&ExpressionKind::Cast)
                    && zag_facts::tables::string_bytes(
                        &tables.strings,
                        tables
                            .expressions
                            .text
                            .get(value.0 as usize)
                            .copied()
                            .unwrap_or(StringId(NO_INDEX)),
                    )
                    .is_empty()
                {
                    if let Some(slot) = tables.expressions.text.get_mut(value.0 as usize) {
                        *slot = named;
                    }
                    return value;
                }
                push_body_expression(tables, ExpressionKind::Cast, named, node.line, &[value])
            }
            _ if matches!(
                node.text.as_str(),
                "@floatFromInt" | "@intFromFloat" | "@truncate" | "@bitCast" | "@enumFromInt"
            ) && operands.len() == 1 =>
            {
                let waiting = push_string(&mut tables.strings, b"");
                push_body_expression(tables, ExpressionKind::Cast, waiting, node.line, &operands)
            }
            _ => unsupported(tables),
        },
        "call" if complete => translate_call(tables, built_names, scope, node, &operands),
        "switch" => {
            let arms: Vec<(ExpressionId, &program::Arm)> = node
                .arms
                .iter()
                .filter_map(|arm| built_child(built, Some(arm.node)).map(|body| (body, arm)))
                .collect();
            match (left, owner) {
                (Some(scrutinee), Some(owner)) if arms.len() == node.arms.len() => {
                    let mut children = vec![scrutinee];
                    for (body, arm) in arms {
                        let Some(pattern) = arm_pattern(tables, owner, &arm.pattern, &arm.capture)
                        else {
                            return unsupported(tables);
                        };
                        let spelled = push_string(&mut tables.strings, &pattern);
                        children.push(push_body_expression(
                            tables,
                            ExpressionKind::Arm,
                            spelled,
                            node.line,
                            &[body],
                        ));
                    }
                    push_body_expression(
                        tables,
                        ExpressionKind::Match,
                        StringId(NO_INDEX),
                        node.line,
                        &children,
                    )
                }
                _ => unsupported(tables),
            }
        }
        // A Zig loop may carry a step that runs at the end of every turn, and
        // Rust has nowhere to put one, so it goes at the end of the body where
        // it already ran.
        "while" => match (left, right) {
            (Some(condition), Some(body)) => {
                let body = match otherwise {
                    Some(step) => {
                        let mut statements: Vec<ExpressionId> =
                            zag_facts::tables::expression_children(
                                &tables.expressions,
                                body.0 as usize,
                            )
                            .filter_map(|slot| tables.expressions.children.get(slot).copied())
                            .collect();
                        if tables.expressions.kind.get(body.0 as usize)
                            != Some(&ExpressionKind::Block)
                        {
                            statements = vec![body];
                        }
                        statements.push(step);
                        push_body_expression(
                            tables,
                            ExpressionKind::Block,
                            StringId(NO_INDEX),
                            node.line,
                            &statements,
                        )
                    }
                    None => body,
                };
                push_body_expression(
                    tables,
                    ExpressionKind::While,
                    StringId(NO_INDEX),
                    node.line,
                    &[condition, body],
                )
            }
            _ => unsupported(tables),
        },
        "for" => match (left, right) {
            (Some(sequence), Some(body)) => {
                let text = spelled(tables);
                push_body_expression(
                    tables,
                    ExpressionKind::For,
                    text,
                    node.line,
                    &[sequence, body],
                )
            }
            _ => unsupported(tables),
        },
        "if" => match (left, right) {
            (Some(condition), Some(then)) => {
                let mut children = vec![condition, then];
                children.extend(otherwise);
                push_body_expression(
                    tables,
                    ExpressionKind::Branch,
                    StringId(NO_INDEX),
                    node.line,
                    &children,
                )
            }
            _ => unsupported(tables),
        },
        "block" if complete => push_body_expression(
            tables,
            ExpressionKind::Block,
            StringId(NO_INDEX),
            node.line,
            &operands,
        ),
        // A function returning `?T` says so in its signature, so a value on the
        // way out needs no type inferred: `null` is the empty option and
        // anything else is the option holding it. Only the boundary is decided
        // here, which is the only place the type is written down.
        "return" => {
            // A fallible function says so in its signature too, so the value
            // and the error are both spelled at the boundary rather than
            // inferred from the body.
            if fallible {
                // `return f()` where `f` can fail propagates the error union
                // rather than a value, so there is nothing to wrap. Zig writes
                // the two the same way and Rust does not.
                if let Some(value) = left
                    && returns_a_failure(tables, built_names, value)
                {
                    return push_body_expression(
                        tables,
                        ExpressionKind::Return,
                        StringId(NO_INDEX),
                        node.line,
                        &[value],
                    );
                }
                let wrapped = match left {
                    Some(value) => {
                        let constructor = if names_an_error(tables, built_names, scope, value) {
                            b"Err".as_slice()
                        } else {
                            b"Ok".as_slice()
                        };
                        let text = push_string(&mut tables.strings, constructor);
                        push_body_expression(
                            tables,
                            ExpressionKind::Wrap,
                            text,
                            node.line,
                            &[value],
                        )
                    }
                    None => {
                        let unit = push_string(&mut tables.strings, b"()");
                        let empty = push_body_expression(
                            tables,
                            ExpressionKind::Literal,
                            unit,
                            node.line,
                            &[],
                        );
                        let text = push_string(&mut tables.strings, b"Ok");
                        push_body_expression(
                            tables,
                            ExpressionKind::Wrap,
                            text,
                            node.line,
                            &[empty],
                        )
                    }
                };
                return push_body_expression(
                    tables,
                    ExpressionKind::Return,
                    StringId(NO_INDEX),
                    node.line,
                    &[wrapped],
                );
            }
            let children: Vec<ExpressionId> = match (optional_return, left) {
                (true, Some(value)) => {
                    let text = tables
                        .expressions
                        .text
                        .get(value.0 as usize)
                        .copied()
                        .map(|text| zag_facts::tables::string_bytes(&tables.strings, text).to_vec())
                        .unwrap_or_default();
                    if text == b"null" {
                        let none = push_string(&mut tables.strings, b"None");
                        vec![push_body_expression(
                            tables,
                            ExpressionKind::Identifier,
                            none,
                            node.line,
                            &[],
                        )]
                    } else {
                        let some = push_string(&mut tables.strings, b"Some");
                        vec![push_body_expression(
                            tables,
                            ExpressionKind::Wrap,
                            some,
                            node.line,
                            &[value],
                        )]
                    }
                }
                (_, left) => left.into_iter().collect(),
            };
            push_body_expression(
                tables,
                ExpressionKind::Return,
                StringId(NO_INDEX),
                node.line,
                &children,
            )
        }
        "let" => match left {
            Some(left) => {
                let text = spelled(tables);
                if let Some(kind) = tables.expressions.result.get(left.0 as usize).copied()
                    && kind.0 != NO_INDEX
                {
                    locals.insert(node.text.clone(), kind);
                }
                let declared =
                    push_body_expression(tables, ExpressionKind::Let, text, node.line, &[left]);
                if let Some(slot) = tables.expressions.parameter.get_mut(declared.0 as usize) {
                    *slot = u32::from(node.mutable);
                }
                declared
            }
            None => unsupported(tables),
        },
        "assign" => match (left, right) {
            (Some(left), Some(right)) => push_body_expression(
                tables,
                ExpressionKind::Assign,
                StringId(NO_INDEX),
                node.line,
                &[left, right],
            ),
            _ => unsupported(tables),
        },
        "expression" => match left {
            Some(left) => left,
            None => unsupported(tables),
        },
        _ => unsupported(tables),
    }
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

fn declare_memory(
    tables: &mut Tables,
    catalogue: &Catalogue,
    program: &Program,
    built: &Built,
    scope: &Scope,
    sources: &Sources,
) {
    for function in &program.functions {
        let Some(handle) = look_up(&built.functions, scope, &function.name) else {
            continue;
        };
        for initialiser in &function.initialisers {
            let Some(owner) = owner_of_initialiser(catalogue, function, initialiser) else {
                continue;
            };
            let Some(field) = field_of(tables, built, scope, &owner, &initialiser.field) else {
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
                catalogue,
                built,
                scope,
                function,
                Some(initialiser),
                kind,
                &initialiser.value,
            );
            set_expression_line(tables, expression, initialiser.line);
            let resolved = through_locals(function, &initialiser.value);
            if let Some(allocator) = allocating_call(resolved) {
                let source = classify_allocator(function, handle, sources, allocator);
                let operation = push_memory_operation(
                    tables,
                    handle,
                    MemoryOperationKind::Allocate,
                    source,
                    PlaceKind::FieldOfParameter,
                    field,
                );
                push_field_assignment_at(
                    tables,
                    field,
                    handle,
                    AssignmentSource::Allocation,
                    operation,
                    expression,
                    initialiser.line,
                );
                continue;
            }
            // A value the port could not read is still an assignment. Dropping
            // the row would make the report say nothing assigns the field,
            // which is a claim about the Zig rather than about the reading.
            let source = if function
                .parameters
                .iter()
                .any(|parameter| parameter.name == initialiser.value)
            {
                AssignmentSource::Parameter
            } else if is_literal(&initialiser.value) {
                AssignmentSource::StaticLiteral
            } else {
                AssignmentSource::Unknown
            };
            push_field_assignment_at(
                tables,
                field,
                handle,
                source,
                MemoryOperationId(NO_INDEX),
                expression,
                initialiser.line,
            );
        }

        for call in &function.calls {
            let name = last_segment(&call.callee);
            let kind = if FREEING.contains(&name) {
                MemoryOperationKind::Free
            } else if RESIZING.contains(&name) {
                MemoryOperationKind::Resize
            } else {
                continue;
            };
            // Both spellings take the allocation as their first argument, so
            // which field is touched is read the same way for either.
            let Some(text) = call.arguments.first() else {
                continue;
            };
            let Some(field) = freed_field(tables, catalogue, built, scope, function, text) else {
                continue;
            };
            let source = classify_allocator(function, handle, sources, receiver(&call.callee));
            push_memory_operation(
                tables,
                handle,
                kind,
                source,
                PlaceKind::FieldOfParameter,
                field,
            );
        }
    }
}

fn freed_field(
    tables: &Tables,
    catalogue: &Catalogue,
    built: &Built,
    scope: &Scope,
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
    let owner = catalogue.containers.get(owner).map(|entry| &entry.name)?;
    field_of(tables, built, scope, owner, field)
}
