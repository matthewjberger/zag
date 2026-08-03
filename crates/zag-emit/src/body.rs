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
use zag_facts::build::push_string;
use zag_facts::tables::{ExpressionKind, Tables, expression_children, string_bytes};
use zag_facts::{ExpressionId, FunctionId, NO_INDEX};
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
        lowered.push(lower_expression(ast, tables, *child, depth + 1)?);
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
        ExpressionKind::Index => push_node(
            ast,
            NodeKind::ExpressionIndex,
            absent(),
            absent(),
            0,
            0,
            &lowered,
        ),
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
            push_node(ast, NodeKind::ExpressionLet, name, absent(), 0, 0, &lowered)
        }
        ExpressionKind::Assign => push_node(
            ast,
            NodeKind::ExpressionAssign,
            absent(),
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
        ExpressionKind::Block => {
            let statements = lower_statements(ast, tables, &children, &lowered);
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

/// Wraps everything but the last statement so it ends in a semicolon. The last
/// one is the value the block has, which is how Rust says what a Zig `return`
/// at the end of a body said.
fn lower_statements(
    ast: &mut Ast,
    tables: &Tables,
    original: &[ExpressionId],
    lowered: &[NodeId],
) -> Vec<NodeId> {
    let mut statements = Vec::with_capacity(lowered.len());
    for (position, node) in lowered.iter().enumerate() {
        let last = position + 1 == lowered.len();
        let returning = original
            .get(position)
            .and_then(|expression| kind_of(tables, *expression))
            == Some(ExpressionKind::Return);
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
pub fn lower_body(ast: &mut Ast, tables: &Tables, function: FunctionId) -> Option<Vec<NodeId>> {
    let body = tables
        .functions
        .body
        .get(function.0 as usize)
        .copied()
        .filter(|body| body.0 != NO_INDEX)?;
    if kind_of(tables, body) != Some(ExpressionKind::Block) {
        return None;
    }
    if !is_spellable(tables, body, 0) {
        return None;
    }
    let children = children_of(tables, body);
    if children.is_empty() {
        return None;
    }
    let mut lowered = Vec::with_capacity(children.len());
    for child in &children {
        lowered.push(lower_expression(ast, tables, *child, 1)?);
    }
    Some(lower_statements(ast, tables, &children, &lowered))
}
