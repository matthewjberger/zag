//! Writes a function body from the expression tree the frontend read.
//!
//! The rule is the same one the constructor follows: all of it or none of it.
//! One shape the port cannot spell anywhere in the body and nothing is written,
//! because a body with a hole in it compiles into something that looks finished
//! and is not.
//!
//! A Zig statement is an expression and so is a Rust one, so there is one tree
//! here rather than a statement list wrapping an expression list.

use crate::lower::absent;
use zag_analysis::ownership::{Ownership, OwnershipClass};
use zag_facts::build::push_string;
use zag_facts::tables::{ExpressionKind, Tables, expression_children, string_bytes};
use zag_facts::{ExpressionId, FunctionId, NO_INDEX, StructId, TypeId};
use zag_render::ast::{Ast, NodeId, NodeKind, push_node};

/// How deep an expression may nest before the port gives up. A tree this deep
/// is not something a person wants read back to them anyway.
const MAXIMUM_DEPTH: u32 = 32;

fn children_of(tables: &Tables, expression: ExpressionId) -> Vec<ExpressionId> {
    expression_children(&tables.expressions, expression.0 as usize)
        .filter_map(|slot| tables.expressions.children.get(slot).copied())
        .collect()
}

fn kind_of(tables: &Tables, expression: ExpressionId) -> Option<ExpressionKind> {
    tables.expressions.kind.get(expression.0 as usize).copied()
}

/// The text a row carries, borrowed. The gate reads one per node and the
/// comparisons need no copy, which is the whole difference from `text_of`.
fn text_bytes(tables: &Tables, expression: ExpressionId) -> &[u8] {
    tables
        .expressions
        .text
        .get(expression.0 as usize)
        .map(|text| string_bytes(&tables.strings, *text))
        .unwrap_or_default()
}

fn text_of(tables: &Tables, expression: ExpressionId) -> Vec<u8> {
    tables
        .expressions
        .text
        .get(expression.0 as usize)
        .map(|text| string_bytes(&tables.strings, *text).to_vec())
        .unwrap_or_default()
}

/// Whether every shape in the tree is one the port can write. Asked before
/// anything is written, so a body is never started and abandoned.
pub fn is_spellable(tables: &Tables, expression: ExpressionId, depth: u32) -> bool {
    if depth >= MAXIMUM_DEPTH {
        return false;
    }
    let Some(kind) = kind_of(tables, expression) else {
        return false;
    };
    // A body expression carries no resolved type, so these two need one the
    // reader does not have. `null` is `None` only once something says what it
    // is null of, and everything on the way out of the function would have to
    // be wrapped to match. `.len` is a length on a slice and a field access on
    // anything else, and Rust spells the first as a call.
    let text = text_bytes(tables, expression);
    if matches!(kind, ExpressionKind::Field) && text == b"len" {
        return false;
    }
    // `null` needs to know what it is null of, and everything on the way out
    // would have to be wrapped to match. `undefined` is uninitialised memory,
    // which Rust has no safe spelling for at all. The constructor already
    // refuses both, and a body is no different.
    if text == b"null" || text == b"undefined" {
        return false;
    }
    // The compiler's own modules are not ported, so anything reached through
    // one names something the port does not have.
    if matches!(kind, ExpressionKind::Identifier) && (text == b"std" || text == b"builtin") {
        return false;
    }
    // A conversion whose target the Zig did not write is one an `@as` around it
    // was meant to supply. Without one there is no type to convert to.
    if matches!(kind, ExpressionKind::Cast) && text.is_empty() {
        return false;
    }
    // Zig coerces a bare numeric literal to whatever the parameter is. Where
    // the callee resolved, the frontend has already widened the literal to the
    // parameter it lands on. A method call resolves no callee, so there is
    // nothing to read the intended type off and the body is left to a person.
    if matches!(kind, ExpressionKind::Method)
        && children_of(tables, expression).into_iter().any(|child| {
            kind_of(tables, child) == Some(ExpressionKind::Literal)
                && text_bytes(tables, child)
                    .iter()
                    .all(|byte| byte.is_ascii_digit())
        })
    {
        return false;
    }
    if !matches!(
        kind,
        ExpressionKind::Identifier
            | ExpressionKind::Literal
            | ExpressionKind::Field
            | ExpressionKind::Binary
            | ExpressionKind::Unary
            | ExpressionKind::Index
            | ExpressionKind::Branch
            | ExpressionKind::Block
            | ExpressionKind::Return
            | ExpressionKind::Let
            | ExpressionKind::Assign
            | ExpressionKind::Group
            | ExpressionKind::Question
            | ExpressionKind::Method
            | ExpressionKind::While
            | ExpressionKind::For
            | ExpressionKind::Call
            | ExpressionKind::Match
            | ExpressionKind::Arm
            | ExpressionKind::StructLiteral
            | ExpressionKind::Wrap
            | ExpressionKind::Cast
    ) {
        return false;
    }
    children_of(tables, expression)
        .into_iter()
        .all(|child| is_spellable(tables, child, depth + 1))
}

