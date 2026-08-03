use crate::handles::NO_INDEX;
use crate::tables::{
    Tables, call_count, field_count, function_count, memory_operation_count, module_count,
    parameter_count, string_count, struct_count, type_count,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Violation {
    ColumnLengthMismatch {
        table: &'static str,
        column: &'static str,
        expected: usize,
        found: usize,
    },
    StringOffsetsNotMonotonic {
        index: usize,
    },
    StringOffsetOutOfRange {
        index: usize,
    },
    StringBlobNotFullyCovered {
        covered: usize,
        length: usize,
    },
    HandleOutOfRange {
        table: &'static str,
        column: &'static str,
        row: usize,
        value: u32,
    },
    FieldRangeMismatch {
        owner: usize,
    },
    FieldsNotGroupedByOwner {
        row: usize,
    },
    ParameterRangeMismatch {
        owner: usize,
    },
    CallsNotSortedByCaller {
        row: usize,
    },
    ParameterIndexOutOfRange {
        table: &'static str,
        row: usize,
        function: u32,
        parameter_index: u32,
    },
    PlaceWithoutField {
        row: usize,
    },
    /// Nothing declares a module, so every declaration's module handle dangles
    /// and the port has no namespace to put anything in.
    NoRootModule,
    UnresolvedImportRangeMismatch {
        owner: usize,
    },
}

pub fn validate(tables: &Tables) -> Result<(), Vec<Violation>> {
    let mut violations = Vec::new();
    check_column_lengths(tables, &mut violations);
    check_strings(tables, &mut violations);
    check_handles(tables, &mut violations);
    check_field_ranges(tables, &mut violations);
    check_parameter_ranges(tables, &mut violations);
    check_module_ranges(tables, &mut violations);
    check_parameter_indices(tables, &mut violations);
    check_places(tables, &mut violations);
    check_call_order(tables, &mut violations);
    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations)
    }
}

fn expect_length(
    violations: &mut Vec<Violation>,
    table: &'static str,
    column: &'static str,
    expected: usize,
    found: usize,
) {
    if expected != found {
        violations.push(Violation::ColumnLengthMismatch {
            table,
            column,
            expected,
            found,
        });
    }
}

fn check_column_lengths(tables: &Tables, violations: &mut Vec<Violation>) {
    let modules = &tables.modules;
    let count = module_count(modules);
    expect_length(violations, "modules", "path", count, modules.path.len());
    expect_length(
        violations,
        "modules",
        "unresolved_start",
        count,
        modules.unresolved_start.len(),
    );
    expect_length(
        violations,
        "modules",
        "unresolved_count",
        count,
        modules.unresolved_count.len(),
    );
    expect_length(
        violations,
        "unresolved_imports",
        "name",
        tables.unresolved_imports.owner.len(),
        tables.unresolved_imports.name.len(),
    );

    let types = &tables.types;
    let count = type_count(types);
    expect_length(violations, "types", "element", count, types.element.len());
    expect_length(violations, "types", "count", count, types.count.len());
    expect_length(violations, "types", "module", count, types.module.len());
    expect_length(violations, "types", "name", count, types.name.len());
    expect_length(violations, "types", "size", count, types.size.len());
    expect_length(
        violations,
        "types",
        "alignment",
        count,
        types.alignment.len(),
    );
    expect_length(
        violations,
        "types",
        "bit_width",
        count,
        types.bit_width.len(),
    );
    expect_length(violations, "types", "flags", count, types.flags.len());

    check_struct_column_lengths(tables, violations);
    check_field_column_lengths(tables, violations);
    check_function_column_lengths(tables, violations);
    check_side_table_column_lengths(tables, violations);
}

fn check_struct_column_lengths(tables: &Tables, violations: &mut Vec<Violation>) {
    let structs = &tables.structs;
    let count = struct_count(structs);
    expect_length(violations, "structs", "module", count, structs.module.len());
    expect_length(
        violations,
        "structs",
        "type_id",
        count,
        structs.type_id.len(),
    );
    expect_length(
        violations,
        "structs",
        "field_start",
        count,
        structs.field_start.len(),
    );
    expect_length(
        violations,
        "structs",
        "field_count",
        count,
        structs.field_count.len(),
    );
    expect_length(violations, "structs", "size", count, structs.size.len());
    expect_length(
        violations,
        "structs",
        "alignment",
        count,
        structs.alignment.len(),
    );
    expect_length(violations, "structs", "flags", count, structs.flags.len());
    expect_length(violations, "structs", "deinit", count, structs.deinit.len());
    expect_length(violations, "structs", "kind", count, structs.kind.len());
}

