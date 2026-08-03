//! Writes a constructor for a struct whose Zig `init` sets every field to
//! something the port can spell. Anything else emits nothing, because a body
//! that is partly ported is worse than a body a person still has to write.
//!
//! The allocator parameter disappears. Where provenance resolved it to the
//! global allocator, `Box` is the allocator, so the port takes one fewer
//! argument than the Zig did.

use crate::lower::{Lowering, absent, lower_field_type, name_of};
use zag_analysis::ownership::{Ownership, OwnershipClass};
use zag_facts::build::push_string;
use zag_facts::tables::{
    ExpressionKind, PARAMETER_FLAG_ALLOCATOR, Tables, function_count, function_parameters,
    string_bytes, struct_fields,
};
use zag_facts::{ExpressionId, FieldId, FunctionId, NO_INDEX, StructId};
use zag_render::ast::{Ast, NodeId, NodeKind, push_node};

fn expression_children(tables: &Tables, expression: ExpressionId) -> std::ops::Range<usize> {
    let index = expression.0 as usize;
    let (Some(&start), Some(&count)) = (
        tables.expressions.child_start.get(index),
        tables.expressions.child_count.get(index),
    ) else {
        return 0..0;
    };
    let start = start as usize;
    start..start.saturating_add(count as usize)
}

fn is_writable(tables: &Tables, expression: ExpressionId) -> bool {
    let Some(&kind) = tables.expressions.kind.get(expression.0 as usize) else {
        return false;
    };
    if kind == ExpressionKind::Unsupported {
        return false;
    }
    expression_children(tables, expression).all(|slot| {
        tables
            .expressions
            .children
            .get(slot)
            .is_some_and(|child| is_writable(tables, *child))
    })
}

fn parameter_name(tables: &Tables, function: FunctionId, index: u32) -> Vec<u8> {
    function_parameters(&tables.functions, function)
        .nth(index as usize)
        .map(|row| string_bytes(&tables.strings, tables.parameters.name[row]).to_vec())
        .unwrap_or_default()
}

fn integer_name(tables: &Tables, kind: zag_facts::TypeId) -> Vec<u8> {
    let index = kind.0 as usize;
    let signed = tables
        .types
        .flags
        .get(index)
        .is_some_and(|flags| flags & zag_facts::tables::TYPE_FLAG_SIGNED != 0);
    let width = tables.types.bit_width.get(index).copied().unwrap_or(32);
    let prefix = if signed { 'i' } else { 'u' };
    format!("{prefix}{width}").into_bytes()
}

fn lower_expression(
    ast: &mut Ast,
    tables: &Tables,
    function: FunctionId,
    expression: ExpressionId,
) -> NodeId {
    let index = expression.0 as usize;
    let kind = tables
        .expressions
        .kind
        .get(index)
        .copied()
        .unwrap_or(ExpressionKind::Unsupported);
    let parameter = tables
        .expressions
        .parameter
        .get(index)
        .copied()
        .unwrap_or(0);
    match kind {
        ExpressionKind::Literal => {
            let text = string_bytes(&tables.strings, tables.expressions.text[index]).to_vec();
            let name = push_string(&mut ast.strings, &text);
            push_node(ast, NodeKind::ExpressionLiteral, name, absent(), 0, 0, &[])
        }
        ExpressionKind::Parameter => {
            let text = parameter_name(tables, function, parameter);
            let name = push_string(&mut ast.strings, &text);
            push_node(ast, NodeKind::ExpressionPath, name, absent(), 0, 0, &[])
        }
        ExpressionKind::Length => {
            let mut text = parameter_name(tables, function, parameter);
            text.extend_from_slice(b".len");
            let name = push_string(&mut ast.strings, &text);
            push_node(ast, NodeKind::ExpressionCall, name, absent(), 0, 0, &[])
        }
        ExpressionKind::Cast => {
            let inner = expression_children(tables, expression)
                .next()
                .and_then(|slot| tables.expressions.children.get(slot).copied());
            let child = match inner {
                Some(child) => lower_expression(ast, tables, function, child),
                None => return unsupported(ast),
            };
            let text = integer_name(tables, tables.expressions.result[index]);
            let name = push_string(&mut ast.strings, &text);
            push_node(ast, NodeKind::ExpressionTry, name, absent(), 0, 0, &[child])
        }
        ExpressionKind::Allocation => {
            let text = parameter_name(tables, function, parameter);
            let argument = push_string(&mut ast.strings, &text);
            let path = push_node(ast, NodeKind::ExpressionPath, argument, absent(), 0, 0, &[]);
            let callee = push_string(&mut ast.strings, b"Box::from");
            push_node(
                ast,
                NodeKind::ExpressionCall,
                callee,
                absent(),
                0,
                0,
                &[path],
            )
        }
        ExpressionKind::StructLiteral => {
            let field = tables.expressions.field[index];
            if field.0 != NO_INDEX {
                let inner = expression_children(tables, expression)
                    .next()
                    .and_then(|slot| tables.expressions.children.get(slot).copied());
                let value = match inner {
                    Some(child) => lower_expression(ast, tables, function, child),
                    None => return unsupported(ast),
                };
                let text = name_of(tables, tables.fields.name.get(field.0 as usize));
                let name = push_string(&mut ast.strings, &text);
                return push_node(ast, NodeKind::FieldValue, name, absent(), 0, 0, &[value]);
            }
            let children: Vec<NodeId> = expression_children(tables, expression)
                .filter_map(|slot| tables.expressions.children.get(slot).copied())
                .map(|child| lower_expression(ast, tables, function, child))
                .collect();
            let text = struct_named(tables, tables.expressions.result[index]);
            let name = push_string(&mut ast.strings, &text);
            push_node(
                ast,
                NodeKind::ExpressionStruct,
                name,
                absent(),
                0,
                0,
                &children,
            )
        }
        ExpressionKind::Unsupported => unsupported(ast),
    }
}