fn lower_expression(
    ast: &mut Ast,
    tables: &Tables,
    lowering: crate::lower::Lowering,
    expression: ExpressionId,
    depth: u32,
) -> Option<NodeId> {
    if depth >= MAXIMUM_DEPTH {
        return None;
    }
    let kind = kind_of(tables, expression)?;
    let children = children_of(tables, expression);
    let mut lowered = Vec::with_capacity(children.len());
    for child in &children {
        lowered.push(lower_expression(ast, tables, lowering, *child, depth + 1)?);
    }
    let text = text_of(tables, expression);
    let name = push_string(&mut ast.strings, &text);

    let node = match kind {
        ExpressionKind::Identifier => {
            push_node(ast, NodeKind::ExpressionPath, name, absent(), 0, 0, &[])
        }
        ExpressionKind::Literal => {
            push_node(ast, NodeKind::ExpressionLiteral, name, absent(), 0, 0, &[])
        }
        // The field it fills is on the row, so a member of a literal prints as
        // the field it names, and the literal itself prints as the struct the
        // signature said the function returns.
        ExpressionKind::StructLiteral => {
            let field = tables
                .expressions
                .field
                .get(expression.0 as usize)
                .copied()
                .unwrap_or(zag_facts::FieldId(zag_facts::NO_INDEX));
            if field.0 != zag_facts::NO_INDEX {
                let text = crate::lower::name_of(tables, tables.fields.name.get(field.0 as usize));
                let name = push_string(&mut ast.strings, &text);
                return Some(push_node(
                    ast,
                    NodeKind::FieldValue,
                    name,
                    absent(),
                    0,
                    0,
                    &lowered,
                ));
            }
            let result = tables
                .expressions
                .result
                .get(expression.0 as usize)
                .copied()
                .unwrap_or(zag_facts::TypeId(zag_facts::NO_INDEX));
            let text = crate::lower::name_of(tables, tables.types.name.get(result.0 as usize));
            if text.is_empty() {
                return None;
            }
            let text = crate::lower::qualify_name(tables, lowering, result, &text);
            let name = push_string(&mut ast.strings, &text);
            push_node(
                ast,
                NodeKind::ExpressionStruct,
                name,
                absent(),
                0,
                0,
                &lowered,
            )
        }
        // The type is on the row, because Zig writes it in the `@as` around the
        // conversion and Rust writes it after the value.
        ExpressionKind::Cast => {
            push_node(ast, NodeKind::ExpressionAs, name, absent(), 0, 0, &lowered)
        }
        ExpressionKind::Wrap => push_node(
            ast,
            NodeKind::ExpressionCall,
            name,
            absent(),
            0,
            0,
            &lowered,
        ),
        ExpressionKind::Field => push_node(
            ast,
            NodeKind::ExpressionField,
            name,
            absent(),
            0,
            0,
            &lowered,
        ),
        ExpressionKind::Binary => push_node(
            ast,
            NodeKind::ExpressionBinary,
            name,
            absent(),
            0,
            0,
            &lowered,
        ),
        ExpressionKind::Unary => push_node(
            ast,
            NodeKind::ExpressionUnary,
            name,
            absent(),
            0,
            0,
            &lowered,
        ),
        // Rust indexes with `usize` and Zig indexes with any integer, so the
        // cast is part of the translation rather than a widening of it.
        ExpressionKind::Index => {
            let mut children = lowered.clone();
            if let Some(subscript) = children.last_mut() {
                let cast = push_string(&mut ast.strings, b"usize");
                *subscript = push_node(
                    ast,
                    NodeKind::ExpressionAs,
                    cast,
                    absent(),
                    0,
                    0,
                    &[*subscript],
                );
            }
            push_node(
                ast,
                NodeKind::ExpressionIndex,
                absent(),
                absent(),
                0,
                0,
                &children,
            )
        }
        ExpressionKind::Group => push_node(
            ast,
            NodeKind::ExpressionGroup,
            absent(),
            absent(),
            0,
            0,
            &lowered,
        ),
        ExpressionKind::Question => push_node(
            ast,
            NodeKind::ExpressionQuestion,
            absent(),
            absent(),
            0,
            0,
            &lowered,
        ),
        ExpressionKind::Return => push_node(
            ast,
            NodeKind::ExpressionReturn,
            absent(),
            absent(),
            0,
            0,
            &lowered,
        ),
        ExpressionKind::Let => {
            let mutable = tables
                .expressions
                .parameter
                .get(expression.0 as usize)
                .copied()
                == Some(1);
            // A Zig local may carry the type Rust would otherwise take from
            // whatever is assigned to it next, so the annotation comes across.
            let mut children = lowered.clone();
            if let Some(kind) = tables
                .expressions
                .result
                .get(expression.0 as usize)
                .copied()
                .filter(|kind| kind.0 != NO_INDEX)
            {
                children.push(crate::lower::lower_type_body(
                    ast, tables, lowering, kind, 0,
                ));
            }
            push_node(
                ast,
                NodeKind::ExpressionLet,
                name,
                absent(),
                0,
                u32::from(mutable),
                &children,
            )
        }
        // `+=` and its neighbours mean the same in both languages, so the
        // operator the row carries is the one written out.
        ExpressionKind::Assign => push_node(
            ast,
            NodeKind::ExpressionAssign,
            name,
            absent(),
            0,
            0,
            &lowered,
        ),
        ExpressionKind::Branch => push_node(
            ast,
            NodeKind::ExpressionBranch,
            absent(),
            absent(),
            0,
            0,
            &lowered,
        ),
        // Zig spells a method in camel case and Rust spells it in snake case,
        // the same as the declaration the call reaches.
        ExpressionKind::Method => {
            let called = push_string(
                &mut ast.strings,
                &crate::function::snake_case(&text_of(tables, expression)),
            );
            push_node(
                ast,
                NodeKind::ExpressionMethod,
                called,
                absent(),
                0,
                0,
                &lowered,
            )
        }
        ExpressionKind::While => push_node(
            ast,
            NodeKind::ExpressionWhile,
            absent(),
            absent(),
            0,
            0,
            &lowered,
        ),
        ExpressionKind::For => {
            push_node(ast, NodeKind::ExpressionFor, name, absent(), 0, 0, &lowered)
        }
        ExpressionKind::Match => push_node(
            ast,
            NodeKind::ExpressionMatch,
            absent(),
            absent(),
            0,
            0,
            &lowered,
        ),
        ExpressionKind::Arm => {
            push_node(ast, NodeKind::ExpressionArm, name, absent(), 0, 0, &lowered)
        }
        ExpressionKind::Call => {
            let callee = tables
                .expressions
                .parameter
                .get(expression.0 as usize)
                .copied()
                .filter(|callee| *callee != NO_INDEX)?;
            let path = call_path(ast, tables, lowering, FunctionId(callee))?;
            push_node(
                ast,
                NodeKind::ExpressionCall,
                path,
                absent(),
                0,
                0,
                &lowered,
            )
        }
        ExpressionKind::Block => {
            let statements = lower_statements(ast, tables, &children, &lowered, false);
            push_node(
                ast,
                NodeKind::ExpressionBlock,
                absent(),
                absent(),
                0,
                0,
                &statements,
            )
        }
        _ => return None,
    };
    Some(node)
}

