//! Names the error set of every function whose Zig left it to the compiler.
//!
//! `!T` is not the absence of an error set. Zig generates one from the body,
//! and Rust has to be handed a type, so working out what Zig would generate and
//! declaring it is the translation rather than an invention.
//!
//! Naming it is the port's decision rather than a fact about the Zig, so it
//! lives here and not in the tables. The tables say the function is fallible
//! and names no set, which is what the Zig says.
//!
//! A function that can fail through something the reading could not attribute
//! gets nothing. A set with a hole in it is worse than no set, because it reads
//! as settled and is not.

use std::collections::BTreeSet;
use zag_facts::tables::{
    ContainerKind, FUNCTION_FLAG_FALLIBLE, Tables, expression_children, fields_of, function_count,
    string_bytes, struct_count,
};
use zag_facts::{ExpressionId, FunctionId, ModuleId, NO_INDEX, StringId, StructId, TypeId};

/// Calls that fail only by running out of memory. Zig folds
/// `std.mem.Allocator.Error` into whatever it infers, and it is one variant, so
/// the port can name it without reading the standard library.
const ALLOCATION_FAILURE: [&str; 12] = [
    "alloc",
    "allocSentinel",
    "alignedAlloc",
    "create",
    "dupe",
    "dupeZ",
    "realloc",
    "append",
    "appendSlice",
    "toOwnedSlice",
    "ensureTotalCapacity",
    "ensureUnusedCapacity",
];

#[derive(Clone, Default, PartialEq, Eq)]
struct Failure {
    /// The declared error sets it can fail through, by struct row.
    sets: BTreeSet<u32>,
    allocates: bool,
    unreadable: bool,
}

fn absorb(into: &mut Failure, from: &Failure) {
    for set in &from.sets {
        into.sets.insert(*set);
    }
    into.allocates |= from.allocates;
    into.unreadable |= from.unreadable;
}

/// A name from a column that may be shorter than the row asking for it, which
/// is what a corrupt fact file looks like from in here.
fn name_at(tables: &Tables, column: &[StringId], row: usize) -> Vec<u8> {
    column
        .get(row)
        .map(|name| string_bytes(&tables.strings, *name).to_vec())
        .unwrap_or_default()
}

fn text_of(tables: &Tables, id: Option<StringId>) -> String {
    let Some(id) = id.filter(|id| id.0 != NO_INDEX) else {
        return String::new();
    };
    String::from_utf8_lossy(string_bytes(&tables.strings, id)).into_owned()
}

/// The error set a name like `Error::ZeroSized` belongs to.
fn error_set_named(tables: &Tables, text: &str) -> Option<u32> {
    let (base, _) = text.split_once("::")?;
    (0..struct_count(&tables.structs))
        .find(|row| {
            tables.structs.kind.get(*row) == Some(&ContainerKind::ErrorSet)
                && name_at(tables, &tables.structs.name, *row) == base.as_bytes()
        })
        .map(|row| row as u32)
}

/// Reads one body for everything it can fail through. `try` is the only way a
/// Zig expression propagates an error, so the tree says what the compiler would
/// infer, up to what it reaches through the standard library.
fn read_failures(
    tables: &Tables,
    expression: ExpressionId,
    depth: u32,
    found: &mut Failure,
    reaches: &mut Vec<FunctionId>,
) {
    if depth >= 32 {
        found.unreadable = true;
        return;
    }
    let index = expression.0 as usize;
    let Some(kind) = tables.expressions.kind.get(index).copied() else {
        return;
    };
    if kind == zag_facts::tables::ExpressionKind::Identifier {
        let text = text_of(tables, tables.expressions.text.get(index).copied());
        if let Some(set) = error_set_named(tables, &text) {
            found.sets.insert(set);
        }
    }
    // An expression the reading could not make sense of may hide a `try`, and
    // `try` is the only way an error leaves an expression. One without it
    // cannot propagate, so it says nothing about what the function fails with.
    if kind == zag_facts::tables::ExpressionKind::Unsupported {
        let text = text_of(tables, tables.expressions.text.get(index).copied());
        if text
            .split(|byte: char| !byte.is_alphanumeric() && byte != '_')
            .any(|word| word == "try")
        {
            found.unreadable = true;
        }
    }
    if kind == zag_facts::tables::ExpressionKind::Question {
        let attempted = expression_children(&tables.expressions, index)
            .next()
            .and_then(|slot| tables.expressions.children.get(slot).copied());
        match attempted.and_then(|child| {
            tables
                .expressions
                .kind
                .get(child.0 as usize)
                .copied()
                .map(|kind| (child, kind))
        }) {
            Some((child, zag_facts::tables::ExpressionKind::Call)) => {
                match tables
                    .expressions
                    .parameter
                    .get(child.0 as usize)
                    .copied()
                    .filter(|callee| *callee != NO_INDEX)
                {
                    Some(callee) => reaches.push(FunctionId(callee)),
                    None => found.unreadable = true,
                }
            }
            Some((child, zag_facts::tables::ExpressionKind::Method)) => {
                let name = text_of(
                    tables,
                    tables.expressions.text.get(child.0 as usize).copied(),
                );
                // A method on a type the program declares is a call like any
                // other, and the receiver's type is what says which one.
                match method_called(tables, child, &name) {
                    Some(callee) => reaches.push(callee),
                    None if ALLOCATION_FAILURE.contains(&name.as_str()) => found.allocates = true,
                    None => found.unreadable = true,
                }
            }
            _ => found.unreadable = true,
        }
    }
    for slot in expression_children(&tables.expressions, index) {
        if let Some(child) = tables.expressions.children.get(slot).copied() {
            read_failures(tables, child, depth + 1, found, reaches);
        }
    }
}

