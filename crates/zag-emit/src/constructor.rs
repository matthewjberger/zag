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
    ExpressionKind, PARAMETER_FLAG_ALLOCATOR, Tables, function_parameters, string_bytes,
    struct_fields,
};
use zag_facts::{ExpressionId, FieldId, FunctionId, NO_INDEX, StringId, StructId};
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

/// The class of the field the value is going into, which decides how an
/// allocation is copied. Nothing else in an initialiser depends on it.
fn class_of(ownership: &Ownership, field: FieldId) -> OwnershipClass {
    ownership
        .class
        .get(field.0 as usize)
        .copied()
        .unwrap_or(OwnershipClass::Unknown)
}

fn lower_expression(
    ast: &mut Ast,
    tables: &Tables,
    ownership: &Ownership,
    function: FunctionId,
    field: FieldId,
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
                Some(child) => lower_expression(ast, tables, ownership, function, field, child),
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
            // Both copy what they were handed. Which one is right is decided by
            // the field, because that is what fixes the type it is copied into.
            if class_of(ownership, field) == OwnershipClass::Grown {
                let method = push_string(&mut ast.strings, b"to_vec");
                return push_node(
                    ast,
                    NodeKind::ExpressionMethod,
                    method,
                    absent(),
                    0,
                    0,
                    &[path],
                );
            }
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
                    Some(child) => lower_value(ast, tables, ownership, function, field, child),
                    None => return unsupported(ast),
                };
                let text = name_of(tables, tables.fields.name.get(field.0 as usize));
                let name = push_string(&mut ast.strings, &text);
                return push_node(ast, NodeKind::FieldValue, name, absent(), 0, 0, &[value]);
            }
            let children: Vec<NodeId> = expression_children(tables, expression)
                .filter_map(|slot| tables.expressions.children.get(slot).copied())
                .map(|child| lower_expression(ast, tables, ownership, function, field, child))
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
        ExpressionKind::Null => {
            let name = push_string(&mut ast.strings, b"None");
            push_node(ast, NodeKind::ExpressionPath, name, absent(), 0, 0, &[])
        }
        // Everything a body is made of, which a field initialiser never is.
        _ => unsupported(ast),
    }
}

/// Anything but `null` going into an optional field is the value the option
/// holds rather than the option itself, so the port wraps it.
fn lower_value(
    ast: &mut Ast,
    tables: &Tables,
    ownership: &Ownership,
    function: FunctionId,
    field: FieldId,
    expression: ExpressionId,
) -> NodeId {
    let node = lower_expression(ast, tables, ownership, function, field, expression);
    let kind = tables.expressions.kind.get(expression.0 as usize).copied();
    if kind == Some(ExpressionKind::Null) {
        return node;
    }
    let declared = tables
        .fields
        .field_type
        .get(field.0 as usize)
        .copied()
        .unwrap_or(zag_facts::TypeId(NO_INDEX));
    if tables.types.kind.get(declared.0 as usize) != Some(&zag_facts::tables::TypeKind::Optional) {
        return node;
    }
    let callee = push_string(&mut ast.strings, b"Some");
    push_node(
        ast,
        NodeKind::ExpressionCall,
        callee,
        absent(),
        0,
        0,
        &[node],
    )
}

fn unsupported(ast: &mut Ast) -> NodeId {
    let name = push_string(&mut ast.strings, b"()");
    push_node(ast, NodeKind::ExpressionLiteral, name, absent(), 0, 0, &[])
}

fn struct_named(tables: &Tables, kind: zag_facts::TypeId) -> Vec<u8> {
    string_bytes(&tables.strings, tables.types.name[kind.0 as usize]).to_vec()
}

fn assignment_for(
    tables: &Tables,
    index: &crate::index::Index,
    function: FunctionId,
    field: FieldId,
) -> Option<ExpressionId> {
    crate::index::assignments_of(index, field)
        .iter()
        .find(|row| tables.field_assignments.function.get(**row as usize) == Some(&function))
        .and_then(|row| {
            tables
                .field_assignments
                .expression
                .get(*row as usize)
                .copied()
        })
        .filter(|expression| expression.0 != NO_INDEX)
}

/// Why a struct got no constructor. Each one names the field that stopped it,
/// because a reader who has to write the body needs to know which.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Refusal {
    /// The struct has no `init` of its own to write a constructor from.
    NoInit,
    /// An `init` that sets no fields, which is not a constructor of anything.
    NoFields,
    NothingAssigns(FieldId),
    /// The Zig sets it to something outside what the port can spell.
    NotSpellable(FieldId),
    /// The analysis could not decide who owns it, so the port cannot say what
    /// the constructor would be handing over.
    OwnershipUnknown(FieldId),
}

