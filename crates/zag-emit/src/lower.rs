use zag_analysis::ownership::{Ownership, OwnershipClass};
use zag_facts::build::push_string;
use zag_facts::tables::{
    ContainerKind, STRUCT_FLAG_EXTERN, TYPE_FLAG_SIGNED, Tables, TypeKind, string_bytes,
    struct_count, struct_fields,
};
use zag_facts::tables::{ROOT_MODULE, has_submodules, module_count};
use zag_facts::{ModuleId, NO_INDEX, StringId, StructId, TypeId};
use zag_render::ast::{
    Ast, Lifetime, NodeId, NodeKind, STRUCT_FLAG_ARENA_LIFETIME, STRUCT_FLAG_BORROW_LIFETIME,
    STRUCT_FLAG_REPR_C, empty_ast, push_node,
};

const MAXIMUM_TYPE_DEPTH: u32 = 8;

pub fn absent() -> StringId {
    StringId(NO_INDEX)
}

fn integer_name(bit_width: u32, signed: bool) -> String {
    let prefix = if signed { 'i' } else { 'u' };
    format!("{prefix}{bit_width}")
}

fn unit_type(ast: &mut Ast) -> NodeId {
    let name = push_string(&mut ast.strings, b"()");
    push_node(ast, NodeKind::TypePath, name, absent(), 0, 0, &[])
}

/// Zig has integers of every width and Rust has five. A `u3` widens to the
/// smallest Rust integer that holds it, which is what a hand port does, and a
/// width no Rust integer holds has no spelling at all.
fn rust_integer_width(bit_width: u32) -> Option<u32> {
    if bit_width == 0 {
        return None;
    }
    [8u32, 16, 32, 64, 128]
        .into_iter()
        .find(|width| *width >= bit_width)
}

/// Where a type is being spelled from. A name in another module needs the path
/// to that module in front of it, and a program that is one file has no other
/// module for a name to be in.
#[derive(Clone, Copy)]
pub struct Lowering<'a> {
    pub lifetimes: &'a [u32],
    /// The cross-table lookups, built once for the whole program. Carried here
    /// rather than passed alongside because everything that already takes a
    /// `Lowering` needs them.
    pub index: &'a crate::index::Index,
    pub module: ModuleId,
    pub qualified: bool,
}

pub fn lowering<'a>(
    lifetimes: &'a [u32],
    index: &'a crate::index::Index,
    module: ModuleId,
    qualified: bool,
) -> Lowering<'a> {
    Lowering {
        lifetimes,
        index,
        module,
        qualified,
    }
}

/// How a reference from here has to spell a name declared there.
///
/// Every module is a direct child of the port's root, so one `super` always
/// reaches it. The path is relative rather than rooted at the crate on
/// purpose: the port is checked by including it inside another module, and a
/// `crate::` path would break the moment it moved.
fn qualify(tables: &Tables, lowering: Lowering, kind: TypeId, name: &[u8]) -> Vec<u8> {
    let owner = tables
        .types
        .module
        .get(kind.0 as usize)
        .copied()
        .unwrap_or(ROOT_MODULE);
    if !lowering.qualified || owner == lowering.module || owner == ROOT_MODULE {
        return name.to_vec();
    }
    let module = tables
        .modules
        .name
        .get(owner.0 as usize)
        .map(|text| string_bytes(&tables.strings, *text))
        .unwrap_or(b"");
    if module.is_empty() {
        return name.to_vec();
    }
    let mut path = Vec::new();
    if lowering.module != ROOT_MODULE {
        path.extend_from_slice(b"super::");
    }
    path.extend_from_slice(module);
    path.extend_from_slice(b"::");
    path.extend_from_slice(name);
    path
}

