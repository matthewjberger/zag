use zag_facts::build::intern;
use zag_facts::{NO_INDEX, StringId};
use zag_render::ast::{
    Ast, Lifetime, NodeId, NodeKind, STRUCT_FLAG_ARENA_LIFETIME, STRUCT_FLAG_BORROW_LIFETIME,
    STRUCT_FLAG_REPR_C, empty_ast, node_children, node_count, push_node,
};
use zag_render::{RenderError, render};

fn absent() -> StringId {
    StringId(NO_INDEX)
}

fn path(ast: &mut Ast, text: &[u8]) -> NodeId {
    let name = intern(&mut ast.strings, text);
    push_node(ast, NodeKind::TypePath, name, absent(), 0, 0, &[])
}

fn field(ast: &mut Ast, text: &[u8], kind: NodeId) -> NodeId {
    let name = intern(&mut ast.strings, text);
    push_node(ast, NodeKind::Field, name, absent(), 0, 0, &[kind])
}

fn structure(ast: &mut Ast, text: &[u8], flags: u32, fields: &[NodeId]) -> NodeId {
    let name = intern(&mut ast.strings, text);
    push_node(ast, NodeKind::Struct, name, absent(), 0, flags, fields)
}

fn file(ast: &mut Ast, items: &[NodeId]) {
    ast.root = push_node(ast, NodeKind::File, absent(), absent(), 0, 0, items);
}

fn rendered(ast: &Ast) -> String {
    String::from_utf8(render(ast).expect("the tree must render")).expect("the output is text")
}

#[test]
fn a_file_with_no_items_renders_as_nothing() {
    let mut ast = empty_ast();
    file(&mut ast, &[]);
    assert_eq!(rendered(&ast), "");
}

#[test]
fn a_struct_with_no_fields_renders_with_an_empty_body() {
    let mut ast = empty_ast();
    let item = structure(&mut ast, b"Empty", 0, &[]);
    file(&mut ast, &[item]);
    assert_eq!(rendered(&ast), "pub struct Empty {\n}\n");
}

#[test]
fn an_extern_struct_gets_a_c_representation() {
    let mut ast = empty_ast();
    let kind = path(&mut ast, b"u32");
    let member = field(&mut ast, b"magic", kind);
    let item = structure(&mut ast, b"Header", STRUCT_FLAG_REPR_C, &[member]);
    file(&mut ast, &[item]);
    assert_eq!(
        rendered(&ast),
        "#[repr(C)]\npub struct Header {\n    pub magic: u32,\n}\n"
    );
}

#[test]
fn a_borrowing_struct_declares_one_lifetime() {
    let mut ast = empty_ast();
    let item = structure(&mut ast, b"View", STRUCT_FLAG_BORROW_LIFETIME, &[]);
    file(&mut ast, &[item]);
    assert_eq!(rendered(&ast), "pub struct View<'a> {\n}\n");
}

#[test]
fn an_arena_struct_declares_the_arena_lifetime() {
    let mut ast = empty_ast();
    let item = structure(&mut ast, b"Node", STRUCT_FLAG_ARENA_LIFETIME, &[]);
    file(&mut ast, &[item]);
    assert_eq!(rendered(&ast), "pub struct Node<'bump> {\n}\n");
}

#[test]
fn a_struct_that_borrows_and_arena_allocates_declares_both_lifetimes() {
    let mut ast = empty_ast();
    let item = structure(
        &mut ast,
        b"Both",
        STRUCT_FLAG_BORROW_LIFETIME | STRUCT_FLAG_ARENA_LIFETIME,
        &[],
    );
    file(&mut ast, &[item]);
    assert_eq!(rendered(&ast), "pub struct Both<'a, 'bump> {\n}\n");
}

fn render_one_type(build: impl Fn(&mut Ast) -> NodeId) -> String {
    let mut ast = empty_ast();
    let kind = build(&mut ast);
    let member = field(&mut ast, b"value", kind);
    let item = structure(&mut ast, b"Holder", 0, &[member]);
    file(&mut ast, &[item]);
    let text = rendered(&ast);
    text.lines()
        .nth(1)
        .expect("the field line")
        .trim()
        .trim_start_matches("pub value: ")
        .trim_end_matches(',')
        .to_string()
}