/// The Zig `init` a constructor can be written from, which is the one that
/// belongs to the struct and sets every field to something writable. Both the
/// emitter and the report ask through here, so what one writes and what the
/// other explains cannot drift apart.
pub fn constructor_for(
    tables: &Tables,
    ownership: &Ownership,
    lowering: Lowering,
    owner: StructId,
) -> Result<FunctionId, Refusal> {
    let function = crate::index::init_of(lowering.index, owner).ok_or(Refusal::NoInit)?;
    let mut fields = 0;
    for row in struct_fields(&tables.structs, owner) {
        let field = FieldId(row as u32);
        let expression = assignment_for(tables, lowering.index, function, field)
            .ok_or(Refusal::NothingAssigns(field))?;
        if !is_writable(tables, expression) {
            return Err(Refusal::NotSpellable(field));
        }
        if ownership.class.get(row).copied() == Some(OwnershipClass::Unknown) {
            return Err(Refusal::OwnershipUnknown(field));
        }
        fields += 1;
    }
    if fields == 0 {
        return Err(Refusal::NoFields);
    }
    Ok(function)
}

pub fn writable_init(
    tables: &Tables,
    ownership: &Ownership,
    lowering: Lowering,
    owner: StructId,
) -> Option<FunctionId> {
    constructor_for(tables, ownership, lowering, owner).ok()
}

/// The Zig the port could not read, where the refusal was about an expression.
pub fn unspellable_text(
    tables: &Tables,
    ownership: &Ownership,
    lowering: Lowering,
    owner: StructId,
) -> Option<Vec<u8>> {
    let Err(Refusal::NotSpellable(field)) = constructor_for(tables, ownership, lowering, owner)
    else {
        return None;
    };
    let function = crate::index::init_of(lowering.index, owner)?;
    let expression = assignment_for(tables, lowering.index, function, field)?;
    let text = first_unspellable(tables, expression)?;
    let bytes = string_bytes(&tables.strings, text);
    (!bytes.is_empty()).then(|| bytes.to_vec())
}

/// Where the Zig the port could not read was written, or zero.
pub fn unspellable_line(
    tables: &Tables,
    ownership: &Ownership,
    lowering: Lowering,
    owner: StructId,
) -> u32 {
    let Err(Refusal::NotSpellable(field)) = constructor_for(tables, ownership, lowering, owner)
    else {
        return 0;
    };
    let Some(function) = crate::index::init_of(lowering.index, owner) else {
        return 0;
    };
    let Some(expression) = assignment_for(tables, lowering.index, function, field) else {
        return 0;
    };
    unspellable_row(tables, expression)
        .and_then(|row| tables.expressions.line.get(row).copied())
        .unwrap_or(0)
}

/// The innermost expression the port could not spell, which is the one whose
/// Zig is worth quoting rather than the wrapper around it.
fn unspellable_row(tables: &Tables, expression: ExpressionId) -> Option<usize> {
    let kind = tables
        .expressions
        .kind
        .get(expression.0 as usize)
        .copied()?;
    if kind == ExpressionKind::Unsupported {
        return Some(expression.0 as usize);
    }
    expression_children(tables, expression)
        .filter_map(|slot| tables.expressions.children.get(slot).copied())
        .find_map(|child| unspellable_row(tables, child))
}

fn first_unspellable(tables: &Tables, expression: ExpressionId) -> Option<StringId> {
    let kind = tables
        .expressions
        .kind
        .get(expression.0 as usize)
        .copied()?;
    if kind == ExpressionKind::Unsupported {
        return tables.expressions.text.get(expression.0 as usize).copied();
    }
    expression_children(tables, expression)
        .filter_map(|slot| tables.expressions.children.get(slot).copied())
        .find_map(|child| first_unspellable(tables, child))
}

/// The `new` a struct gets, written only when the Zig `init` belongs to it and
/// sets every one of its fields to something writable. The caller puts it in
/// the struct's `impl` block alongside whatever else the port writes there.
pub fn lower_constructor(
    ast: &mut Ast,
    tables: &Tables,
    ownership: &Ownership,
    lowering: Lowering,
    owner: StructId,
) -> Option<NodeId> {
    let function = writable_init(tables, ownership, lowering, owner)?;

    let mut values = Vec::new();
    for row in struct_fields(&tables.structs, owner) {
        let field = FieldId(row as u32);
        let expression = assignment_for(tables, lowering.index, function, field)?;
        let value = lower_value(ast, tables, ownership, function, field, expression);
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
        // A parameter range that runs off the end of the table is a corrupt
        // fact file, which is a row to drop rather than a reason to stop.
        let (Some(&flags), Some(&declared)) = (
            tables.parameters.flags.get(row),
            tables.parameters.parameter_type.get(row),
        ) else {
            continue;
        };
        if flags & PARAMETER_FLAG_ALLOCATOR != 0 {
            continue;
        }
        // A slice or a pointer comes across as a borrow with the lifetime
        // elided, and everything else comes across by value.
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
        let text = tables
            .parameters
            .name
            .get(row)
            .map(|name| string_bytes(&tables.strings, *name).to_vec())
            .unwrap_or_default();
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
    Some(push_node(
        ast,
        NodeKind::Function,
        name,
        absent(),
        count,
        0,
        &children,
    ))
}