pub fn lower_type_body(
    ast: &mut Ast,
    tables: &Tables,
    lowering: Lowering,
    kind: TypeId,
    depth: u32,
) -> NodeId {
    let index = kind.0 as usize;
    let Some(&type_kind) = tables.types.kind.get(index) else {
        return unit_type(ast);
    };
    if depth >= MAXIMUM_TYPE_DEPTH {
        return unit_type(ast);
    }
    let element = tables
        .types
        .element
        .get(index)
        .copied()
        .unwrap_or(TypeId(NO_INDEX));
    match type_kind {
        TypeKind::Slice => {
            let element = lower_type_body(ast, tables, lowering, element, depth + 1);
            push_node(
                ast,
                NodeKind::TypeSliceBody,
                absent(),
                absent(),
                0,
                0,
                &[element],
            )
        }
        TypeKind::Pointer => lower_type_body(ast, tables, lowering, element, depth + 1),
        TypeKind::Optional => {
            let inner = lower_type_body(ast, tables, lowering, element, depth + 1);
            push_node(
                ast,
                NodeKind::TypeOption,
                absent(),
                absent(),
                0,
                0,
                &[inner],
            )
        }
        TypeKind::Array => {
            let inner = lower_type_body(ast, tables, lowering, element, depth + 1);
            let count = tables.types.count.get(index).copied().unwrap_or(0);
            push_node(
                ast,
                NodeKind::TypeArray,
                absent(),
                absent(),
                count,
                0,
                &[inner],
            )
        }
        TypeKind::Integer => {
            let signed = tables
                .types
                .flags
                .get(index)
                .is_some_and(|flags| flags & TYPE_FLAG_SIGNED != 0);
            let declared = tables.types.bit_width.get(index).copied().unwrap_or(0);
            match rust_integer_width(declared) {
                Some(width) => {
                    let text = integer_name(width, signed);
                    let name = push_string(&mut ast.strings, text.as_bytes());
                    push_node(ast, NodeKind::TypePath, name, absent(), 0, 0, &[])
                }
                None => unit_type(ast),
            }
        }
        TypeKind::Bool => {
            let name = push_string(&mut ast.strings, b"bool");
            push_node(ast, NodeKind::TypePath, name, absent(), 0, 0, &[])
        }
        TypeKind::Void => unit_type(ast),
        TypeKind::Struct | TypeKind::Opaque => {
            let text = name_of(tables, tables.types.name.get(index));
            if text.is_empty() {
                return unit_type(ast);
            }
            let text = qualify(tables, lowering, kind, &text);
            let name = push_string(&mut ast.strings, &text);
            let carried = lowering.lifetimes.get(index).copied().unwrap_or(0);
            push_node(ast, NodeKind::TypePath, name, absent(), 0, carried, &[])
        }
    }
}

/// A named struct has to be spelled with the lifetimes it declares, so the
/// lifetimes of every struct are settled before any field that mentions one is
/// lowered. Indexed by type rather than by struct, which is how a field names
/// what it points at.
pub fn lifetimes_by_type(tables: &Tables, ownership: &Ownership) -> Vec<u32> {
    let mut carried = vec![0u32; tables.types.kind.len()];
    for index in 0..struct_count(&tables.structs) {
        let owner = StructId(index as u32);
        let flags = struct_flags(tables, ownership, owner)
            & (STRUCT_FLAG_BORROW_LIFETIME | STRUCT_FLAG_ARENA_LIFETIME);
        if let Some(&kind) = tables.structs.type_id.get(index)
            && let Some(slot) = carried.get_mut(kind.0 as usize)
        {
            *slot = flags;
        }
    }
    carried
}

pub fn lower_field_type(
    ast: &mut Ast,
    tables: &Tables,
    lowering: Lowering,
    kind: TypeId,
    class: OwnershipClass,
) -> NodeId {
    // The ownership wrapper belongs inside the option, so a field that is
    // owned and optional comes across as an optional box rather than a box of
    // an option, which is a different type.
    if tables.types.kind.get(kind.0 as usize) == Some(&TypeKind::Optional) {
        let element = tables
            .types
            .element
            .get(kind.0 as usize)
            .copied()
            .unwrap_or(TypeId(NO_INDEX));
        let inner = lower_field_type(ast, tables, lowering, element, class);
        return push_node(
            ast,
            NodeKind::TypeOption,
            absent(),
            absent(),
            0,
            0,
            &[inner],
        );
    }
    let body = lower_type_body(ast, tables, lowering, kind, 0);
    match class {
        OwnershipClass::Value => body,
        OwnershipClass::Owned => {
            push_node(ast, NodeKind::TypeBoxed, absent(), absent(), 0, 0, &[body])
        }
        OwnershipClass::Borrowed => reference(ast, body, Lifetime::Borrow),
        OwnershipClass::Static => reference(ast, body, Lifetime::Static),
        OwnershipClass::Arena => reference(ast, body, Lifetime::Arena),
        OwnershipClass::Unknown => push_node(
            ast,
            NodeKind::TypeOptionNonNull,
            absent(),
            absent(),
            0,
            0,
            &[body],
        ),
    }
}

fn reference(ast: &mut Ast, body: NodeId, lifetime: Lifetime) -> NodeId {
    push_node(
        ast,
        NodeKind::TypeReference,
        absent(),
        absent(),
        0,
        lifetime as u32,
        &[body],
    )
}