#[test]
fn a_boxed_slice_renders_as_an_owned_slice() {
    assert_eq!(
        render_one_type(|ast| {
            let element = path(ast, b"u8");
            let body = push_node(
                ast,
                NodeKind::TypeSliceBody,
                absent(),
                absent(),
                0,
                0,
                &[element],
            );
            push_node(ast, NodeKind::TypeBoxed, absent(), absent(), 0, 0, &[body])
        }),
        "Box<[u8]>"
    );
}

#[test]
fn an_option_renders_around_whatever_it_holds() {
    assert_eq!(
        render_one_type(|ast| {
            let element = path(ast, b"u8");
            let body = push_node(
                ast,
                NodeKind::TypeSliceBody,
                absent(),
                absent(),
                0,
                0,
                &[element],
            );
            let boxed = push_node(ast, NodeKind::TypeBoxed, absent(), absent(), 0, 0, &[body]);
            push_node(
                ast,
                NodeKind::TypeOption,
                absent(),
                absent(),
                0,
                0,
                &[boxed],
            )
        }),
        "Option<Box<[u8]>>"
    );
}

#[test]
fn an_array_renders_its_length_beside_its_element() {
    assert_eq!(
        render_one_type(|ast| {
            let element = path(ast, b"u32");
            push_node(
                ast,
                NodeKind::TypeArray,
                absent(),
                absent(),
                4,
                0,
                &[element],
            )
        }),
        "[u32; 4]"
    );
}

#[test]
fn an_array_of_arrays_nests() {
    assert_eq!(
        render_one_type(|ast| {
            let element = path(ast, b"u8");
            let inner = push_node(
                ast,
                NodeKind::TypeArray,
                absent(),
                absent(),
                2,
                0,
                &[element],
            );
            push_node(ast, NodeKind::TypeArray, absent(), absent(), 3, 0, &[inner])
        }),
        "[[u8; 2]; 3]"
    );
}

#[test]
fn a_result_renders_what_it_gives_back_before_what_it_fails_with() {
    assert_eq!(
        render_one_type(|ast| {
            let value = path(ast, b"Colour");
            let failure = path(ast, b"ParseError");
            push_node(
                ast,
                NodeKind::TypeResult,
                absent(),
                absent(),
                0,
                0,
                &[value, failure],
            )
        }),
        "Result<Colour, ParseError>"
    );
}

#[test]
fn a_result_missing_one_half_is_refused() {
    let mut ast = empty_ast();
    let value = path(&mut ast, b"Colour");
    let kind = push_node(
        &mut ast,
        NodeKind::TypeResult,
        absent(),
        absent(),
        0,
        0,
        &[value],
    );
    let member = field(&mut ast, b"value", kind);
    let item = structure(&mut ast, b"Holder", 0, &[member]);
    file(&mut ast, &[item]);
    assert_eq!(
        render(&ast),
        Err(RenderError::MissingChild { node: kind }),
        "a result with one half cannot be spelled"
    );
}

#[test]
fn a_function_body_renders_every_statement_in_order() {
    let mut ast = empty_ast();
    let kind = path(&mut ast, b"u32");
    let name = intern(&mut ast.strings, b"text");
    let parameter = push_node(&mut ast, NodeKind::Parameter, name, absent(), 0, 0, &[kind]);
    let returns = path(&mut ast, b"u32");
    let discarded = intern(&mut ast.strings, b"text");
    let discard = push_node(&mut ast, NodeKind::Discard, discarded, absent(), 0, 0, &[]);
    let body = intern(&mut ast.strings, b"todo!()");
    let body = push_node(
        &mut ast,
        NodeKind::ExpressionLiteral,
        body,
        absent(),
        0,
        0,
        &[],
    );
    let name = intern(&mut ast.strings, b"measure");
    let function = push_node(
        &mut ast,
        NodeKind::Function,
        name,
        absent(),
        1,
        0,
        &[parameter, returns, discard, body],
    );
    file(&mut ast, &[function]);
    assert_eq!(
        rendered(&ast),
        "pub fn measure(text: u32) -> u32 {\n    let _ = text;\n    todo!()\n}\n"
    );
}

