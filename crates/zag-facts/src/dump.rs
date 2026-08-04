//! Prints a fact database as text, one row per line.
//!
//! The wire format is columns of little-endian `u32` and says nothing to a
//! reader. This is the same data in the order the tables hold it, so a row is
//! greppable and two dumps of the same tables are byte identical.
//!
//! Nothing here interprets. A handle prints as the name it resolves to where
//! there is one and as its number where there is not, because a dangling
//! handle is exactly what a reader is looking for when they reach for this.

use crate::handles::{FunctionId, ModuleId, NO_INDEX, StringId, StructId, TypeId};
use crate::tables::{
    AllocatorSourceKind, ArtifactKind, AssignmentSource, ContainerKind, ExpressionKind,
    MemoryOperationKind, PlaceKind, Tables, TypeKind, artifact_count, call_count, field_count,
    function_count, memory_operation_count, module_count, string_bytes, struct_count, type_count,
};

fn text(tables: &Tables, id: StringId) -> String {
    if id.0 == NO_INDEX {
        return "-".to_string();
    }
    String::from_utf8_lossy(string_bytes(&tables.strings, id)).into_owned()
}

fn handle(value: u32) -> String {
    if value == NO_INDEX {
        "-".to_string()
    } else {
        value.to_string()
    }
}

fn struct_name(tables: &Tables, owner: StructId) -> String {
    match tables.structs.name.get(owner.0 as usize) {
        Some(name) => text(tables, *name),
        None => handle(owner.0),
    }
}

fn function_name(tables: &Tables, function: FunctionId) -> String {
    match tables.functions.name.get(function.0 as usize) {
        Some(name) => text(tables, *name),
        None => handle(function.0),
    }
}

fn module_name(tables: &Tables, module: ModuleId) -> String {
    match tables.modules.name.get(module.0 as usize) {
        Some(name) if name.0 != NO_INDEX && !string_bytes(&tables.strings, *name).is_empty() => {
            text(tables, *name)
        }
        Some(_) => "root".to_string(),
        None => handle(module.0),
    }
}

fn artifact_kind(kind: ArtifactKind) -> &'static str {
    match kind {
        ArtifactKind::Executable => "executable",
        ArtifactKind::Library => "library",
        ArtifactKind::Test => "test",
    }
}

fn type_kind(kind: TypeKind) -> &'static str {
    match kind {
        TypeKind::Void => "void",
        TypeKind::Integer => "integer",
        TypeKind::Bool => "bool",
        TypeKind::Slice => "slice",
        TypeKind::Pointer => "pointer",
        TypeKind::Struct => "struct",
        TypeKind::Opaque => "opaque",
        TypeKind::Optional => "optional",
        TypeKind::Array => "array",
        TypeKind::Float => "float",
    }
}

fn container_kind(kind: ContainerKind) -> &'static str {
    match kind {
        ContainerKind::Struct => "struct",
        ContainerKind::Enum => "enum",
        ContainerKind::Union => "union",
        ContainerKind::ErrorSet => "error",
    }
}

fn allocator_kind(kind: AllocatorSourceKind) -> &'static str {
    match kind {
        AllocatorSourceKind::Global => "global",
        AllocatorSourceKind::Arena => "arena",
        AllocatorSourceKind::Parameter => "parameter",
        AllocatorSourceKind::Unknown => "unknown",
    }
}

fn memory_kind(kind: MemoryOperationKind) -> &'static str {
    match kind {
        MemoryOperationKind::Allocate => "allocate",
        MemoryOperationKind::Free => "free",
        MemoryOperationKind::Resize => "resize",
    }
}

fn place_kind(kind: PlaceKind) -> &'static str {
    match kind {
        PlaceKind::FieldOfParameter => "field",
        PlaceKind::Local => "local",
        PlaceKind::Unknown => "unknown",
    }
}

fn assignment_source(source: AssignmentSource) -> &'static str {
    match source {
        AssignmentSource::Allocation => "allocation",
        AssignmentSource::Parameter => "parameter",
        AssignmentSource::StaticLiteral => "literal",
        AssignmentSource::Unknown => "unknown",
    }
}