/// The function a method call reaches, found through the receiver's own type
/// rather than the spelling of the call.
fn method_called(tables: &Tables, call: ExpressionId, name: &str) -> Option<FunctionId> {
    let receiver = expression_children(&tables.expressions, call.0 as usize)
        .next()
        .and_then(|slot| tables.expressions.children.get(slot).copied())?;
    let kind = tables
        .expressions
        .result
        .get(receiver.0 as usize)
        .copied()
        .filter(|kind| kind.0 != NO_INDEX)?;
    let owner = struct_behind(tables, kind, 0)?;
    (0..function_count(&tables.functions))
        .find(|row| {
            tables.functions.owner.get(*row).copied() == Some(owner)
                && name_at(tables, &tables.functions.name, *row) == name.as_bytes()
        })
        .map(|row| FunctionId(row as u32))
}

/// The struct a type reaches, through whatever wrappers it was written behind.
fn struct_behind(tables: &Tables, kind: TypeId, depth: u32) -> Option<StructId> {
    if depth >= 8 {
        return None;
    }
    if tables.types.kind.get(kind.0 as usize) == Some(&zag_facts::tables::TypeKind::Struct) {
        return (0..struct_count(&tables.structs))
            .find(|row| tables.structs.type_id.get(*row).copied() == Some(kind))
            .map(|row| StructId(row as u32));
    }
    let element = tables.types.element.get(kind.0 as usize).copied()?;
    if element.0 == NO_INDEX || element == kind {
        return None;
    }
    struct_behind(tables, element, depth + 1)
}

/// The struct a function belongs to, so two functions of one name in one module
/// do not generate one error set between them.
fn owner_name(tables: &Tables, row: usize) -> Vec<u8> {
    match tables
        .functions
        .owner
        .get(row)
        .copied()
        .filter(|owner| owner.0 != NO_INDEX)
    {
        Some(owner) => name_at(tables, &tables.structs.name, owner.0 as usize),
        None => Vec::new(),
    }
}

/// What the port calls the error a function can fail with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failing {
    /// A set the Zig declared, which the function keeps the name of.
    Declared(StructId),
    /// A set Zig would have generated, which the port has to declare itself.
    Generated {
        name: Vec<u8>,
        module: ModuleId,
        variants: Vec<Vec<u8>>,
    },
}

/// One entry per function, empty where the tables already name its error set or
/// where nothing could be said about it.
pub type Failures = Vec<Option<Failing>>;