#[test]
fn a_receiver_renders_without_a_type_beside_it() {
    let mut ast = empty_ast();
    let name = intern(&mut ast.strings, b"&mut self");
    let receiver = push_node(
        &mut ast,
        NodeKind::Parameter,
        name,
        absent(),
        0,
        zag_render::ast::PARAMETER_FLAG_RECEIVER,
        &[],
    );
    let returns = path(&mut ast, b"u32");
    let body = intern(&mut ast.strings, b"todo!()");
    let body = push_node(
        &mut ast,
        NodeKind::ExpressionLiteral,
        body,
        absent(),
        0,
        0,
        &[],
    );
    let name = intern(&mut ast.strings, b"length");
    let function = push_node(
        &mut ast,
        NodeKind::Function,
        name,
        absent(),
        1,
        0,
        &[receiver, returns, body],
    );
    let name = intern(&mut ast.strings, b"Holder");
    let block = push_node(
        &mut ast,
        NodeKind::Implementation,
        name,
        absent(),
        0,
        0,
        &[function],
    );
    file(&mut ast, &[block]);
    assert_eq!(
        rendered(&ast),
        "impl Holder {\n    pub fn length(&mut self) -> u32 {\n        todo!()\n    }\n}\n"
    );
}

#[test]
fn an_implementation_for_a_borrowing_struct_declares_the_lifetime_on_both_sides() {
    let mut ast = empty_ast();
    let returns = path(&mut ast, b"u32");
    let body = intern(&mut ast.strings, b"todo!()");
    let body = push_node(
        &mut ast,
        NodeKind::ExpressionLiteral,
        body,
        absent(),
        0,
        0,
        &[],
    );
    let name = intern(&mut ast.strings, b"length");
    let function = push_node(
        &mut ast,
        NodeKind::Function,
        name,
        absent(),
        0,
        0,
        &[returns, body],
    );
    let name = intern(&mut ast.strings, b"View");
    let block = push_node(
        &mut ast,
        NodeKind::Implementation,
        name,
        absent(),
        0,
        STRUCT_FLAG_BORROW_LIFETIME,
        &[function],
    );
    file(&mut ast, &[block]);
    assert!(
        rendered(&ast).starts_with("impl<'a> View<'a> {"),
        "{}",
        rendered(&ast)
    );
}

#[test]
fn each_lifetime_renders_its_own_name() {
    let cases = [
        (Lifetime::Borrow, "&'a [u8]"),
        (Lifetime::Static, "&'static [u8]"),
        (Lifetime::Arena, "&'bump [u8]"),
    ];
    for (lifetime, expected) in cases {
        assert_eq!(
            render_one_type(|ast| {
                let element = path(ast, b"u8");
                let body = push_node(
                    ast,
                    NodeKind::TypeSliceBody,
                    absent(),
                    absent(),
                    0,
                    0,
                    &[element],
                );
                push_node(
                    ast,
                    NodeKind::TypeReference,
                    absent(),
                    absent(),
                    0,
                    lifetime as u32,
                    &[body],
                )
            }),
            expected
        );
    }
}

#[test]
fn an_unknown_pointer_renders_as_an_optional_non_null() {
    assert_eq!(
        render_one_type(|ast| {
            let element = path(ast, b"u8");
            let body = push_node(
                ast,
                NodeKind::TypeSliceBody,
                absent(),
                absent(),
                0,
                0,
                &[element],
            );
            push_node(
                ast,
                NodeKind::TypeOptionNonNull,
                absent(),
                absent(),
                0,
                0,
                &[body],
            )
        }),
        "Option<core::ptr::NonNull<[u8]>>"
    );
}

#[test]
fn nested_slices_render_from_the_inside_out() {
    assert_eq!(
        render_one_type(|ast| {
            let element = path(ast, b"u8");
            let inner = push_node(
                ast,
                NodeKind::TypeSliceBody,
                absent(),
                absent(),
                0,
                0,
                &[element],
            );
            push_node(
                ast,
                NodeKind::TypeSliceBody,
                absent(),
                absent(),
                0,
                0,
                &[inner],
            )
        }),
        "[[u8]]"
    );
}

