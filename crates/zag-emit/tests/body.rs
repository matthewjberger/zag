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
fn a_var_becomes_a_mutable_local_and_a_const_does_not() {
    for (mutable, expected) in [(1u32, "let mut held = left;"), (0, "let held = left;")] {
        let program = measuring(move |tables| {
            let left = spelled(tables, ExpressionKind::Identifier, b"left", &[]);
            let held = spelled(tables, ExpressionKind::Let, b"held", &[left]);
            tables.expressions.parameter[held.0 as usize] = mutable;
            let name = spelled(tables, ExpressionKind::Identifier, b"held", &[]);
            let returned = spelled(tables, ExpressionKind::Return, b"", &[name]);
            spelled(tables, ExpressionKind::Block, b"", &[held, returned])
        });
        let source = ported(&program);
        assert!(source.contains(expected), "{source}");
    }
}

#[test]
fn a_method_puts_the_receiver_in_front_of_it() {
    let program = measuring(|tables| {
        let left = spelled(tables, ExpressionKind::Identifier, b"left", &[]);
        let right = spelled(tables, ExpressionKind::Identifier, b"right", &[]);
        let largest = spelled(tables, ExpressionKind::Method, b"max", &[left, right]);
        let returned = spelled(tables, ExpressionKind::Return, b"", &[largest]);
        spelled(tables, ExpressionKind::Block, b"", &[returned])
    });
    assert!(
        ported(&program).contains("left.max(right)"),
        "{}",
        ported(&program)
    );
}

/// A loop has no value, so it stays a statement wherever it sits, and it
/// carries its own braces so no semicolon follows it.
#[test]
fn a_loop_is_a_statement_even_as_the_last_thing_in_a_block() {
    let program = measuring(|tables| {
        let sequence = spelled(tables, ExpressionKind::Identifier, b"left", &[]);
        let item = spelled(tables, ExpressionKind::Identifier, b"item", &[]);
        let inside = spelled(tables, ExpressionKind::Block, b"", &[item]);
        let walked = spelled(tables, ExpressionKind::For, b"item", &[sequence, inside]);
        spelled(tables, ExpressionKind::Block, b"", &[walked])
    });
    let source = ported(&program);
    assert!(source.contains("for item in left {"), "{source}");
    assert!(
        !source.contains("};"),
        "a braced statement takes no semicolon: {source}"
    );
}

#[test]
fn a_while_carries_its_condition_and_its_body() {
    let program = measuring(|tables| {
        let left = spelled(tables, ExpressionKind::Identifier, b"left", &[]);
        let right = spelled(tables, ExpressionKind::Identifier, b"right", &[]);
        let condition = spelled(tables, ExpressionKind::Binary, b"<", &[left, right]);
        let step = spelled(tables, ExpressionKind::Identifier, b"left", &[]);
        let inside = spelled(tables, ExpressionKind::Block, b"", &[step]);
        let loop_ = spelled(tables, ExpressionKind::While, b"", &[condition, inside]);
        spelled(tables, ExpressionKind::Block, b"", &[loop_])
    });
    assert!(
        ported(&program).contains("while left < right {"),
        "{}",
        ported(&program)
    );
}

/// An assignment gives nothing back, so it keeps its semicolon even where a
/// value-producing expression would have become the block's value.
#[test]
fn an_assignment_at_the_end_of_a_block_still_ends_in_a_semicolon() {
    let program = measuring(|tables| {
        let place = spelled(tables, ExpressionKind::Identifier, b"left", &[]);
        let value = spelled(tables, ExpressionKind::Identifier, b"right", &[]);
        let stored = spelled(tables, ExpressionKind::Assign, b"", &[place, value]);
        spelled(tables, ExpressionKind::Block, b"", &[stored])
    });
    assert!(
        ported(&program).contains("left = right;"),
        "{}",
        ported(&program)
    );
}

#[test]
fn a_call_with_no_callee_leaves_the_body_unwritten() {
    let program = measuring(|tables| {
        let called = spelled(tables, ExpressionKind::Call, b"", &[]);
        let returned = spelled(tables, ExpressionKind::Return, b"", &[called]);
        spelled(tables, ExpressionKind::Block, b"", &[returned])
    });
    assert!(ported(&program).contains("todo!()"), "{}", ported(&program));
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