/// The lifetimes a struct declares, which is what an `impl` block for it puts
/// in scope for everything written inside.
pub fn lifetimes_of(tables: &Tables, ownership: &Ownership, owner: StructId) -> u32 {
    struct_flags(tables, ownership, owner)
        & (STRUCT_FLAG_BORROW_LIFETIME | STRUCT_FLAG_ARENA_LIFETIME)
}

fn struct_flags(tables: &Tables, ownership: &Ownership, owner: StructId) -> u32 {
    let mut flags = 0;
    if is_extern(tables, owner) {
        flags |= STRUCT_FLAG_REPR_C;
    }
    for row in struct_fields(&tables.structs, owner) {
        match ownership.class.get(row) {
            Some(OwnershipClass::Borrowed) => flags |= STRUCT_FLAG_BORROW_LIFETIME,
            Some(OwnershipClass::Arena) => flags |= STRUCT_FLAG_ARENA_LIFETIME,
            _ => {}
        }
    }
    flags
}

fn is_extern(tables: &Tables, owner: StructId) -> bool {
    tables
        .structs
        .flags
        .get(owner.0 as usize)
        .is_some_and(|flags| flags & STRUCT_FLAG_EXTERN != 0)
}

pub fn name_of(tables: &Tables, name: Option<&StringId>) -> Vec<u8> {
    name.map(|name| string_bytes(&tables.strings, *name).to_vec())
        .unwrap_or_default()
}

/// A Zig enum has variants with no payload and a Zig union has variants that
/// carry one. Both become a Rust enum, and the payload is a child of the
/// variant when the member declared a type.
fn lower_enum(
    ast: &mut Ast,
    tables: &Tables,
    lowering: Lowering,
    owner: StructId,
    carries_payloads: bool,
) -> NodeId {
    let mut variants = Vec::new();
    for row in struct_fields(&tables.structs, owner) {
        let text = pascal_case(&name_of(tables, tables.fields.name.get(row)));
        let name = push_string(&mut ast.strings, &text);
        let mut payload = Vec::new();
        if carries_payloads
            && let Some(&kind) = tables.fields.field_type.get(row)
            && !is_void(tables, kind)
        {
            payload.push(lower_type_body(ast, tables, lowering, kind, 0));
        }
        variants.push(push_node(
            ast,
            NodeKind::Variant,
            name,
            absent(),
            0,
            0,
            &payload,
        ));
    }
    let text = name_of(tables, tables.structs.name.get(owner.0 as usize));
    let name = push_string(&mut ast.strings, &text);
    let flags = if is_extern(tables, owner) {
        STRUCT_FLAG_REPR_C
    } else {
        0
    };
    push_node(ast, NodeKind::Enum, name, absent(), 0, flags, &variants)
}

