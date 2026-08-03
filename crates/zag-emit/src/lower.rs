use zag_analysis::ownership::{Ownership, OwnershipClass};
use zag_facts::build::push_string;
use zag_facts::tables::{
    STRUCT_FLAG_EXTERN, TYPE_FLAG_SIGNED, Tables, TypeKind, string_bytes, struct_count,
    struct_fields,
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

fn lower_type_body(ast: &mut Ast, tables: &Tables, kind: TypeId, depth: u32) -> NodeId {
    let index = kind.0 as usize;
    if depth >= MAXIMUM_TYPE_DEPTH || index >= tables.types.kind.len() {
        let name = push_string(&mut ast.strings, b"()");
        return push_node(ast, NodeKind::TypePath, name, absent(), 0, 0, &[]);
    }
    match tables.types.kind[index] {
        TypeKind::Slice => {
            let element = lower_type_body(ast, tables, tables.types.element[index], depth + 1);
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
        TypeKind::Pointer => lower_type_body(ast, tables, tables.types.element[index], depth + 1),
        TypeKind::Integer => {
            let signed = tables.types.flags[index] & TYPE_FLAG_SIGNED != 0;
            let text = integer_name(tables.types.bit_width[index], signed);
            let name = push_string(&mut ast.strings, text.as_bytes());
            push_node(ast, NodeKind::TypePath, name, absent(), 0, 0, &[])
        }
        TypeKind::Bool => {
            let name = push_string(&mut ast.strings, b"bool");
            push_node(ast, NodeKind::TypePath, name, absent(), 0, 0, &[])
        }
        TypeKind::Void => {
            let name = push_string(&mut ast.strings, b"()");
            push_node(ast, NodeKind::TypePath, name, absent(), 0, 0, &[])
        }
        TypeKind::Struct | TypeKind::Opaque => {
            let text = string_bytes(&tables.strings, tables.types.name[index]).to_vec();
            let name = push_string(&mut ast.strings, &text);
            push_node(ast, NodeKind::TypePath, name, absent(), 0, 0, &[])
        }
    }
}

fn lower_field_type(ast: &mut Ast, tables: &Tables, kind: TypeId, class: OwnershipClass) -> NodeId {
    let body = lower_type_body(ast, tables, kind, 0);
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
    if tables.structs.flags[owner.0 as usize] & STRUCT_FLAG_EXTERN != 0 {
        flags |= STRUCT_FLAG_REPR_C;
    }
    for row in struct_fields(&tables.structs, owner) {
        match ownership.class[row] {
            OwnershipClass::Borrowed => flags |= STRUCT_FLAG_BORROW_LIFETIME,
            OwnershipClass::Arena => flags |= STRUCT_FLAG_ARENA_LIFETIME,
            _ => {}
        }
    }
    flags
}

fn lower_struct(ast: &mut Ast, tables: &Tables, ownership: &Ownership, owner: StructId) -> NodeId {
    let mut fields = Vec::new();
    for row in struct_fields(&tables.structs, owner) {
        let field_type = lower_field_type(
            ast,
            tables,
            tables.fields.field_type[row],
            ownership.class[row],
        );
        let text = string_bytes(&tables.strings, tables.fields.name[row]).to_vec();
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
    let text = string_bytes(&tables.strings, tables.structs.name[owner.0 as usize]).to_vec();
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
    if tables.structs.flags[index] & STRUCT_FLAG_EXTERN == 0 {
        return;
    }
    let text = string_bytes(&tables.strings, tables.structs.name[index]).to_vec();
    let name = push_string(&mut ast.strings, &text);
    items.push(push_node(
        ast,
        NodeKind::AssertSize,
        name,
        absent(),
        tables.structs.size[index],
        0,
        &[],
    ));
    items.push(push_node(
        ast,
        NodeKind::AssertAlignment,
        name,
        absent(),
        tables.structs.alignment[index],
        0,
        &[],
    ));
    for row in struct_fields(&tables.structs, owner) {
        let text = string_bytes(&tables.strings, tables.fields.name[row]).to_vec();
        let field_name = push_string(&mut ast.strings, &text);
        items.push(push_node(
            ast,
            NodeKind::AssertOffset,
            name,
            field_name,
            tables.fields.offset[row],
            0,
            &[],
        ));
    }
}

pub fn lower(tables: &Tables, ownership: &Ownership) -> Ast {
    let mut ast = empty_ast();
    let mut items = Vec::new();
    for index in 0..struct_count(&tables.structs) {
        let owner = StructId(index as u32);
        items.push(lower_struct(&mut ast, tables, ownership, owner));
        lower_layout_assertions(&mut ast, tables, owner, &mut items);
    }
    ast.root = push_node(&mut ast, NodeKind::File, absent(), absent(), 0, 0, &items);
    ast
}