/// How a call from here reaches the function it names. The same relative
/// spelling the type paths use, so a port stays correct when it moves.
fn call_path(
    ast: &mut Ast,
    tables: &Tables,
    lowering: crate::lower::Lowering,
    callee: FunctionId,
) -> Option<zag_facts::StringId> {
    let index = callee.0 as usize;
    let name = tables.functions.name.get(index).copied()?;
    let name = crate::function::snake_case(string_bytes(&tables.strings, name));
    let owner = tables
        .functions
        .module
        .get(index)
        .copied()
        .unwrap_or(zag_facts::tables::ROOT_MODULE);
    let mut path = Vec::new();
    if lowering.qualified && owner != lowering.module {
        let module = tables
            .modules
            .name
            .get(owner.0 as usize)
            .map(|text| string_bytes(&tables.strings, *text))
            .unwrap_or(b"");
        if module.is_empty() {
            // The root has no name to reach it by from inside a submodule.
            if lowering.module != zag_facts::tables::ROOT_MODULE {
                return None;
            }
        } else {
            if lowering.module != zag_facts::tables::ROOT_MODULE {
                path.extend_from_slice(b"super::");
            }
            path.extend_from_slice(module);
            path.extend_from_slice(b"::");
        }
    }
    path.extend_from_slice(&name);
    Some(push_string(&mut ast.strings, &path))
}