/// Zig spells a variant in snake case and Rust spells it in Pascal case, so a
/// port that keeps the Zig spelling is a port the compiler complains about.
fn pascal_case(name: &[u8]) -> Vec<u8> {
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

fn is_void(tables: &Tables, kind: TypeId) -> bool {
    tables
        .types
        .kind
        .get(kind.0 as usize)
        .is_some_and(|entry| *entry == TypeKind::Void)
}

fn lower_struct(
    ast: &mut Ast,
    tables: &Tables,
    ownership: &Ownership,
    lowering: Lowering,
    owner: StructId,
) -> NodeId {
    let mut fields = Vec::new();
    for row in struct_fields(&tables.structs, owner) {
        let (Some(&field_type), Some(&class)) =
            (tables.fields.field_type.get(row), ownership.class.get(row))
        else {
            continue;
        };
        let field_type = lower_field_type(ast, tables, lowering, field_type, class);
        let text = name_of(tables, tables.fields.name.get(row));
        let name = push_string(&mut ast.strings, &text);
        fields.push(push_node(
            ast,
            NodeKind::Field,
            name,
            absent(),
            0,
            0,
            &[field_type],
        ));
    }
    let text = name_of(tables, tables.structs.name.get(owner.0 as usize));
    let name = push_string(&mut ast.strings, &text);
    let flags = struct_flags(tables, ownership, owner);
    push_node(ast, NodeKind::Struct, name, absent(), 0, flags, &fields)
}

fn lower_layout_assertions(
    ast: &mut Ast,
    tables: &Tables,
    owner: StructId,
    items: &mut Vec<NodeId>,
) {
    let index = owner.0 as usize;
    if !is_extern(tables, owner) {
        return;
    }
    let (Some(&size), Some(&alignment)) = (
        tables.structs.size.get(index),
        tables.structs.alignment.get(index),
    ) else {
        return;
    };
    let text = name_of(tables, tables.structs.name.get(index));
    let name = push_string(&mut ast.strings, &text);
    items.push(push_node(
        ast,
        NodeKind::AssertSize,
        name,
        absent(),
        size,
        0,
        &[],
    ));
    items.push(push_node(
        ast,
        NodeKind::AssertAlignment,
        name,
        absent(),
        alignment,
        0,
        &[],
    ));
    for row in struct_fields(&tables.structs, owner) {
        let Some(&offset) = tables.fields.offset.get(row) else {
            continue;
        };
        let text = name_of(tables, tables.fields.name.get(row));
        let field_name = push_string(&mut ast.strings, &text);
        items.push(push_node(
            ast,
            NodeKind::AssertOffset,
            name,
            field_name,
            offset,
            0,
            &[],
        ));
    }
}

/// Everything one module declares, in declaration order. The root's items go
/// at the top level and every other module's go inside a `pub mod`, which is
/// the shape the Zig already had: its root file is the program's namespace and
/// each other file is a namespace inside it.
fn lower_module(
    ast: &mut Ast,
    tables: &Tables,
    ownership: &Ownership,
    lowering: Lowering,
    items: &mut Vec<NodeId>,
) {
    for row in crate::index::structs_of(lowering.index, lowering.module) {
        let index = *row as usize;
        let owner = StructId(*row);
        if tables.structs.module.get(index).copied() != Some(lowering.module) {
            continue;
        }
        match tables.structs.kind.get(index).copied() {
            Some(ContainerKind::Enum) | Some(ContainerKind::ErrorSet) => {
                items.push(lower_enum(ast, tables, lowering, owner, false));
                continue;
            }
            Some(ContainerKind::Union) => {
                items.push(lower_enum(ast, tables, lowering, owner, true));
                continue;
            }
            _ => {}
        }
        items.push(lower_struct(ast, tables, ownership, lowering, owner));
        lower_layout_assertions(ast, tables, owner, items);
        lower_implementation(ast, tables, ownership, lowering, owner, items);
    }
    items.extend(crate::function::signatures_for(
        ast, tables, ownership, lowering, None,
    ));
}

pub fn lower(tables: &Tables, ownership: &Ownership) -> Ast {
    let mut ast = empty_ast();
    let lifetimes = lifetimes_by_type(tables, ownership);
    let lookups = crate::index::build_index(tables);
    let qualified = has_submodules(&tables.modules);
    let mut items = Vec::new();
    lower_module(
        &mut ast,
        tables,
        ownership,
        lowering(&lifetimes, &lookups, ROOT_MODULE, qualified),
        &mut items,
    );
    for index in 1..module_count(&tables.modules) {
        let module = ModuleId(index as u32);
        let mut inside = Vec::new();
        lower_module(
            &mut ast,
            tables,
            ownership,
            lowering(&lifetimes, &lookups, module, qualified),
            &mut inside,
        );
        if inside.is_empty() {
            continue;
        }
        let text = name_of(tables, tables.modules.name.get(index));
        let name = push_string(&mut ast.strings, &text);
        items.push(push_node(
            &mut ast,
            NodeKind::Module,
            name,
            absent(),
            0,
            0,
            &inside,
        ));
    }
    ast.root = push_node(&mut ast, NodeKind::File, absent(), absent(), 0, 0, &items);
    ast
}

/// Everything the port writes for one struct goes in one `impl` block, so a
/// constructor and the signatures beside it do not each open their own.
fn lower_implementation(
    ast: &mut Ast,
    tables: &Tables,
    ownership: &Ownership,
    lowering: Lowering,
    owner: StructId,
    items: &mut Vec<NodeId>,
) {
    let mut methods: Vec<NodeId> =
        crate::constructor::lower_constructor(ast, tables, ownership, lowering, owner)
            .into_iter()
            .collect();
    methods.extend(crate::function::signatures_for(
        ast,
        tables,
        ownership,
        lowering,
        Some(owner),
    ));
    if methods.is_empty() {
        return;
    }
    let text = name_of(tables, tables.structs.name.get(owner.0 as usize));
    let name = push_string(&mut ast.strings, &text);
    let flags = lifetimes_of(tables, ownership, owner);
    items.push(push_node(
        ast,
        NodeKind::Implementation,
        name,
        absent(),
        0,
        flags,
        &methods,
    ));
}