fn check_field_column_lengths(tables: &Tables, violations: &mut Vec<Violation>) {
    let fields = &tables.fields;
    let count = field_count(fields);
    expect_length(violations, "fields", "name", count, fields.name.len());
    expect_length(
        violations,
        "fields",
        "field_type",
        count,
        fields.field_type.len(),
    );
    expect_length(violations, "fields", "offset", count, fields.offset.len());
}

fn check_function_column_lengths(tables: &Tables, violations: &mut Vec<Violation>) {
    let functions = &tables.functions;
    let count = function_count(functions);
    expect_length(
        violations,
        "functions",
        "module",
        count,
        functions.module.len(),
    );
    expect_length(
        violations,
        "functions",
        "owner",
        count,
        functions.owner.len(),
    );
    expect_length(
        violations,
        "functions",
        "parameter_start",
        count,
        functions.parameter_start.len(),
    );
    expect_length(
        violations,
        "functions",
        "parameter_count",
        count,
        functions.parameter_count.len(),
    );
    expect_length(
        violations,
        "functions",
        "returns",
        count,
        functions.returns.len(),
    );
    expect_length(
        violations,
        "functions",
        "error_set",
        count,
        functions.error_set.len(),
    );
    expect_length(
        violations,
        "functions",
        "flags",
        count,
        functions.flags.len(),
    );
    expect_length(violations, "functions", "line", count, functions.line.len());

    let parameters = &tables.parameters;
    let count = parameter_count(parameters);
    expect_length(
        violations,
        "parameters",
        "name",
        count,
        parameters.name.len(),
    );
    expect_length(
        violations,
        "parameters",
        "parameter_type",
        count,
        parameters.parameter_type.len(),
    );
    expect_length(
        violations,
        "parameters",
        "flags",
        count,
        parameters.flags.len(),
    );
}

fn check_side_table_column_lengths(tables: &Tables, violations: &mut Vec<Violation>) {
    let sources = &tables.allocator_sources;
    let count = sources.kind.len();
    expect_length(
        violations,
        "allocator_sources",
        "function",
        count,
        sources.function.len(),
    );
    expect_length(
        violations,
        "allocator_sources",
        "parameter_index",
        count,
        sources.parameter_index.len(),
    );

    let calls = &tables.calls;
    expect_length(
        violations,
        "calls",
        "callee",
        call_count(calls),
        calls.callee.len(),
    );

    let arguments = &tables.call_arguments;
    let count = arguments.call.len();
    expect_length(
        violations,
        "call_arguments",
        "parameter_index",
        count,
        arguments.parameter_index.len(),
    );
    expect_length(
        violations,
        "call_arguments",
        "source",
        count,
        arguments.source.len(),
    );

    let operations = &tables.memory_operations;
    let count = memory_operation_count(operations);
    expect_length(
        violations,
        "memory_operations",
        "kind",
        count,
        operations.kind.len(),
    );
    expect_length(
        violations,
        "memory_operations",
        "allocator",
        count,
        operations.allocator.len(),
    );
    expect_length(
        violations,
        "memory_operations",
        "place",
        count,
        operations.place.len(),
    );
    expect_length(
        violations,
        "memory_operations",
        "place_field",
        count,
        operations.place_field.len(),
    );

    let assignments = &tables.field_assignments;
    let count = assignments.field.len();
    expect_length(
        violations,
        "field_assignments",
        "function",
        count,
        assignments.function.len(),
    );
    expect_length(
        violations,
        "field_assignments",
        "source",
        count,
        assignments.source.len(),
    );
    expect_length(
        violations,
        "field_assignments",
        "memory_operation",
        count,
        assignments.memory_operation.len(),
    );
    expect_length(
        violations,
        "field_assignments",
        "expression",
        count,
        assignments.expression.len(),
    );
    expect_length(
        violations,
        "field_assignments",
        "line",
        count,
        assignments.line.len(),
    );
    let expressions = &tables.expressions;
    let count = expressions.kind.len();
    expect_length(
        violations,
        "expressions",
        "line",
        count,
        expressions.line.len(),
    );
    expect_length(
        violations,
        "expressions",
        "text",
        count,
        expressions.text.len(),
    );
    expect_length(
        violations,
        "expressions",
        "parameter",
        count,
        expressions.parameter.len(),
    );
    expect_length(
        violations,
        "expressions",
        "result",
        count,
        expressions.result.len(),
    );
    expect_length(
        violations,
        "expressions",
        "field",
        count,
        expressions.field.len(),
    );
    expect_length(
        violations,
        "expressions",
        "child_start",
        count,
        expressions.child_start.len(),
    );
    expect_length(
        violations,
        "expressions",
        "child_count",
        count,
        expressions.child_count.len(),
    );
}