#[test]
fn layout_assertions_render_one_per_line_with_no_blank_between_them() {
    let mut ast = empty_ast();
    let name = intern(&mut ast.strings, b"Header");
    let field_name = intern(&mut ast.strings, b"magic");
    let size = push_node(&mut ast, NodeKind::AssertSize, name, absent(), 8, 0, &[]);
    let alignment = push_node(
        &mut ast,
        NodeKind::AssertAlignment,
        name,
        absent(),
        4,
        0,
        &[],
    );
    let offset = push_node(
        &mut ast,
        NodeKind::AssertOffset,
        name,
        field_name,
        0,
        0,
        &[],
    );
    file(&mut ast, &[size, alignment, offset]);
    assert_eq!(
        rendered(&ast),
        "const _: () = assert!(core::mem::size_of::<Header>() == 8);\n\
         const _: () = assert!(core::mem::align_of::<Header>() == 4);\n\
         const _: () = assert!(core::mem::offset_of!(Header, magic) == 0);\n"
    );
}

#[test]
fn items_are_separated_by_a_blank_line() {
    let mut ast = empty_ast();
    let first = structure(&mut ast, b"First", 0, &[]);
    let second = structure(&mut ast, b"Second", 0, &[]);
    file(&mut ast, &[first, second]);
    assert_eq!(
        rendered(&ast),
        "pub struct First {\n}\n\npub struct Second {\n}\n"
    );
}

#[test]
fn a_tree_with_no_root_is_refused() {
    let ast = empty_ast();
    assert_eq!(render(&ast), Err(RenderError::MissingRoot));
}

#[test]
fn a_root_that_is_not_a_file_is_refused() {
    let mut ast = empty_ast();
    let item = structure(&mut ast, b"Lonely", 0, &[]);
    ast.root = item;
    assert!(matches!(render(&ast), Err(RenderError::WrongKind { .. })));
}

#[test]
fn a_type_where_an_item_belongs_is_refused() {
    let mut ast = empty_ast();
    let item = path(&mut ast, b"u32");
    file(&mut ast, &[item]);
    assert!(matches!(render(&ast), Err(RenderError::WrongKind { .. })));
}

#[test]
fn an_item_where_a_type_belongs_is_refused() {
    let mut ast = empty_ast();
    let inner = structure(&mut ast, b"Inner", 0, &[]);
    let member = field(&mut ast, b"value", inner);
    let item = structure(&mut ast, b"Outer", 0, &[member]);
    file(&mut ast, &[item]);
    assert!(matches!(render(&ast), Err(RenderError::WrongKind { .. })));
}

#[test]
fn a_wrapper_type_with_no_child_is_refused() {
    let mut ast = empty_ast();
    let body = push_node(&mut ast, NodeKind::TypeBoxed, absent(), absent(), 0, 0, &[]);
    let member = field(&mut ast, b"value", body);
    let item = structure(&mut ast, b"Holder", 0, &[member]);
    file(&mut ast, &[item]);
    assert!(matches!(
        render(&ast),
        Err(RenderError::MissingChild { .. })
    ));
}

#[test]
fn a_child_pointing_outside_the_tree_is_refused() {
    let mut ast = empty_ast();
    file(&mut ast, &[NodeId(500)]);
    assert!(matches!(
        render(&ast),
        Err(RenderError::NodeOutOfRange { .. })
    ));
}

#[test]
fn child_ranges_tile_the_child_table() {
    let mut ast = empty_ast();
    let kind = path(&mut ast, b"u32");
    let member = field(&mut ast, b"value", kind);
    let item = structure(&mut ast, b"Holder", 0, &[member]);
    file(&mut ast, &[item]);
    let mut total = 0;
    for index in 0..node_count(&ast) {
        total += node_children(&ast, NodeId(index as u32)).len();
    }
    assert_eq!(total, ast.children.len());
}

#[test]
fn children_of_an_out_of_range_node_are_empty() {
    let ast = empty_ast();
    assert_eq!(node_children(&ast, NodeId(9)), 0..0);
}
