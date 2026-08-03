//! Writes the signature of a Zig function the port keeps, with the body left
//! to a person. A signature is worth writing on its own, because it carries the
//! parameter ownership, the return type, and the error set, and those are the
//! decisions a person porting the body would otherwise have to make first.
//!
//! Nothing is guessed. A function whose return type did not resolve, or which
//! can fail through an error set the Zig never named, gets nothing at all.

use crate::lower::{Lowering, absent, lower_field_type, lower_type_body, name_of};
use zag_analysis::ownership::{Ownership, OwnershipClass};
use zag_facts::build::push_string;
use zag_facts::tables::{
    FUNCTION_FLAG_FALLIBLE, PARAMETER_FLAG_ALLOCATOR, PARAMETER_FLAG_MUTABLE, Tables,
    function_parameters, is_reference_type, string_bytes,
};
use zag_facts::{FunctionId, NO_INDEX, StructId, TypeId};
use zag_render::ast::{
    Ast, Lifetime, NodeId, NodeKind, PARAMETER_FLAG_RECEIVER, STRUCT_FLAG_ARENA_LIFETIME,
    STRUCT_FLAG_BORROW_LIFETIME, push_node,
};

/// Zig spells a function in camel case and Rust spells it in snake case.
pub fn snake_case(name: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(name.len() + 4);
    for (position, byte) in name.iter().enumerate() {
        if byte.is_ascii_uppercase() {
            if position != 0 && out.last() != Some(&b'_') {
                out.push(b'_');
            }
            out.push(byte.to_ascii_lowercase());
            continue;
        }
        out.push(*byte);
    }
    out
}

fn receiver_text(tables: &Tables, row: usize) -> &'static [u8] {
    let declared = tables
        .parameters
        .parameter_type
        .get(row)
        .copied()
        .unwrap_or(TypeId(NO_INDEX));
    let mutable = tables
        .parameters
        .flags
        .get(row)
        .is_some_and(|flags| flags & PARAMETER_FLAG_MUTABLE != 0);
    let points_at_something = tables
        .types
        .kind
        .get(declared.0 as usize)
        .is_some_and(|kind| *kind == zag_facts::tables::TypeKind::Pointer);
    if !points_at_something {
        return b"self";
    }
    if mutable { b"&mut self" } else { b"&self" }
}

/// The receiver row, where the function has one. A method's first parameter is
/// the struct it belongs to, which Rust spells as part of the signature rather
/// than as a parameter.
fn receiver_row(tables: &Tables, function: FunctionId) -> Option<usize> {
    let row = function_parameters(&tables.functions, function).next()?;
    let owner = tables.functions.owner.get(function.0 as usize).copied()?;
    if owner.0 == NO_INDEX {
        return None;
    }
    (string_bytes(&tables.strings, *tables.parameters.name.get(row)?) == b"self").then_some(row)
}

/// Every lifetime the return type mentions, however deeply. A slice of a
/// borrowing struct carries that struct's lifetimes as much as the struct does.
fn lifetimes_in(tables: &Tables, lowering: Lowering, kind: TypeId, depth: u32) -> u32 {
    if depth >= 8 {
        return 0;
    }
    let index = kind.0 as usize;
    let mut carried = lowering.lifetimes.get(index).copied().unwrap_or(0);
    if let Some(element) = tables.types.element.get(index).copied() {
        carried |= lifetimes_in(tables, lowering, element, depth + 1);
    }
    carried
}

/// The lifetimes the function has to declare for itself, which are the ones its
/// return type mentions and its `impl` block does not already supply.
fn lifetimes_to_declare(
    tables: &Tables,
    lowering: Lowering,
    ownership: &Ownership,
    function: FunctionId,
) -> u32 {
    let returns = tables
        .functions
        .returns
        .get(function.0 as usize)
        .copied()
        .unwrap_or(TypeId(NO_INDEX));
    let carried = lifetimes_in(tables, lowering, returns, 0);
    let in_scope = owner_of(tables, function)
        .map(|owner| crate::lower::lifetimes_of(tables, ownership, owner))
        .unwrap_or(0);
    carried & !in_scope
}

fn has_reference_parameter(tables: &Tables, function: FunctionId) -> bool {
    function_parameters(&tables.functions, function).any(|row| {
        tables
            .parameters
            .flags
            .get(row)
            .is_some_and(|flags| flags & PARAMETER_FLAG_ALLOCATOR == 0)
            && tables
                .parameters
                .parameter_type
                .get(row)
                .is_some_and(|kind| is_reference_type(&tables.types, *kind))
    })
}