fn check_strings(tables: &Tables, violations: &mut Vec<Violation>) {
    let strings = &tables.strings;
    if strings.offsets.is_empty() {
        if !strings.bytes.is_empty() {
            violations.push(Violation::StringBlobNotFullyCovered {
                covered: 0,
                length: strings.bytes.len(),
            });
        }
        return;
    }
    if strings.offsets[0] != 0 {
        violations.push(Violation::StringOffsetsNotMonotonic { index: 0 });
    }
    for index in 1..strings.offsets.len() {
        if strings.offsets[index] < strings.offsets[index - 1] {
            violations.push(Violation::StringOffsetsNotMonotonic { index });
        }
        if strings.offsets[index] as usize > strings.bytes.len() {
            violations.push(Violation::StringOffsetOutOfRange { index });
        }
    }
    let covered = *strings.offsets.last().unwrap_or(&0) as usize;
    if covered != strings.bytes.len() {
        violations.push(Violation::StringBlobNotFullyCovered {
            covered,
            length: strings.bytes.len(),
        });
    }
}

struct Limits {
    strings: usize,
    modules: usize,
    types: usize,
    structs: usize,
    fields: usize,
    functions: usize,
    calls: usize,
    allocator_sources: usize,
    memory_operations: usize,
}

fn limits(tables: &Tables) -> Limits {
    Limits {
        strings: string_count(&tables.strings),
        modules: module_count(&tables.modules),
        types: type_count(&tables.types),
        structs: struct_count(&tables.structs),
        fields: field_count(&tables.fields),
        functions: function_count(&tables.functions),
        calls: call_count(&tables.calls),
        allocator_sources: tables.allocator_sources.kind.len(),
        memory_operations: memory_operation_count(&tables.memory_operations),
    }
}

fn check_column<T>(
    violations: &mut Vec<Violation>,
    table: &'static str,
    column: &'static str,
    values: &[T],
    project: impl Fn(&T) -> u32,
    limit: usize,
    allow_absent: bool,
) {
    for (row, value) in values.iter().enumerate() {
        let raw = project(value);
        if raw == NO_INDEX && allow_absent {
            continue;
        }
        if raw as usize >= limit {
            violations.push(Violation::HandleOutOfRange {
                table,
                column,
                row,
                value: raw,
            });
        }
    }
}

fn check_handles(tables: &Tables, violations: &mut Vec<Violation>) {
    let limits = limits(tables);
    check_type_handles(tables, violations, &limits);
    check_declaration_handles(tables, violations, &limits);
    check_flow_handles(tables, violations, &limits);
    if tables.target.0 != NO_INDEX && tables.target.0 as usize >= limits.strings {
        violations.push(Violation::HandleOutOfRange {
            table: "tables",
            column: "target",
            row: 0,
            value: tables.target.0,
        });
    }
}

fn check_type_handles(tables: &Tables, violations: &mut Vec<Violation>, limits: &Limits) {
    check_column(
        violations,
        "modules",
        "name",
        &tables.modules.name,
        |value| value.0,
        limits.strings,
        true,
    );
    check_column(
        violations,
        "modules",
        "path",
        &tables.modules.path,
        |value| value.0,
        limits.strings,
        true,
    );
    check_column(
        violations,
        "unresolved_imports",
        "owner",
        &tables.unresolved_imports.owner,
        |value| value.0,
        limits.modules,
        false,
    );
    check_column(
        violations,
        "unresolved_imports",
        "name",
        &tables.unresolved_imports.name,
        |value| value.0,
        limits.strings,
        false,
    );
    check_column(
        violations,
        "types",
        "module",
        &tables.types.module,
        |value| value.0,
        limits.modules,
        false,
    );
    check_column(
        violations,
        "types",
        "element",
        &tables.types.element,
        |value| value.0,
        limits.types,
        true,
    );
    check_column(
        violations,
        "types",
        "name",
        &tables.types.name,
        |value| value.0,
        limits.strings,
        true,
    );
}