fn expression_kind(kind: ExpressionKind) -> &'static str {
    match kind {
        ExpressionKind::Literal => "literal",
        ExpressionKind::Parameter => "parameter",
        ExpressionKind::Length => "length",
        ExpressionKind::Cast => "cast",
        ExpressionKind::Allocation => "allocation",
        ExpressionKind::StructLiteral => "struct",
        ExpressionKind::Unsupported => "unsupported",
        ExpressionKind::Null => "null",
        ExpressionKind::Identifier => "identifier",
        ExpressionKind::Field => "field",
        ExpressionKind::Binary => "binary",
        ExpressionKind::Unary => "unary",
        ExpressionKind::Index => "index",
        ExpressionKind::Call => "call",
        ExpressionKind::Branch => "branch",
        ExpressionKind::Block => "block",
        ExpressionKind::Return => "return",
        ExpressionKind::Let => "let",
        ExpressionKind::Assign => "assign",
        ExpressionKind::Group => "group",
        ExpressionKind::Question => "question",
        ExpressionKind::Method => "method",
        ExpressionKind::While => "while",
        ExpressionKind::For => "for",
        ExpressionKind::Match => "match",
        ExpressionKind::Arm => "arm",
    }
}

fn line(out: &mut String, parts: &[&str]) {
    for part in parts {
        out.push_str(part);
    }
    out.push('\n');
}

fn dump_modules(out: &mut String, tables: &Tables) {
    for index in 0..module_count(&tables.modules) {
        let path = tables
            .modules
            .path
            .get(index)
            .map(|path| text(tables, *path))
            .unwrap_or_default();
        line(
            out,
            &[
                "module ",
                &index.to_string(),
                " name=",
                &module_name(tables, ModuleId(index as u32)),
                " path=",
                &path,
            ],
        );
    }
    for row in 0..tables.unresolved_imports.owner.len() {
        let owner = tables.unresolved_imports.owner[row];
        let name = tables
            .unresolved_imports
            .name
            .get(row)
            .map(|name| text(tables, *name))
            .unwrap_or_default();
        line(
            out,
            &[
                "unresolved ",
                &module_name(tables, owner),
                " import=",
                &name,
            ],
        );
    }
    for row in 0..artifact_count(&tables.artifacts) {
        let name = tables
            .artifacts
            .name
            .get(row)
            .map(|name| text(tables, *name))
            .unwrap_or_default();
        let root = match tables.artifacts.root.get(row) {
            Some(root) if root.0 != NO_INDEX => module_name(tables, *root),
            _ => "-".to_string(),
        };
        let kind = tables
            .artifacts
            .kind
            .get(row)
            .map(|kind| artifact_kind(*kind))
            .unwrap_or("-");
        line(out, &["artifact ", &name, " kind=", kind, " root=", &root]);
    }
}

fn dump_types(out: &mut String, tables: &Tables) {
    for index in 0..type_count(&tables.types) {
        let Some(kind) = tables.types.kind.get(index) else {
            continue;
        };
        let element = tables
            .types
            .element
            .get(index)
            .map(|value| handle(value.0))
            .unwrap_or_else(|| "-".to_string());
        let name = tables
            .types
            .name
            .get(index)
            .map(|name| text(tables, *name))
            .unwrap_or_default();
        line(
            out,
            &[
                "type ",
                &index.to_string(),
                " kind=",
                type_kind(*kind),
                " name=",
                &name,
                " element=",
                &element,
                " count=",
                &tables
                    .types
                    .count
                    .get(index)
                    .copied()
                    .unwrap_or(0)
                    .to_string(),
                " size=",
                &tables
                    .types
                    .size
                    .get(index)
                    .copied()
                    .unwrap_or(0)
                    .to_string(),
                " align=",
                &tables
                    .types
                    .alignment
                    .get(index)
                    .copied()
                    .unwrap_or(0)
                    .to_string(),
                " bits=",
                &tables
                    .types
                    .bit_width
                    .get(index)
                    .copied()
                    .unwrap_or(0)
                    .to_string(),
                " flags=",
                &tables
                    .types
                    .flags
                    .get(index)
                    .copied()
                    .unwrap_or(0)
                    .to_string(),
            ],
        );
    }
}