/// Whether the shape produces a value. A block ends in the value it has, and
/// something that has none has to stay a statement wherever it sits.
fn produces_a_value(kind: Option<ExpressionKind>) -> bool {
    !matches!(
        kind,
        Some(ExpressionKind::Let)
            | Some(ExpressionKind::Assign)
            | Some(ExpressionKind::While)
            | Some(ExpressionKind::For)
    )
}

/// Whether Rust writes it with braces, in which case no semicolon follows it.
fn is_braced(kind: Option<ExpressionKind>) -> bool {
    matches!(
        kind,
        Some(ExpressionKind::While)
            | Some(ExpressionKind::For)
            | Some(ExpressionKind::Block)
            | Some(ExpressionKind::Branch)
    )
}

/// Wraps everything but the last statement so it ends in a semicolon. The last
/// one is the value the block has, which is how Rust says what a Zig `return`
/// at the end of a body said, unless it is something with no value to give.
/// `tail` is whether this block is the last thing the function does. Only there
/// does a trailing `return x;` mean the same as `x`. Inside an `if` it does
/// not: dropping the keyword leaves the value in a block that falls through to
/// whatever comes after it.
fn lower_statements(
    ast: &mut Ast,
    tables: &Tables,
    original: &[ExpressionId],
    lowered: &[NodeId],
    tail: bool,
) -> Vec<NodeId> {
    let mut statements = Vec::with_capacity(lowered.len());
    for (position, node) in lowered.iter().enumerate() {
        let kind = original
            .get(position)
            .and_then(|expression| kind_of(tables, *expression));
        let last = tail && position + 1 == lowered.len() && produces_a_value(kind);
        let returning = kind == Some(ExpressionKind::Return);
        if is_braced(kind) {
            // A braced statement carries its own end, so a semicolon after it
            // is one the linter asks to be taken away again.
            statements.push(*node);
            continue;
        }
        if last && returning {
            // `return x;` as the last thing a function does is what Rust
            // writes as a trailing expression, and the linter says so.
            let value = zag_render::ast::node_children(ast, *node).next();
            match value.map(|slot| ast.children[slot]) {
                Some(value) => statements.push(value),
                None => statements.push(*node),
            }
            continue;
        }
        if last {
            statements.push(*node);
            continue;
        }
        statements.push(push_node(
            ast,
            NodeKind::Statement,
            absent(),
            absent(),
            0,
            0,
            &[*node],
        ));
    }
    statements
}

/// The statements a function body is made of, or nothing where any part of it
/// is a shape the port cannot spell.
/// The struct a parameter reaches, through whatever wrappers it was declared
/// behind. A body is written against the types in its signature, so this is
/// what those types are.
fn struct_behind(tables: &Tables, kind: TypeId, depth: u32) -> Option<StructId> {
    if depth >= 8 {
        return None;
    }
    let index = kind.0 as usize;
    if tables.types.kind.get(index) == Some(&zag_facts::tables::TypeKind::Struct) {
        return (0..zag_facts::tables::struct_count(&tables.structs))
            .find(|row| tables.structs.type_id.get(*row).copied() == Some(kind))
            .map(|row| StructId(row as u32));
    }
    let element = tables.types.element.get(index).copied()?;
    if element.0 == NO_INDEX || element == kind {
        return None;
    }
    struct_behind(tables, element, depth + 1)
}

/// Whether the signature names a struct the analysis could not finish. A field
/// left `unknown` comes across as a raw pointer, and a body that reads one is
/// written against a type that says nothing about what it points at, so there
/// is nothing to write it against yet.
pub fn signature_is_settled(tables: &Tables, ownership: &Ownership, function: FunctionId) -> bool {
    zag_facts::tables::parameters_of(tables, function).all(|row| {
        let Some(declared) = tables.parameters.parameter_type.get(row).copied() else {
            return true;
        };
        let Some(owner) = struct_behind(tables, declared, 0) else {
            return true;
        };
        zag_facts::tables::fields_of(tables, owner)
            .all(|field| ownership.class.get(field) != Some(&OwnershipClass::Unknown))
    })
}