pub fn resolve_failures(tables: &Tables) -> Failures {
    let count = function_count(&tables.functions);
    let mut failures = vec![Failure::default(); count];
    let mut reaches: Vec<Vec<FunctionId>> = vec![Vec::new(); count];
    for row in 0..count {
        let body = tables
            .functions
            .body
            .get(row)
            .copied()
            .filter(|body| body.0 != NO_INDEX);
        if let Some(body) = body {
            read_failures(tables, body, 0, &mut failures[row], &mut reaches[row]);
        }
        // The call table says who reaches whom whether or not the body was
        // read. Zig only lets an error out through `try`, and a `catch` stops
        // one, so taking every edge can widen the set past what Zig would
        // infer. A set that is too wide costs a variant nobody constructs; one
        // that is too narrow fails to compile the first time somebody
        // propagates through it.
        for edge in 0..zag_facts::tables::call_count(&tables.calls) {
            if tables.calls.caller.get(edge).copied() == Some(FunctionId(row as u32))
                && let Some(callee) = tables.calls.callee.get(edge).copied()
                && !reaches[row].contains(&callee)
            {
                reaches[row].push(callee);
            }
        }
        // An allocation is a fact the tables carry whether or not the body was
        // read, and allocating is how a Zig function comes to fail with
        // `OutOfMemory` in the first place.
        if (0..zag_facts::tables::memory_operation_count(&tables.memory_operations)).any(|op| {
            tables.memory_operations.kind.get(op)
                == Some(&zag_facts::tables::MemoryOperationKind::Allocate)
                && tables.memory_operations.function.get(op).copied()
                    == Some(FunctionId(row as u32))
        }) {
            failures[row].allocates = true;
        }
        let declared = tables
            .functions
            .error_set
            .get(row)
            .copied()
            .filter(|set| set.0 != NO_INDEX);
        // A declared set is the answer already, and is what a caller absorbs
        // when it lets one through. A body that was never recorded says
        // nothing either way: what is recorded is what this works from.
        if let Some(set) = declared {
            failures[row].sets.insert(set.0);
        }
    }

    // A caller fails with whatever it lets through, so this settles from the
    // leaves up and takes at most as many rounds as the deepest chain.
    for _ in 0..count.min(64) {
        let mut changed = false;
        for (row, reached) in reaches.iter().enumerate() {
            let Some(mut carried) = failures.get(row).cloned() else {
                continue;
            };
            for callee in reached {
                if let Some(theirs) = failures.get(callee.0 as usize).cloned() {
                    absorb(&mut carried, &theirs);
                }
            }
            if failures.get(row) != Some(&carried) {
                failures[row] = carried;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let mut named: Failures = vec![None; count];
    // Two functions that fail the same way fail with the same type. Naming one
    // set per function would give each its own, and a call between them would
    // then not compile.
    let mut minted: Vec<(Vec<Vec<u8>>, Failing)> = Vec::new();
    for (row, failure) in failures.iter().enumerate() {
        let fallible = tables
            .functions
            .flags
            .get(row)
            .is_some_and(|flags| flags & FUNCTION_FLAG_FALLIBLE != 0);
        let declared = tables
            .functions
            .error_set
            .get(row)
            .copied()
            .is_some_and(|set| set.0 != NO_INDEX);
        if !fallible || declared {
            continue;
        }
        let mut variants: Vec<Vec<u8>> = Vec::new();
        for set in &failure.sets {
            for field in fields_of(tables, StructId(*set)) {
                // A range running past the end of the field table is a corrupt
                // fact file, and reading a variant per row it claims would mean
                // holding one per number it was handed.
                if field >= zag_facts::tables::field_count(&tables.fields) {
                    break;
                }
                let name = name_at(tables, &tables.fields.name, field);
                if !variants.contains(&name) {
                    variants.push(name);
                }
            }
        }
        if failure.allocates && !variants.iter().any(|name| name == b"OutOfMemory") {
            variants.push(b"OutOfMemory".to_vec());
        }
        if variants.is_empty() {
            continue;
        }
        // One declared set and nothing else is that set, so the port keeps the
        // name the Zig already wrote rather than making a second one for it.
        if failure.sets.len() == 1 && !failure.allocates {
            let only = StructId(*failure.sets.iter().next().expect("one set"));
            named[row] = Some(Failing::Declared(only));
            continue;
        }
        if let Some((_, already)) = minted.iter().find(|(seen, _)| *seen == variants) {
            named[row] = Some(already.clone());
            continue;
        }
        let mut text = crate::lower::pascal_case(&owner_name(tables, row));
        text.extend_from_slice(&crate::lower::pascal_case(&name_at(
            tables,
            &tables.functions.name,
            row,
        )));
        text.extend_from_slice(b"Error");
        let minting = Failing::Generated {
            name: text,
            module: tables
                .functions
                .module
                .get(row)
                .copied()
                .unwrap_or(zag_facts::tables::ROOT_MODULE),
            variants: variants.clone(),
        };
        minted.push((variants, minting.clone()));
        named[row] = Some(minting);
    }
    named
}