fn check_declaration_handles(tables: &Tables, violations: &mut Vec<Violation>, limits: &Limits) {
    check_column(
        violations,
        "structs",
        "module",
        &tables.structs.module,
        |value| value.0,
        limits.modules,
        false,
    );
    check_column(
        violations,
        "functions",
        "module",
        &tables.functions.module,
        |value| value.0,
        limits.modules,
        false,
    );
    check_column(
        violations,
        "structs",
        "name",
        &tables.structs.name,
        |value| value.0,
        limits.strings,
        false,
    );
    check_column(
        violations,
        "structs",
        "type_id",
        &tables.structs.type_id,
        |value| value.0,
        limits.types,
        false,
    );
    check_column(
        violations,
        "structs",
        "deinit",
        &tables.structs.deinit,
        |value| value.0,
        limits.functions,
        true,
    );
    check_column(
        violations,
        "fields",
        "owner",
        &tables.fields.owner,
        |value| value.0,
        limits.structs,
        false,
    );
    check_column(
        violations,
        "fields",
        "name",
        &tables.fields.name,
        |value| value.0,
        limits.strings,
        false,
    );
    check_column(
        violations,
        "fields",
        "field_type",
        &tables.fields.field_type,
        |value| value.0,
        limits.types,
        false,
    );
    check_column(
        violations,
        "functions",
        "name",
        &tables.functions.name,
        |value| value.0,
        limits.strings,
        false,
    );
    check_column(
        violations,
        "functions",
        "owner",
        &tables.functions.owner,
        |value| value.0,
        limits.structs,
        true,
    );
    check_column(
        violations,
        "functions",
        "returns",
        &tables.functions.returns,
        |value| value.0,
        limits.types,
        true,
    );
    check_column(
        violations,
        "functions",
        "error_set",
        &tables.functions.error_set,
        |value| value.0,
        limits.structs,
        true,
    );
    check_column(
        violations,
        "parameters",
        "owner",
        &tables.parameters.owner,
        |value| value.0,
        limits.functions,
        false,
    );
    check_column(
        violations,
        "parameters",
        "name",
        &tables.parameters.name,
        |value| value.0,
        limits.strings,
        false,
    );
    check_column(
        violations,
        "parameters",
        "parameter_type",
        &tables.parameters.parameter_type,
        |value| value.0,
        limits.types,
        false,
    );
}

fn check_flow_handles(tables: &Tables, violations: &mut Vec<Violation>, limits: &Limits) {
    check_column(
        violations,
        "allocator_sources",
        "function",
        &tables.allocator_sources.function,
        |value| value.0,
        limits.functions,
        true,
    );
    check_column(
        violations,
        "calls",
        "caller",
        &tables.calls.caller,
        |value| value.0,
        limits.functions,
        false,
    );
    check_column(
        violations,
        "calls",
        "callee",
        &tables.calls.callee,
        |value| value.0,
        limits.functions,
        false,
    );
    check_column(
        violations,
        "call_arguments",
        "call",
        &tables.call_arguments.call,
        |value| value.0,
        limits.calls,
        false,
    );
    check_column(
        violations,
        "call_arguments",
        "source",
        &tables.call_arguments.source,
        |value| value.0,
        limits.allocator_sources,
        false,
    );
    check_column(
        violations,
        "memory_operations",
        "function",
        &tables.memory_operations.function,
        |value| value.0,
        limits.functions,
        false,
    );
    check_column(
        violations,
        "memory_operations",
        "allocator",
        &tables.memory_operations.allocator,
        |value| value.0,
        limits.allocator_sources,
        true,
    );
    check_column(
        violations,
        "memory_operations",
        "place_field",
        &tables.memory_operations.place_field,
        |value| value.0,
        limits.fields,
        true,
    );
    check_column(
        violations,
        "field_assignments",
        "field",
        &tables.field_assignments.field,
        |value| value.0,
        limits.fields,
        false,
    );
    check_column(
        violations,
        "field_assignments",
        "function",
        &tables.field_assignments.function,
        |value| value.0,
        limits.functions,
        false,
    );
    check_column(
        violations,
        "field_assignments",
        "memory_operation",
        &tables.field_assignments.memory_operation,
        |value| value.0,
        limits.memory_operations,
        true,
    );
}

fn check_field_ranges(tables: &Tables, violations: &mut Vec<Violation>) {
    let structs = &tables.structs;
    let mut expected_start: u32 = 0;
    for owner in 0..struct_count(structs) {
        let (Some(&start), Some(&count)) = (
            structs.field_start.get(owner),
            structs.field_count.get(owner),
        ) else {
            return;
        };
        if start != expected_start {
            violations.push(Violation::FieldRangeMismatch { owner });
        }
        let Some(next) = start.checked_add(count) else {
            violations.push(Violation::FieldRangeMismatch { owner });
            return;
        };
        expected_start = next;
    }
    if expected_start as usize != field_count(&tables.fields) {
        violations.push(Violation::FieldRangeMismatch {
            owner: struct_count(structs),
        });
    }
    for row in 1..field_count(&tables.fields) {
        if tables.fields.owner[row].0 < tables.fields.owner[row - 1].0 {
            violations.push(Violation::FieldsNotGroupedByOwner { row });
        }
    }
}

