use crate::build::{
    declare_field, declare_function, declare_parameter, declare_struct, intern, name_root_module,
    push_integer_type, push_opaque_type, push_slice_type, push_void_type, set_function_line,
    set_function_signature, set_struct_kind, struct_type,
};
use crate::handles::{NO_INDEX, StructId};
use crate::tables::{ContainerKind, Tables, empty_tables};

pub fn tables() -> Tables {
    let mut tables = empty_tables();
    tables.target = intern(&mut tables.strings, b"x86_64-linux");
    name_root_module(&mut tables, b"", b"main.zig");

    let byte = push_integer_type(&mut tables, 8, false);
    let word = push_integer_type(&mut tables, 32, false);
    let text = push_slice_type(&mut tables, byte);
    let void = push_void_type(&mut tables);
    let float_name = intern(&mut tables.strings, b"f32");
    let float = push_opaque_type(&mut tables, float_name);

    let colour = declare_struct(&mut tables, b"Colour", 1, 1, 0);
    set_struct_kind(&mut tables, colour, ContainerKind::Enum);
    for variant in [b"red".as_slice(), b"green".as_slice(), b"blue".as_slice()] {
        declare_field(&mut tables, colour, variant, void, 0);
    }

    let extent = declare_struct(&mut tables, b"Extent", 8, 4, 0);
    declare_field(&mut tables, extent, b"width", word, 0);
    declare_field(&mut tables, extent, b"height", word, 4);
    let extent_type = struct_type(&tables, extent);

    let shape = declare_struct(&mut tables, b"Shape", 12, 4, 0);
    set_struct_kind(&mut tables, shape, ContainerKind::Union);
    declare_field(&mut tables, shape, b"circle", float, 0);
    declare_field(&mut tables, shape, b"rectangle", extent_type, 0);
    declare_field(&mut tables, shape, b"empty", void, 0);

    let failure = declare_struct(&mut tables, b"ParseError", 2, 2, 0);
    set_struct_kind(&mut tables, failure, ContainerKind::ErrorSet);
    for variant in [b"Empty".as_slice(), b"TooLarge".as_slice()] {
        declare_field(&mut tables, failure, variant, void, 0);
    }

    let shape_type = struct_type(&tables, shape);
    let area = declare_function(&mut tables, b"area", StructId(NO_INDEX));
    declare_parameter(&mut tables, area, b"shape", shape_type, 0);
    let parse = declare_function(&mut tables, b"parse", StructId(NO_INDEX));
    declare_parameter(&mut tables, parse, b"text", text, 0);
    let main = declare_function(&mut tables, b"main", StructId(NO_INDEX));

    let colour_type = struct_type(&tables, colour);
    set_function_signature(&mut tables, area, float, StructId(NO_INDEX), false);
    set_function_signature(&mut tables, parse, colour_type, failure, true);
    set_function_signature(&mut tables, main, void, StructId(NO_INDEX), true);
    for (function, line) in [(area, 30), (parse, 38), (main, 46)] {
        set_function_line(&mut tables, function, line);
    }

    tables
}