fn lower_parameter(
    ast: &mut Ast,
    tables: &Tables,
    lowering: Lowering,
    row: usize,
    receiver: Option<usize>,
    borrow: Lifetime,
) -> NodeId {
    if receiver == Some(row) {
        let name = push_string(&mut ast.strings, receiver_text(tables, row));
        return push_node(
            ast,
            NodeKind::Parameter,
            name,
            absent(),
            0,
            PARAMETER_FLAG_RECEIVER,
            &[],
        );
    }
    let declared = tables
        .parameters
        .parameter_type
        .get(row)
        .copied()
        .unwrap_or(TypeId(NO_INDEX));
    // A slice or a pointer comes across as a borrow with the lifetime elided,
    // and everything else comes across by value. The same rule the constructor
    // uses, because it is the same question.
    let kind = if is_reference_type(&tables.types, declared) {
        let body = lower_type_body(ast, tables, lowering, declared, 0);
        push_node(
            ast,
            NodeKind::TypeReference,
            absent(),
            absent(),
            0,
            borrow as u32,
            &[body],
        )
    } else {
        lower_field_type(ast, tables, lowering, declared, OwnershipClass::Value)
    };
    let text = string_bytes(&tables.strings, tables.parameters.name[row]).to_vec();
    let name = push_string(&mut ast.strings, &text);
    push_node(ast, NodeKind::Parameter, name, absent(), 0, 0, &[kind])
}

fn error_set_name(tables: &Tables, function: FunctionId) -> Option<Vec<u8>> {
    let set = tables
        .functions
        .error_set
        .get(function.0 as usize)
        .copied()
        .filter(|set| set.0 != NO_INDEX)?;
    let text = name_of(tables, tables.structs.name.get(set.0 as usize));
    (!text.is_empty()).then_some(text)
}

/// Why a function got no signature. Each one is a different thing to go and
/// fix, so they are kept apart rather than collapsed into one message.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Refusal {
    /// The frontend could not resolve what the function returns.
    ReturnTypeUnresolved,
    /// A `!T` infers its error set from the body, so the Zig named no error
    /// type and the port will not invent one.
    UnnamedErrorSet,
    /// What it returns borrows from an arena, and the allocator that owned
    /// that arena does not survive the port as a parameter.
    ReturnBorrowsAnArena,
    /// What it returns borrows, and nothing in the signature can carry the
    /// lifetime that borrow needs.
    ReturnBorrowsWithNothingToTieItTo,
}

/// Whether the port can spell what the function gives back, and where it
/// cannot, which of the reasons applies.
pub fn signature_refusal(
    tables: &Tables,
    ownership: &Ownership,
    lowering: Lowering,
    function: FunctionId,
) -> Option<Refusal> {
    let index = function.0 as usize;
    let Some(returns) = tables.functions.returns.get(index).copied() else {
        return Some(Refusal::ReturnTypeUnresolved);
    };
    if tables.types.kind.get(returns.0 as usize).is_none() {
        return Some(Refusal::ReturnTypeUnresolved);
    }
    let fallible = tables
        .functions
        .flags
        .get(index)
        .is_some_and(|flags| flags & FUNCTION_FLAG_FALLIBLE != 0);
    if fallible && error_set_name(tables, function).is_none() {
        return Some(Refusal::UnnamedErrorSet);
    }
    let declared = lifetimes_to_declare(tables, lowering, ownership, function);
    if declared & STRUCT_FLAG_ARENA_LIFETIME != 0 {
        return Some(Refusal::ReturnBorrowsAnArena);
    }
    if declared & STRUCT_FLAG_BORROW_LIFETIME != 0
        && (receiver_row(tables, function).is_some() || !has_reference_parameter(tables, function))
    {
        return Some(Refusal::ReturnBorrowsWithNothingToTieItTo);
    }
    None
}

fn lower_return_type(
    ast: &mut Ast,
    tables: &Tables,
    lowering: Lowering,
    function: FunctionId,
) -> NodeId {
    let returns = tables
        .functions
        .returns
        .get(function.0 as usize)
        .copied()
        .unwrap_or(TypeId(NO_INDEX));
    let body = lower_type_body(ast, tables, lowering, returns, 0);
    let Some(text) = error_set_name(tables, function) else {
        return body;
    };
    let name = push_string(&mut ast.strings, &text);
    let failure = push_node(ast, NodeKind::TypePath, name, absent(), 0, 0, &[]);
    push_node(
        ast,
        NodeKind::TypeResult,
        absent(),
        absent(),
        0,
        0,
        &[body, failure],
    )
}

