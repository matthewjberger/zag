use zag_analysis::ownership::{Ownership, OwnershipClass};
use zag_facts::build::push_string;
use zag_facts::tables::{
    ContainerKind, STRUCT_FLAG_EXTERN, TYPE_FLAG_SIGNED, Tables, TypeKind, string_bytes,
    struct_count, struct_fields,
};
use zag_facts::{NO_INDEX, StringId, StructId, TypeId};
use zag_render::ast::{
    Ast, Lifetime, NodeId, NodeKind, STRUCT_FLAG_ARENA_LIFETIME, STRUCT_FLAG_BORROW_LIFETIME,
    STRUCT_FLAG_REPR_C, empty_ast, push_node,
};

const MAXIMUM_TYPE_DEPTH: u32 = 8;

fn absent() -> StringId {
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

fn lower_type_body(
    ast: &mut Ast,
    tables: &Tables,
    lifetimes: &[u32],
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
            let element = lower_type_body(ast, tables, lifetimes, element, depth + 1);
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
        TypeKind::Pointer => lower_type_body(ast, tables, lifetimes, element, depth + 1),
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
            let name = push_string(&mut ast.strings, &text);
            let carried = lifetimes.get(index).copied().unwrap_or(0);
            push_node(ast, NodeKind::TypePath, name, absent(), 0, carried, &[])
        }
    }
}

/// A named struct has to be spelled with the lifetimes it declares, so the
/// lifetimes of every struct are settled before any field that mentions one is
/// lowered. Indexed by type rather than by struct, which is how a field names
/// what it points at.
fn lifetimes_by_type(tables: &Tables, ownership: &Ownership) -> Vec<u32> {
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

fn lower_field_type(
    ast: &mut Ast,
    tables: &Tables,
    lifetimes: &[u32],
    kind: TypeId,
    class: OwnershipClass,
) -> NodeId {
    let body = lower_type_body(ast, tables, lifetimes, kind, 0);
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

fn name_of(tables: &Tables, name: Option<&StringId>) -> Vec<u8> {
    name.map(|name| string_bytes(&tables.strings, *name).to_vec())
        .unwrap_or_default()
}

/// A Zig enum has variants with no payload and a Zig union has variants that
/// carry one. Both become a Rust enum, and the payload is a child of the
/// variant when the member declared a type.
fn lower_enum(
    ast: &mut Ast,
    tables: &Tables,
    lifetimes: &[u32],
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
            payload.push(lower_type_body(ast, tables, lifetimes, kind, 0));
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
    lifetimes: &[u32],
    owner: StructId,
) -> NodeId {
    let mut fields = Vec::new();
    for row in struct_fields(&tables.structs, owner) {
        let (Some(&field_type), Some(&class)) =
            (tables.fields.field_type.get(row), ownership.class.get(row))
        else {
            continue;
        };
        let field_type = lower_field_type(ast, tables, lifetimes, field_type, class);
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

pub fn lower(tables: &Tables, ownership: &Ownership) -> Ast {
    let mut ast = empty_ast();
    let lifetimes = lifetimes_by_type(tables, ownership);
    let mut items = Vec::new();
    for index in 0..struct_count(&tables.structs) {
        let owner = StructId(index as u32);
        match tables.structs.kind.get(index).copied() {
            Some(ContainerKind::Enum) | Some(ContainerKind::ErrorSet) => {
                items.push(lower_enum(&mut ast, tables, &lifetimes, owner, false));
                continue;
            }
            Some(ContainerKind::Union) => {
                items.push(lower_enum(&mut ast, tables, &lifetimes, owner, true));
                continue;
            }
            _ => {}
        }
        items.push(lower_struct(&mut ast, tables, ownership, &lifetimes, owner));
        lower_layout_assertions(&mut ast, tables, owner, &mut items);
    }
    ast.root = push_node(&mut ast, NodeKind::File, absent(), absent(), 0, 0, &items);
    ast
}