fn unsupported(ast: &mut Ast) -> NodeId {
    let name = push_string(&mut ast.strings, b"()");
    push_node(ast, NodeKind::ExpressionLiteral, name, absent(), 0, 0, &[])
}

fn struct_named(tables: &Tables, kind: zag_facts::TypeId) -> Vec<u8> {
    string_bytes(&tables.strings, tables.types.name[kind.0 as usize]).to_vec()
}

fn assignment_for(tables: &Tables, function: FunctionId, field: FieldId) -> Option<ExpressionId> {
    (0..tables.field_assignments.field.len())
        .find(|row| {
            tables.field_assignments.field[*row] == field
                && tables.field_assignments.function[*row] == function
        })
        .and_then(|row| tables.field_assignments.expression.get(row).copied())
        .filter(|expression| expression.0 != NO_INDEX)
}

/// The Zig `init` a constructor can be written from, which is the one that
/// belongs to the struct and sets every field to something writable. The
/// report asks the same question to say what became of each function.
pub fn writable_init(
    tables: &Tables,
    ownership: &Ownership,
    owner: StructId,
) -> Option<FunctionId> {
    let function = (0..function_count(&tables.functions))
        .map(|index| FunctionId(index as u32))
        .find(|handle| {
            tables.functions.owner.get(handle.0 as usize) == Some(&owner)
                && string_bytes(&tables.strings, tables.functions.name[handle.0 as usize])
                    == b"init"
        })?;
    let mut fields = 0;
    for row in struct_fields(&tables.structs, owner) {
        let expression = assignment_for(tables, function, FieldId(row as u32))?;
        if !is_writable(tables, expression) {
            return None;
        }
        if ownership.class.get(row).copied() == Some(OwnershipClass::Unknown) {
            return None;
        }
        fields += 1;
    }
    (fields > 0).then_some(function)
}

/// A constructor is written only when the Zig `init` belongs to the struct and
/// sets every one of its fields to something writable.
pub fn lower_constructor(
    ast: &mut Ast,
    tables: &Tables,
    ownership: &Ownership,
    lowering: &Lowering,
    owner: StructId,
) -> Option<NodeId> {
    let function = writable_init(tables, ownership, owner)?;

    let mut values = Vec::new();
    for row in struct_fields(&tables.structs, owner) {
        let field = FieldId(row as u32);
        let expression = assignment_for(tables, function, field)?;
        let value = lower_expression(ast, tables, function, expression);
        let text = name_of(tables, tables.fields.name.get(row));
        let name = push_string(&mut ast.strings, &text);
        values.push(push_node(
            ast,
            NodeKind::FieldValue,
            name,
            absent(),
            0,
            0,
            &[value],
        ));
    }
    if values.is_empty() {
        return None;
    }

    let mut children = Vec::new();
    let mut count = 0;
    for row in function_parameters(&tables.functions, function) {
        if tables.parameters.flags[row] & PARAMETER_FLAG_ALLOCATOR != 0 {
            continue;
        }
        // A slice or a pointer comes across as a borrow with the lifetime
        // elided, and everything else comes across by value.
        let declared = tables.parameters.parameter_type[row];
        let kind = if zag_facts::tables::is_reference_type(&tables.types, declared) {
            let body = crate::lower::lower_type_body(ast, tables, lowering, declared, 0);
            push_node(
                ast,
                NodeKind::TypeReference,
                absent(),
                absent(),
                0,
                zag_render::ast::Lifetime::Elided as u32,
                &[body],
            )
        } else {
            lower_field_type(ast, tables, lowering, declared, OwnershipClass::Value)
        };
        let text = string_bytes(&tables.strings, tables.parameters.name[row]).to_vec();
        let name = push_string(&mut ast.strings, &text);
        children.push(push_node(
            ast,
            NodeKind::Parameter,
            name,
            absent(),
            0,
            0,
            &[kind],
        ));
        count += 1;
    }
    let returns = push_string(&mut ast.strings, b"Self");
    children.push(push_node(
        ast,
        NodeKind::TypePath,
        returns,
        absent(),
        0,
        0,
        &[],
    ));
    let literal = push_string(&mut ast.strings, b"Self");
    children.push(push_node(
        ast,
        NodeKind::ExpressionStruct,
        literal,
        absent(),
        0,
        0,
        &values,
    ));

    let name = push_string(&mut ast.strings, b"new");
    let method = push_node(ast, NodeKind::Function, name, absent(), count, 0, &children);
    let text = name_of(tables, tables.structs.name.get(owner.0 as usize));
    let owner_name = push_string(&mut ast.strings, &text);
    Some(push_node(
        ast,
        NodeKind::Implementation,
        owner_name,
        absent(),
        0,
        0,
        &[method],
    ))
}
