//! Function bodies, built as the expression trees the frontend hands over.
//!
//! The rule these check is all of it or none of it: one shape the port cannot
//! spell anywhere in a body and the whole body stays a `todo!()`, because a
//! body with a hole in it looks finished and is not.

use zag_analysis::analyze;
use zag_emit::lower::lower;
use zag_facts::build::{
    declare_function, declare_parameter, intern, push_body_expression, push_integer_type,
    push_string, push_void_type, set_function_body, set_function_signature,
};
use zag_facts::tables::{ExpressionKind, Tables, empty_tables};
use zag_facts::{ExpressionId, NO_INDEX, StringId, StructId};
use zag_render::render;

struct Program {
    tables: Tables,
    word: zag_facts::TypeId,
}

fn program() -> Program {
    let mut tables = empty_tables();
    tables.target = intern(&mut tables.strings, b"x86_64-linux");
    let word = push_integer_type(&mut tables, 32, false);
    Program { tables, word }
}

fn ported(program: &Program) -> String {
    let analysis = analyze(&program.tables);
    let ast = lower(&program.tables, &analysis.ownership);
    String::from_utf8(render(&ast).expect("the tree must render")).expect("the output is text")
}

fn spelled(
    tables: &mut Tables,
    kind: ExpressionKind,
    text: &[u8],
    children: &[ExpressionId],
) -> ExpressionId {
    let text = if text.is_empty() {
        StringId(NO_INDEX)
    } else {
        push_string(&mut tables.strings, text)
    };
    push_body_expression(tables, kind, text, 1, children)
}

/// A function of two parameters whose body the caller builds.
fn measuring(build: impl Fn(&mut Tables) -> ExpressionId) -> Program {
    let mut program = program();
    let function = declare_function(&mut program.tables, b"measure", StructId(NO_INDEX));
    declare_parameter(&mut program.tables, function, b"left", program.word, 0);
    declare_parameter(&mut program.tables, function, b"right", program.word, 0);
    set_function_signature(
        &mut program.tables,
        function,
        program.word,
        StructId(NO_INDEX),
        false,
    );
    let body = build(&mut program.tables);
    set_function_body(&mut program.tables, function, body);
    program
}

#[test]
fn a_returned_expression_becomes_the_value_the_block_has() {
    let program = measuring(|tables| {
        let left = spelled(tables, ExpressionKind::Identifier, b"left", &[]);
        let right = spelled(tables, ExpressionKind::Identifier, b"right", &[]);
        let sum = spelled(tables, ExpressionKind::Binary, b"+", &[left, right]);
        let returned = spelled(tables, ExpressionKind::Return, b"", &[sum]);
        spelled(tables, ExpressionKind::Block, b"", &[returned])
    });
    let source = ported(&program);
    assert!(source.contains("    left + right\n"), "{source}");
    assert!(!source.contains("todo!()"), "{source}");
    assert!(
        !source.contains("return"),
        "a trailing return is what Rust omits: {source}"
    );
}

#[test]
fn a_field_access_carries_across() {
    let program = measuring(|tables| {
        let base = spelled(tables, ExpressionKind::Identifier, b"left", &[]);
        let field = spelled(tables, ExpressionKind::Field, b"count", &[base]);
        let returned = spelled(tables, ExpressionKind::Return, b"", &[field]);
        spelled(tables, ExpressionKind::Block, b"", &[returned])
    });
    assert!(
        ported(&program).contains("left.count"),
        "{}",
        ported(&program)
    );
}

#[test]
fn a_local_and_the_value_after_it_come_out_as_two_lines() {
    let program = measuring(|tables| {
        let left = spelled(tables, ExpressionKind::Identifier, b"left", &[]);
        let held = spelled(tables, ExpressionKind::Let, b"held", &[left]);
        let name = spelled(tables, ExpressionKind::Identifier, b"held", &[]);
        let returned = spelled(tables, ExpressionKind::Return, b"", &[name]);
        spelled(tables, ExpressionKind::Block, b"", &[held, returned])
    });
    let source = ported(&program);
    assert!(source.contains("let held = left;"), "{source}");
    assert!(source.contains("    held\n"), "{source}");
}