/// Whether the body names the allocator it was handed. The port allocates
/// through the types themselves, so the parameter disappears from the
/// signature, and a body that reads it is written against an argument that is
/// no longer there.
pub fn reads_the_allocator(
    tables: &Tables,
    function: FunctionId,
    expression: ExpressionId,
    depth: u32,
) -> bool {
    if depth >= MAXIMUM_DEPTH {
        return false;
    }
    if kind_of(tables, expression) == Some(ExpressionKind::Identifier) {
        let text = text_bytes(tables, expression);
        let named = zag_facts::tables::parameters_of(tables, function).any(|row| {
            tables
                .parameters
                .flags
                .get(row)
                .is_some_and(|flags| flags & zag_facts::tables::PARAMETER_FLAG_ALLOCATOR != 0)
                && tables
                    .parameters
                    .name
                    .get(row)
                    .is_some_and(|name| string_bytes(&tables.strings, *name) == text)
        });
        if named {
            return true;
        }
    }
    children_of(tables, expression)
        .into_iter()
        .any(|child| reads_the_allocator(tables, function, child, depth + 1))
}

/// Whether the body moves an owned field out of the reference it was reached
/// through. Zig copies a slice out of a struct; Rust moves a `Box` or a `Vec`,
/// which is a swap the port cannot invent.
pub fn moves_an_owned_field(
    tables: &Tables,
    ownership: &Ownership,
    expression: ExpressionId,
    depth: u32,
) -> bool {
    if depth >= MAXIMUM_DEPTH {
        return false;
    }
    let taken = matches!(
        kind_of(tables, expression),
        Some(ExpressionKind::Let) | Some(ExpressionKind::Assign)
    );
    if taken
        && let Some(value) = children_of(tables, expression).last().copied()
        && kind_of(tables, value) == Some(ExpressionKind::Field)
        && let Some(field) = tables.expressions.field.get(value.0 as usize).copied()
        && field.0 != NO_INDEX
        && matches!(
            ownership.class.get(field.0 as usize),
            Some(OwnershipClass::Owned) | Some(OwnershipClass::Grown)
        )
    {
        return true;
    }
    children_of(tables, expression)
        .into_iter()
        .any(|child| moves_an_owned_field(tables, ownership, child, depth + 1))
}

pub fn lower_body(
    ast: &mut Ast,
    tables: &Tables,
    ownership: &Ownership,
    lowering: crate::lower::Lowering,
    function: FunctionId,
) -> Option<Vec<NodeId>> {
    if !signature_is_settled(tables, ownership, function) {
        return None;
    }
    let body = tables
        .functions
        .body
        .get(function.0 as usize)
        .copied()
        .filter(|body| body.0 != NO_INDEX)?;
    if kind_of(tables, body) != Some(ExpressionKind::Block) {
        return None;
    }
    if !is_spellable(tables, body, 0)
        || reads_the_allocator(tables, function, body, 0)
        || moves_an_owned_field(tables, ownership, body, 0)
    {
        return None;
    }
    let children = children_of(tables, body);
    if children.is_empty() {
        return None;
    }
    let mut lowered = Vec::with_capacity(children.len());
    for child in &children {
        lowered.push(lower_expression(ast, tables, lowering, *child, 1)?);
    }
    let mut statements = lower_statements(ast, tables, &children, &lowered, true);
    // A fallible function that runs off the end returns nothing in Zig and has
    // to say so in Rust.
    let fallible = tables
        .functions
        .flags
        .get(function.0 as usize)
        .is_some_and(|flags| flags & zag_facts::tables::FUNCTION_FLAG_FALLIBLE != 0);
    let returns = children
        .last()
        .and_then(|last| kind_of(tables, *last))
        .is_some_and(|kind| kind == ExpressionKind::Return);
    if fallible && !returns {
        let unit = push_string(&mut ast.strings, b"Ok(())");
        statements.push(push_node(
            ast,
            NodeKind::ExpressionLiteral,
            unit,
            absent(),
            0,
            0,
            &[],
        ));
    }
    Some(statements)
}