fn dump_structs(out: &mut String, tables: &Tables) {
    for index in 0..struct_count(&tables.structs) {
        let owner = StructId(index as u32);
        let kind = tables
            .structs
            .kind
            .get(index)
            .copied()
            .unwrap_or(ContainerKind::Struct);
        line(
            out,
            &[
                "struct ",
                &struct_name(tables, owner),
                " kind=",
                container_kind(kind),
                " module=",
                &module_name(
                    tables,
                    tables
                        .structs
                        .module
                        .get(index)
                        .copied()
                        .unwrap_or(crate::tables::ROOT_MODULE),
                ),
                " size=",
                &tables
                    .structs
                    .size
                    .get(index)
                    .copied()
                    .unwrap_or(0)
                    .to_string(),
                " align=",
                &tables
                    .structs
                    .alignment
                    .get(index)
                    .copied()
                    .unwrap_or(0)
                    .to_string(),
                " flags=",
                &tables
                    .structs
                    .flags
                    .get(index)
                    .copied()
                    .unwrap_or(0)
                    .to_string(),
                " deinit=",
                &tables
                    .structs
                    .deinit
                    .get(index)
                    .map(|value| {
                        if value.0 == NO_INDEX {
                            "-".to_string()
                        } else {
                            function_name(tables, *value)
                        }
                    })
                    .unwrap_or_else(|| "-".to_string()),
            ],
        );
        for row in crate::tables::struct_fields(&tables.structs, owner) {
            let name = tables
                .fields
                .name
                .get(row)
                .map(|name| text(tables, *name))
                .unwrap_or_default();
            line(
                out,
                &[
                    "field ",
                    &struct_name(tables, owner),
                    ".",
                    &name,
                    " type=",
                    &tables
                        .fields
                        .field_type
                        .get(row)
                        .map(|value| handle(value.0))
                        .unwrap_or_else(|| "-".to_string()),
                    " offset=",
                    &tables
                        .fields
                        .offset
                        .get(row)
                        .copied()
                        .unwrap_or(0)
                        .to_string(),
                ],
            );
        }
    }
}

fn dump_functions(out: &mut String, tables: &Tables) {
    for index in 0..function_count(&tables.functions) {
        let function = FunctionId(index as u32);
        let owner = tables
            .functions
            .owner
            .get(index)
            .copied()
            .unwrap_or(StructId(NO_INDEX));
        line(
            out,
            &[
                "function ",
                &function_name(tables, function),
                " owner=",
                &if owner.0 == NO_INDEX {
                    "-".to_string()
                } else {
                    struct_name(tables, owner)
                },
                " module=",
                &module_name(
                    tables,
                    tables
                        .functions
                        .module
                        .get(index)
                        .copied()
                        .unwrap_or(crate::tables::ROOT_MODULE),
                ),
                " returns=",
                &tables
                    .functions
                    .returns
                    .get(index)
                    .map(|value| handle(value.0))
                    .unwrap_or_else(|| "-".to_string()),
                " errors=",
                &tables
                    .functions
                    .error_set
                    .get(index)
                    .map(|value| {
                        if value.0 == NO_INDEX {
                            "-".to_string()
                        } else {
                            struct_name(tables, *value)
                        }
                    })
                    .unwrap_or_else(|| "-".to_string()),
                " flags=",
                &tables
                    .functions
                    .flags
                    .get(index)
                    .copied()
                    .unwrap_or(0)
                    .to_string(),
            ],
        );
        for row in crate::tables::function_parameters(&tables.functions, function) {
            let name = tables
                .parameters
                .name
                .get(row)
                .map(|name| text(tables, *name))
                .unwrap_or_default();
            line(
                out,
                &[
                    "parameter ",
                    &function_name(tables, function),
                    ".",
                    &name,
                    " type=",
                    &tables
                        .parameters
                        .parameter_type
                        .get(row)
                        .map(|value| handle(value.0))
                        .unwrap_or_else(|| "-".to_string()),
                    " flags=",
                    &tables
                        .parameters
                        .flags
                        .get(row)
                        .copied()
                        .unwrap_or(0)
                        .to_string(),
                ],
            );
        }
    }
}