#[test]
fn a_branch_becomes_a_rust_if_with_both_arms() {
    let program = measuring(|tables| {
        let left = spelled(tables, ExpressionKind::Identifier, b"left", &[]);
        let right = spelled(tables, ExpressionKind::Identifier, b"right", &[]);
        let condition = spelled(tables, ExpressionKind::Binary, b">", &[left, right]);
        let one = spelled(tables, ExpressionKind::Literal, b"1", &[]);
        let then = spelled(tables, ExpressionKind::Block, b"", &[one]);
        let zero = spelled(tables, ExpressionKind::Literal, b"0", &[]);
        let otherwise = spelled(tables, ExpressionKind::Block, b"", &[zero]);
        let branch = spelled(
            tables,
            ExpressionKind::Branch,
            b"",
            &[condition, then, otherwise],
        );
        let returned = spelled(tables, ExpressionKind::Return, b"", &[branch]);
        spelled(tables, ExpressionKind::Block, b"", &[returned])
    });
    let source = ported(&program);
    assert!(source.contains("if left > right {"), "{source}");
    assert!(source.contains("} else {"), "{source}");
}

#[test]
fn a_try_becomes_a_question_mark() {
    let program = measuring(|tables| {
        let inner = spelled(tables, ExpressionKind::Identifier, b"left", &[]);
        let asked = spelled(tables, ExpressionKind::Question, b"", &[inner]);
        let returned = spelled(tables, ExpressionKind::Return, b"", &[asked]);
        spelled(tables, ExpressionKind::Block, b"", &[returned])
    });
    assert!(ported(&program).contains("left?"), "{}", ported(&program));
}

#[test]
fn an_index_and_a_group_carry_across() {
    let program = measuring(|tables| {
        let base = spelled(tables, ExpressionKind::Identifier, b"left", &[]);
        let at = spelled(tables, ExpressionKind::Literal, b"0", &[]);
        let indexed = spelled(tables, ExpressionKind::Index, b"", &[base, at]);
        let grouped = spelled(tables, ExpressionKind::Group, b"", &[indexed]);
        let returned = spelled(tables, ExpressionKind::Return, b"", &[grouped]);
        spelled(tables, ExpressionKind::Block, b"", &[returned])
    });
    assert!(
        ported(&program).contains("(left[0])"),
        "{}",
        ported(&program)
    );
}

#[test]
fn one_shape_the_port_cannot_spell_stops_the_whole_body() {
    let program = measuring(|tables| {
        let left = spelled(tables, ExpressionKind::Identifier, b"left", &[]);
        let unreadable = spelled(tables, ExpressionKind::Unsupported, b"@ptrCast(x)", &[]);
        let sum = spelled(tables, ExpressionKind::Binary, b"+", &[left, unreadable]);
        let returned = spelled(tables, ExpressionKind::Return, b"", &[sum]);
        spelled(tables, ExpressionKind::Block, b"", &[returned])
    });
    let source = ported(&program);
    assert!(source.contains("todo!()"), "{source}");
    assert!(!source.contains("@ptrCast"), "{source}");
}

#[test]
fn a_function_with_no_body_keeps_its_marker() {
    let mut program = program();
    let void = push_void_type(&mut program.tables);
    let function = declare_function(&mut program.tables, b"reset", StructId(NO_INDEX));
    set_function_signature(
        &mut program.tables,
        function,
        void,
        StructId(NO_INDEX),
        false,
    );
    assert!(ported(&program).contains("todo!()"), "{}", ported(&program));
}

#[test]
fn a_body_that_is_reported_as_ported_is_the_one_that_gets_written() {
    let program = measuring(|tables| {
        let left = spelled(tables, ExpressionKind::Identifier, b"left", &[]);
        let returned = spelled(tables, ExpressionKind::Return, b"", &[left]);
        spelled(tables, ExpressionKind::Block, b"", &[returned])
    });
    let analysis = analyze(&program.tables);
    let report = String::from_utf8(zag_emit::report::render_report(&program.tables, &analysis))
        .expect("the report is text");
    assert!(report.contains("ported, signature and body"), "{report}");
}
