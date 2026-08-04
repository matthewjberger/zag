use crate::build::{
    declare_field, declare_function, declare_module, declare_parameter, declare_struct, intern,
    name_root_module, push_allocator_source, push_body_expression, push_call, push_call_argument,
    push_expression, push_field_assignment_at, push_integer_type, push_memory_operation,
    push_opaque_type, push_pointer_type, push_slice_type, push_string, push_void_type,
    set_expression_line, set_function_body, set_function_line, set_function_module,
    set_function_signature, set_struct_module, struct_type,
};
use crate::handles::{ExpressionId, FieldId, FunctionId, NO_INDEX, StringId, StructId};
use crate::tables::{
    AllocatorSourceKind, AssignmentSource, ExpressionKind, MemoryOperationKind,
    PARAMETER_FLAG_ALLOCATOR, PARAMETER_FLAG_MUTABLE, PlaceKind, ROOT_MODULE, Tables, empty_tables,
};

fn sum_of_amounts(tables: &mut Tables) -> ExpressionId {
    let side = |tables: &mut Tables, name: &[u8]| {
        let spelled = push_string(&mut tables.strings, name);
        let base = push_body_expression(tables, ExpressionKind::Identifier, spelled, 14, &[]);
        let field = push_string(&mut tables.strings, b"amount");
        push_body_expression(tables, ExpressionKind::Field, field, 14, &[base])
    };
    let first = side(tables, b"first");
    let second = side(tables, b"second");
    let plus = push_string(&mut tables.strings, b"+");
    let sum = push_body_expression(tables, ExpressionKind::Binary, plus, 14, &[first, second]);
    let returned = push_body_expression(
        tables,
        ExpressionKind::Return,
        StringId(NO_INDEX),
        14,
        &[sum],
    );
    push_body_expression(
        tables,
        ExpressionKind::Block,
        StringId(NO_INDEX),
        13,
        &[returned],
    )
}

fn name(tables: &mut Tables, text: &[u8], line: u32) -> ExpressionId {
    let spelled = push_string(&mut tables.strings, text);
    push_body_expression(tables, ExpressionKind::Identifier, spelled, line, &[])
}

/// `var highest = 0; for (entries) |item| highest = @max(highest, item.amount);
/// return highest;`, which is the loop, the local, and the builtin all at once.
fn largest_body(tables: &mut Tables) -> ExpressionId {
    let zero = push_string(&mut tables.strings, b"0");
    let zero = push_body_expression(tables, ExpressionKind::Literal, zero, 20, &[]);
    let held = push_string(&mut tables.strings, b"highest");
    let declared = push_body_expression(tables, ExpressionKind::Let, held, 20, &[zero]);
    if let Some(slot) = tables.expressions.parameter.get_mut(declared.0 as usize) {
        *slot = 1;
    }
    // `var highest: u32` writes the width down, and Rust needs it: without one
    // the binding takes its type from whatever is assigned to it next.
    let word = push_integer_type(tables, 32, false);
    if let Some(slot) = tables.expressions.result.get_mut(declared.0 as usize) {
        *slot = word;
    }

    let item = name(tables, b"item", 22);
    let amount = push_string(&mut tables.strings, b"amount");
    let amount = push_body_expression(tables, ExpressionKind::Field, amount, 22, &[item]);
    let running = name(tables, b"highest", 22);
    let method = push_string(&mut tables.strings, b"max");
    let largest = push_body_expression(
        tables,
        ExpressionKind::Method,
        method,
        22,
        &[running, amount],
    );
    let place = name(tables, b"highest", 22);
    let stored = push_body_expression(
        tables,
        ExpressionKind::Assign,
        StringId(NO_INDEX),
        22,
        &[place, largest],
    );
    let inside = push_body_expression(
        tables,
        ExpressionKind::Block,
        StringId(NO_INDEX),
        21,
        &[stored],
    );
    // Zig walks a slice by value and Rust moves what it walks, so the loop
    // borrows and the elements are read through the reference.
    let sequence = name(tables, b"entries", 21);
    let walking = push_string(&mut tables.strings, b"iter");
    let sequence = push_body_expression(tables, ExpressionKind::Method, walking, 21, &[sequence]);
    let binding = push_string(&mut tables.strings, b"item");
    let walked = push_body_expression(
        tables,
        ExpressionKind::For,
        binding,
        21,
        &[sequence, inside],
    );

    let answer = name(tables, b"highest", 24);
    let returned = push_body_expression(
        tables,
        ExpressionKind::Return,
        StringId(NO_INDEX),
        24,
        &[answer],
    );
    push_body_expression(
        tables,
        ExpressionKind::Block,
        StringId(NO_INDEX),
        19,
        &[declared, walked, returned],
    )
}

/// `return total(first, second) + 1;`, which is a call to a function the
/// tables declare rather than to something they know nothing about.
fn combined_body(tables: &mut Tables, total: FunctionId) -> ExpressionId {
    let first = name(tables, b"first", 28);
    let second = name(tables, b"second", 28);
    let called = push_body_expression(
        tables,
        ExpressionKind::Call,
        StringId(NO_INDEX),
        28,
        &[first, second],
    );
    if let Some(slot) = tables.expressions.parameter.get_mut(called.0 as usize) {
        *slot = total.0;
    }
    let one = push_string(&mut tables.strings, b"1");
    let one = push_body_expression(tables, ExpressionKind::Literal, one, 28, &[]);
    let plus = push_string(&mut tables.strings, b"+");
    let sum = push_body_expression(tables, ExpressionKind::Binary, plus, 28, &[called, one]);
    let returned = push_body_expression(
        tables,
        ExpressionKind::Return,
        StringId(NO_INDEX),
        28,
        &[sum],
    );
    push_body_expression(
        tables,
        ExpressionKind::Block,
        StringId(NO_INDEX),
        27,
        &[returned],
    )
}