fn check_module_ranges(tables: &Tables, violations: &mut Vec<Violation>) {
    let modules = &tables.modules;
    if module_count(modules) == 0 {
        if struct_count(&tables.structs) != 0 || function_count(&tables.functions) != 0 {
            violations.push(Violation::NoRootModule);
        }
        return;
    }
    let mut expected_start: u32 = 0;
    for owner in 0..module_count(modules) {
        let (Some(&start), Some(&count)) = (
            modules.unresolved_start.get(owner),
            modules.unresolved_count.get(owner),
        ) else {
            return;
        };
        if start != expected_start {
            violations.push(Violation::UnresolvedImportRangeMismatch { owner });
        }
        let Some(next) = start.checked_add(count) else {
            violations.push(Violation::UnresolvedImportRangeMismatch { owner });
            return;
        };
        expected_start = next;
    }
    if expected_start as usize != tables.unresolved_imports.owner.len() {
        violations.push(Violation::UnresolvedImportRangeMismatch {
            owner: module_count(modules),
        });
    }
}

fn check_parameter_ranges(tables: &Tables, violations: &mut Vec<Violation>) {
    let functions = &tables.functions;
    let mut expected_start: u32 = 0;
    for owner in 0..function_count(functions) {
        let (Some(&start), Some(&count)) = (
            functions.parameter_start.get(owner),
            functions.parameter_count.get(owner),
        ) else {
            return;
        };
        if start != expected_start {
            violations.push(Violation::ParameterRangeMismatch { owner });
        }
        let Some(next) = start.checked_add(count) else {
            violations.push(Violation::ParameterRangeMismatch { owner });
            return;
        };
        expected_start = next;
    }
    if expected_start as usize != parameter_count(&tables.parameters) {
        violations.push(Violation::ParameterRangeMismatch {
            owner: function_count(functions),
        });
    }
}

/// A memory operation on a field of a parameter that names no field is dropped
/// by the ownership pass, so the free it records would disappear silently.
fn check_places(tables: &Tables, violations: &mut Vec<Violation>) {
    let operations = &tables.memory_operations;
    for row in 0..operations.place.len() {
        if operations.place[row] != crate::tables::PlaceKind::FieldOfParameter {
            continue;
        }
        let names_a_field = operations
            .place_field
            .get(row)
            .is_some_and(|field| field.0 != NO_INDEX);
        if !names_a_field {
            violations.push(Violation::PlaceWithoutField { row });
        }
    }
}

fn names_a_parameter(tables: &Tables, function: u32, parameter_index: u32) -> bool {
    tables
        .functions
        .parameter_count
        .get(function as usize)
        .is_some_and(|count| parameter_index < *count)
}

/// A parameter index that names nothing would make the provenance pass fall
/// back to `Conflicting` for every allocator that flows through it, which
/// reads as a real finding rather than as a broken fact file.
fn check_parameter_indices(tables: &Tables, violations: &mut Vec<Violation>) {
    let sources = &tables.allocator_sources;
    for row in 0..sources.kind.len() {
        if sources.kind[row] != crate::tables::AllocatorSourceKind::Parameter {
            continue;
        }
        let (Some(function), Some(parameter_index)) = (
            sources.function.get(row).map(|value| value.0),
            sources.parameter_index.get(row).copied(),
        ) else {
            continue;
        };
        if !names_a_parameter(tables, function, parameter_index) {
            violations.push(Violation::ParameterIndexOutOfRange {
                table: "allocator_sources",
                row,
                function,
                parameter_index,
            });
        }
    }
    let arguments = &tables.call_arguments;
    for row in 0..arguments.call.len() {
        let call = arguments.call[row].0 as usize;
        let (Some(callee), Some(parameter_index)) = (
            tables.calls.callee.get(call).map(|value| value.0),
            arguments.parameter_index.get(row).copied(),
        ) else {
            continue;
        };
        if !names_a_parameter(tables, callee, parameter_index) {
            violations.push(Violation::ParameterIndexOutOfRange {
                table: "call_arguments",
                row,
                function: callee,
                parameter_index,
            });
        }
    }
}

fn check_call_order(tables: &Tables, violations: &mut Vec<Violation>) {
    for row in 1..call_count(&tables.calls) {
        if tables.calls.caller[row].0 < tables.calls.caller[row - 1].0 {
            violations.push(Violation::CallsNotSortedByCaller { row });
        }
    }
}