fn dump_flow(out: &mut String, tables: &Tables) {
    for index in 0..tables.allocator_sources.kind.len() {
        line(
            out,
            &[
                "allocator ",
                &index.to_string(),
                " kind=",
                allocator_kind(tables.allocator_sources.kind[index]),
                " function=",
                &tables
                    .allocator_sources
                    .function
                    .get(index)
                    .map(|value| {
                        if value.0 == NO_INDEX {
                            "-".to_string()
                        } else {
                            function_name(tables, *value)
                        }
                    })
                    .unwrap_or_else(|| "-".to_string()),
                " parameter=",
                &handle(
                    tables
                        .allocator_sources
                        .parameter_index
                        .get(index)
                        .copied()
                        .unwrap_or(NO_INDEX),
                ),
            ],
        );
    }
    for index in 0..call_count(&tables.calls) {
        line(
            out,
            &[
                "call ",
                &index.to_string(),
                " caller=",
                &function_name(tables, tables.calls.caller[index]),
                " callee=",
                &tables
                    .calls
                    .callee
                    .get(index)
                    .map(|value| function_name(tables, *value))
                    .unwrap_or_else(|| "-".to_string()),
            ],
        );
    }
    for row in 0..tables.call_arguments.call.len() {
        line(
            out,
            &[
                "argument call=",
                &handle(tables.call_arguments.call[row].0),
                " parameter=",
                &tables
                    .call_arguments
                    .parameter_index
                    .get(row)
                    .copied()
                    .unwrap_or(0)
                    .to_string(),
                " allocator=",
                &tables
                    .call_arguments
                    .source
                    .get(row)
                    .map(|value| handle(value.0))
                    .unwrap_or_else(|| "-".to_string()),
            ],
        );
    }
    for row in 0..memory_operation_count(&tables.memory_operations) {
        let operations = &tables.memory_operations;
        line(
            out,
            &[
                "memory ",
                &row.to_string(),
                " kind=",
                memory_kind(operations.kind[row]),
                " function=",
                &operations
                    .function
                    .get(row)
                    .map(|value| function_name(tables, *value))
                    .unwrap_or_else(|| "-".to_string()),
                " allocator=",
                &operations
                    .allocator
                    .get(row)
                    .map(|value| handle(value.0))
                    .unwrap_or_else(|| "-".to_string()),
                " place=",
                operations
                    .place
                    .get(row)
                    .map(|kind| place_kind(*kind))
                    .unwrap_or("-"),
                " field=",
                &operations
                    .place_field
                    .get(row)
                    .map(|value| handle(value.0))
                    .unwrap_or_else(|| "-".to_string()),
            ],
        );
    }
}

fn dump_expressions(out: &mut String, tables: &Tables) {
    for index in 0..tables.expressions.kind.len() {
        let children: Vec<String> = crate::tables::expression_children(&tables.expressions, index)
            .filter_map(|slot| tables.expressions.children.get(slot))
            .map(|child| handle(child.0))
            .collect();
        line(
            out,
            &[
                "expression ",
                &index.to_string(),
                " kind=",
                expression_kind(tables.expressions.kind[index]),
                " text=",
                &tables
                    .expressions
                    .text
                    .get(index)
                    .map(|value| text(tables, *value))
                    .unwrap_or_else(|| "-".to_string()),
                " parameter=",
                &handle(
                    tables
                        .expressions
                        .parameter
                        .get(index)
                        .copied()
                        .unwrap_or(NO_INDEX),
                ),
                " field=",
                &tables
                    .expressions
                    .field
                    .get(index)
                    .map(|value| handle(value.0))
                    .unwrap_or_else(|| "-".to_string()),
                " children=",
                &children.join(","),
            ],
        );
    }
    for row in 0..tables.field_assignments.field.len() {
        line(
            out,
            &[
                "assignment field=",
                &handle(tables.field_assignments.field[row].0),
                " function=",
                &tables
                    .field_assignments
                    .function
                    .get(row)
                    .map(|value| function_name(tables, *value))
                    .unwrap_or_else(|| "-".to_string()),
                " source=",
                tables
                    .field_assignments
                    .source
                    .get(row)
                    .map(|value| assignment_source(*value))
                    .unwrap_or("-"),
                " expression=",
                &tables
                    .field_assignments
                    .expression
                    .get(row)
                    .map(|value| handle(value.0))
                    .unwrap_or_else(|| "-".to_string()),
            ],
        );
    }
}

/// The whole database, in the fixed order the tables are declared in. Two
/// dumps of the same tables are byte identical, which is what lets one be
/// compared against a checked in file.
pub fn dump(tables: &Tables) -> String {
    let mut out = String::new();
    line(&mut out, &["target ", &text(tables, tables.target)]);
    line(
        &mut out,
        &[
            "counts modules=",
            &module_count(&tables.modules).to_string(),
            " types=",
            &type_count(&tables.types).to_string(),
            " structs=",
            &struct_count(&tables.structs).to_string(),
            " fields=",
            &field_count(&tables.fields).to_string(),
            " functions=",
            &function_count(&tables.functions).to_string(),
            " calls=",
            &call_count(&tables.calls).to_string(),
        ],
    );
    dump_modules(&mut out, tables);
    dump_types(&mut out, tables);
    dump_structs(&mut out, tables);
    dump_functions(&mut out, tables);
    dump_flow(&mut out, tables);
    dump_expressions(&mut out, tables);
    out
}

/// A type identifier as a reader would write it, which is what `dump` prints
/// numbers for and a caller may want spelled out.
pub fn describe_type(tables: &Tables, kind: TypeId) -> String {
    let index = kind.0 as usize;
    let Some(entry) = tables.types.kind.get(index) else {
        return "-".to_string();
    };
    type_kind(*entry).to_string()
}