pub fn tables() -> Tables {
    let mut tables = empty_tables();
    tables.target = intern(&mut tables.strings, b"x86_64-linux");

    name_root_module(&mut tables, b"", b"main.zig");
    let close_module = declare_module(&mut tables, b"close", b"close.zig");
    let entry_module = declare_module(&mut tables, b"entry", b"entry.zig");
    let store_module = declare_module(&mut tables, b"store", b"store.zig");

    let byte = push_integer_type(&mut tables, 8, false);
    let word = push_integer_type(&mut tables, 32, false);
    let text = push_slice_type(&mut tables, byte);
    let void = push_void_type(&mut tables);
    let allocator_name = intern(&mut tables.strings, b"Allocator");
    let allocator = push_opaque_type(&mut tables, allocator_name);

    let entry = declare_struct(&mut tables, b"Entry", 24, 8, 0);
    set_struct_module(&mut tables, entry, entry_module);
    let entry_label = declare_field(&mut tables, entry, b"label", text, 0);
    declare_field(&mut tables, entry, b"amount", word, 16);
    let entry_type = struct_type(&tables, entry);
    let entry_pointer = push_pointer_type(&mut tables, entry_type);

    let main = declare_function(&mut tables, b"main", StructId(NO_INDEX));
    set_function_module(&mut tables, main, ROOT_MODULE);
    set_function_signature(&mut tables, main, void, StructId(NO_INDEX), true);

    let close = declare_function(&mut tables, b"close", StructId(NO_INDEX));
    set_function_module(&mut tables, close, close_module);
    declare_parameter(
        &mut tables,
        close,
        b"self",
        entry_pointer,
        PARAMETER_FLAG_MUTABLE,
    );
    declare_parameter(
        &mut tables,
        close,
        b"allocator",
        allocator,
        PARAMETER_FLAG_ALLOCATOR,
    );
    set_function_signature(&mut tables, close, void, StructId(NO_INDEX), false);

    let open = declare_function(&mut tables, b"open", StructId(NO_INDEX));
    set_function_module(&mut tables, open, store_module);
    declare_parameter(
        &mut tables,
        open,
        b"allocator",
        allocator,
        PARAMETER_FLAG_ALLOCATOR,
    );
    declare_parameter(&mut tables, open, b"label", text, 0);
    declare_parameter(&mut tables, open, b"amount", word, 0);
    set_function_signature(&mut tables, open, entry_type, StructId(NO_INDEX), true);

    let total = declare_function(&mut tables, b"total", StructId(NO_INDEX));
    set_function_module(&mut tables, total, store_module);
    declare_parameter(&mut tables, total, b"first", entry_pointer, 0);
    declare_parameter(&mut tables, total, b"second", entry_pointer, 0);
    set_function_signature(&mut tables, total, word, StructId(NO_INDEX), false);

    let entries = push_slice_type(&mut tables, entry_type);
    let largest = declare_function(&mut tables, b"largest", StructId(NO_INDEX));
    set_function_module(&mut tables, largest, store_module);
    declare_parameter(&mut tables, largest, b"entries", entries, 0);
    set_function_signature(&mut tables, largest, word, StructId(NO_INDEX), false);

    let combined = declare_function(&mut tables, b"combined", StructId(NO_INDEX));
    set_function_module(&mut tables, combined, store_module);
    declare_parameter(&mut tables, combined, b"first", entry_pointer, 0);
    declare_parameter(&mut tables, combined, b"second", entry_pointer, 0);
    set_function_signature(&mut tables, combined, word, StructId(NO_INDEX), false);

    for (function, line) in [
        (main, 5),
        (close, 4),
        (open, 4),
        (total, 13),
        (largest, 19),
        (combined, 27),
    ] {
        set_function_line(&mut tables, function, line);
    }

    // The bodies the port writes rather than leaving a `todo!()`.
    let body = sum_of_amounts(&mut tables);
    set_function_body(&mut tables, total, body);
    let body = largest_body(&mut tables);
    set_function_body(&mut tables, largest, body);
    let body = combined_body(&mut tables, total);
    set_function_body(&mut tables, combined, body);

    let page = push_allocator_source(
        &mut tables,
        AllocatorSourceKind::Global,
        FunctionId(NO_INDEX),
        NO_INDEX,
    );
    let close_allocator =
        push_allocator_source(&mut tables, AllocatorSourceKind::Parameter, close, 1);
    let open_allocator =
        push_allocator_source(&mut tables, AllocatorSourceKind::Parameter, open, 0);

    let call = push_call(&mut tables, main, open);
    push_call_argument(&mut tables, call, 0, page);
    let call = push_call(&mut tables, main, close);
    push_call_argument(&mut tables, call, 1, page);

    let allocate = push_memory_operation(
        &mut tables,
        open,
        MemoryOperationKind::Allocate,
        open_allocator,
        PlaceKind::FieldOfParameter,
        entry_label,
    );
    push_memory_operation(
        &mut tables,
        close,
        MemoryOperationKind::Free,
        close_allocator,
        PlaceKind::FieldOfParameter,
        entry_label,
    );

    let copied = push_expression(
        &mut tables,
        ExpressionKind::Allocation,
        StringId(NO_INDEX),
        1,
        text,
        FieldId(NO_INDEX),
        &[],
    );
    set_expression_line(&mut tables, copied, 6);
    push_field_assignment_at(
        &mut tables,
        entry_label,
        open,
        AssignmentSource::Allocation,
        allocate,
        copied,
        6,
    );

    tables
}
