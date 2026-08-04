use crate::build::{
    declare_field, declare_function, declare_parameter, declare_struct, intern, name_root_module,
    push_body_expression, push_integer_type, push_opaque_type, push_slice_type, push_string,
    push_void_type, set_function_body, set_function_line, set_function_signature, set_struct_kind,
    struct_type,
};
use crate::handles::{ExpressionId, NO_INDEX, StringId, StructId};
use crate::tables::{ContainerKind, ExpressionKind, Tables, empty_tables};

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
    let colour_kind = struct_type(&tables, colour);
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
    let shade = declare_function(&mut tables, b"shade", StructId(NO_INDEX));
    declare_parameter(&mut tables, shade, b"colour", colour_kind, 0);

    let parse = declare_function(&mut tables, b"parse", StructId(NO_INDEX));
    declare_parameter(&mut tables, parse, b"text", text, 0);
    let main = declare_function(&mut tables, b"main", StructId(NO_INDEX));

    let colour_type = struct_type(&tables, colour);
    set_function_signature(&mut tables, area, float, StructId(NO_INDEX), false);
    set_function_signature(&mut tables, shade, word, StructId(NO_INDEX), false);
    set_function_signature(&mut tables, parse, colour_type, failure, true);
    set_function_signature(&mut tables, main, void, StructId(NO_INDEX), true);
    for (function, line) in [(area, 30), (shade, 40), (parse, 48), (main, 56)] {
        set_function_line(&mut tables, function, line);
    }

    let body = shade_body(&mut tables);
    set_function_body(&mut tables, shade, body);

    tables
}

/// `switch (colour) { .red => 1, .green => 2, .blue => 3 }`, which is the one
/// shape that needs the type being switched on to say what an arm means.
fn shade_body(tables: &mut Tables) -> ExpressionId {
    let scrutinee = push_string(&mut tables.strings, b"colour");
    let scrutinee = push_body_expression(tables, ExpressionKind::Identifier, scrutinee, 41, &[]);
    let mut children = vec![scrutinee];
    for (index, variant) in [b"Colour::Red".as_slice(), b"Colour::Green", b"Colour::Blue"]
        .into_iter()
        .enumerate()
    {
        let line = 42 + index as u32;
        let value = push_string(&mut tables.strings, (index + 1).to_string().as_bytes());
        let value = push_body_expression(tables, ExpressionKind::Literal, value, line, &[]);
        let pattern = push_string(&mut tables.strings, variant);
        children.push(push_body_expression(
            tables,
            ExpressionKind::Arm,
            pattern,
            41,
            &[value],
        ));
    }
    let matched = push_body_expression(
        tables,
        ExpressionKind::Match,
        StringId(NO_INDEX),
        41,
        &children,
    );
    let returned = push_body_expression(
        tables,
        ExpressionKind::Return,
        StringId(NO_INDEX),
        41,
        &[matched],
    );
    push_body_expression(
        tables,
        ExpressionKind::Block,
        StringId(NO_INDEX),
        40,
        &[returned],
    )
}
