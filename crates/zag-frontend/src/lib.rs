pub mod program;
pub mod project;

use program::{Function, Layout, Program};
use std::collections::BTreeMap;
use zag_facts::build::{
    declare_field, declare_function, declare_module, declare_parameter, intern, name_root_module,
    push_allocator_source, push_array_type, push_call, push_call_argument, push_expression,
    push_field_assignment_with, push_integer_type, push_memory_operation, push_opaque_type,
    push_optional_type, push_pointer_type, push_slice_type, push_string, push_struct,
    push_struct_type, push_unresolved_import, set_function_module, set_function_signature,
    set_struct_deinit, set_struct_kind, set_struct_module, set_type_module,
};
use zag_facts::handles::{
    ExpressionId, FieldId, FunctionId, MemoryOperationId, ModuleId, NO_INDEX, StringId, StructId,
    TypeId,
};
use zag_facts::tables::{
    AllocatorSourceKind, AssignmentSource, ContainerKind, ExpressionKind, MemoryOperationKind,
    PARAMETER_FLAG_ALLOCATOR, PARAMETER_FLAG_MUTABLE, PlaceKind, ROOT_MODULE, STRUCT_FLAG_EXTERN,
    Tables, empty_tables,
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

fn resolve(tables: &mut Tables, resolver: &mut Resolver, scope: &Scope, text: &str) -> TypeId {
    let text = strip_error_union(text).trim();
    if let Some(rest) = text.strip_prefix('?') {
        let element = resolve(tables, resolver, scope, rest);
        return push_optional_type(tables, element);
    }
    if let Some(rest) = text.strip_prefix("[]") {
        let element = resolve(
            tables,
            resolver,
            scope,
            rest.trim_start_matches("const ").trim(),
        );
        return push_slice_type(tables, element);
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
        let element = resolve(
            tables,
            resolver,
            scope,
            rest.trim_start_matches("const ").trim(),
        );
        return push_pointer_type(tables, element);
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
pub fn build_project(modules: &[project::SourceModule], target: &str) -> Tables {
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
                push_field_assignment_with(
                    tables,
                    field,
                    handle,
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
                handle,
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
            let Some(field) = freed_field(tables, catalogue, built, scope, function, text) else {
                continue;
            };
            let source = classify_allocator(function, handle, sources, receiver(&call.callee));
            push_memory_operation(
                tables,
                handle,
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