/// The signature, with `todo!()` where the Zig body was. The body is the one
/// part of a function this cannot decide, and a marker that fails to compile
/// when reached is a better answer than a guess that does not.
pub fn lower_signature(
    ast: &mut Ast,
    tables: &Tables,
    ownership: &Ownership,
    lowering: Lowering,
    function: FunctionId,
) -> Option<NodeId> {
    if signature_refusal(tables, ownership, lowering, function).is_some() {
        return None;
    }
    let receiver = receiver_row(tables, function);
    // A lifetime the signature introduces is tied to its own reference
    // parameters, which is the only thing in the signature it can be tied to.
    let declared = lifetimes_to_declare(tables, lowering, ownership, function);
    let borrows = declared & STRUCT_FLAG_BORROW_LIFETIME != 0;
    let borrow = if borrows {
        Lifetime::Borrow
    } else {
        Lifetime::Elided
    };
    let mut children = Vec::new();
    let mut count = 0;
    for row in function_parameters(&tables.functions, function) {
        // The allocator disappears, because the port allocates through the
        // types themselves rather than through a handle passed in.
        if receiver != Some(row)
            && tables
                .parameters
                .flags
                .get(row)
                .is_some_and(|flags| flags & PARAMETER_FLAG_ALLOCATOR != 0)
        {
            continue;
        }
        children.push(lower_parameter(
            ast, tables, lowering, row, receiver, borrow,
        ));
        count += 1;
    }
    children.push(lower_return_type(ast, tables, lowering, function));
    // An unwritten body uses none of what it was handed, so the port discards
    // each argument the way a person writing the same stub would. Each line
    // goes away as soon as the body starts using its argument.
    for slot in 0..count as usize {
        let parameter = children[slot];
        if ast.flags[parameter.0 as usize] & PARAMETER_FLAG_RECEIVER != 0 {
            continue;
        }
        let text = string_bytes(&ast.strings, ast.text[parameter.0 as usize]).to_vec();
        let name = push_string(&mut ast.strings, &text);
        children.push(push_node(ast, NodeKind::Discard, name, absent(), 0, 0, &[]));
    }
    let body = push_string(&mut ast.strings, b"todo!()");
    children.push(push_node(
        ast,
        NodeKind::ExpressionLiteral,
        body,
        absent(),
        0,
        0,
        &[],
    ));
    let text = snake_case(&name_of(
        tables,
        tables.functions.name.get(function.0 as usize),
    ));
    let name = push_string(&mut ast.strings, &text);
    Some(push_node(
        ast,
        NodeKind::Function,
        name,
        absent(),
        count,
        declared,
        &children,
    ))
}

/// Whether the function belongs to a struct, which decides whether its
/// signature goes inside that struct's `impl` block or stands on its own.
pub fn owner_of(tables: &Tables, function: FunctionId) -> Option<StructId> {
    tables
        .functions
        .owner
        .get(function.0 as usize)
        .copied()
        .filter(|owner| owner.0 != NO_INDEX)
}

/// Signatures the port writes for one struct in one module, in declaration
/// order. Anything the report says is already ported or disappears is not
/// among them.
pub fn signatures_for(
    ast: &mut Ast,
    tables: &Tables,
    ownership: &Ownership,
    lowering: Lowering,
    owner: Option<StructId>,
) -> Vec<NodeId> {
    let mut written = Vec::new();
    // Only the functions that could belong here, rather than every function in
    // the program filtered down, which would be quadratic once a program has
    // as many structs as functions.
    let candidates = match owner {
        Some(owner) => crate::index::methods_of(lowering.index, owner),
        None => crate::index::free_functions_of(lowering.index, lowering.module),
    };
    for row in candidates {
        let index = *row as usize;
        let function = FunctionId(*row);
        if tables.functions.module.get(index).copied() != Some(lowering.module) {
            continue;
        }
        if owner_of(tables, function) != owner {
            continue;
        }
        if crate::report::disposition(tables, ownership, lowering, function)
            != crate::report::Disposition::Signature
        {
            continue;
        }
        if let Some(node) = lower_signature(ast, tables, ownership, lowering, function) {
            written.push(node);
        }
    }
    written
}
