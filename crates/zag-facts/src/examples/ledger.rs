use crate::build::{
    declare_field, declare_function, declare_module, declare_parameter, declare_struct, intern,
    name_root_module, push_allocator_source, push_call, push_call_argument, push_expression,
    push_field_assignment_with, push_integer_type, push_memory_operation, push_opaque_type,
    push_pointer_type, push_slice_type, push_void_type, set_function_module,
    set_function_signature, set_struct_module, struct_type,
};
use crate::handles::{FieldId, FunctionId, NO_INDEX, StringId, StructId};
use crate::tables::{
    AllocatorSourceKind, AssignmentSource, ExpressionKind, MemoryOperationKind,
    PARAMETER_FLAG_ALLOCATOR, PARAMETER_FLAG_MUTABLE, PlaceKind, ROOT_MODULE, Tables, empty_tables,
};

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
    push_field_assignment_with(
        &mut tables,
        entry_label,
        open,
        AssignmentSource::Allocation,
        allocate,
        copied,
    );

    tables
}
