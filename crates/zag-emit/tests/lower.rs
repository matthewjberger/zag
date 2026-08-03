use zag_analysis::analyze;
use zag_analysis::ownership::{Confidence, Ownership, OwnershipClass};
use zag_emit::lower::lower;
use zag_emit::report::render_report;
use zag_facts::build::{
    intern, push_array_type, push_field, push_integer_type, push_optional_type, push_pointer_type,
    push_slice_type, push_struct, push_struct_type,
};
use zag_facts::fixture::example_tables;
use zag_facts::tables::{TypeKind, empty_tables};
use zag_facts::{StructId, TypeId};
use zag_render::ast::{
    NodeKind, STRUCT_FLAG_ARENA_LIFETIME, STRUCT_FLAG_BORROW_LIFETIME, STRUCT_FLAG_REPR_C,
    node_count,
};
use zag_render::render;

fn analysis_with(
    tables: &zag_facts::tables::Tables,
    ownership: Ownership,
) -> zag_analysis::Analysis {
    let mut analysis = analyze(tables);
    analysis.ownership = ownership;
    analysis
}

fn ownership_of(classes: &[OwnershipClass]) -> Ownership {
    Ownership {
        class: classes.to_vec(),
        confidence: vec![Confidence::High; classes.len()],
        evidence_start: vec![0; classes.len()],
        evidence_count: vec![0; classes.len()],
        evidence_kind: Vec::new(),
        evidence_function: Vec::new(),
    }
}

fn one_field_source(
    field_type: impl Fn(&mut zag_facts::tables::Tables) -> TypeId,
    class: OwnershipClass,
) -> String {
    let mut tables = empty_tables();
    let name = intern(&mut tables.strings, b"Holder");
    let holder = push_struct_type(&mut tables, name, 8, 8);
    let owner = push_struct(&mut tables, name, holder, 8, 8, 0);
    let kind = field_type(&mut tables);
    let field_name = intern(&mut tables.strings, b"value");
    push_field(&mut tables, owner, field_name, kind, 0);
    let ast = lower(&tables, &ownership_of(&[class]));
    String::from_utf8(render(&ast).expect("the tree must render")).expect("the output is text")
}

#[test]
fn an_empty_program_lowers_to_an_empty_file() {
    let ast = lower(&empty_tables(), &ownership_of(&[]));
    assert_eq!(node_count(&ast), 1);
    assert_eq!(ast.kind[ast.root.0 as usize], NodeKind::File);
}

#[test]
fn an_owned_slice_becomes_a_boxed_slice() {
    let source = one_field_source(
        |tables| {
            let element = push_integer_type(tables, 8, false);
            push_slice_type(tables, element)
        },
        OwnershipClass::Owned,
    );
    assert!(source.contains("pub value: Box<[u8]>,"), "{source}");
}

#[test]
fn a_borrowed_slice_becomes_a_reference_and_gives_the_struct_a_lifetime() {
    let source = one_field_source(
        |tables| {
            let element = push_integer_type(tables, 8, false);
            push_slice_type(tables, element)
        },
        OwnershipClass::Borrowed,
    );
    assert!(source.contains("pub struct Holder<'a> {"), "{source}");
    assert!(source.contains("pub value: &'a [u8],"), "{source}");
}

#[test]
fn an_arena_slice_becomes_a_reference_with_the_arena_lifetime() {
    let source = one_field_source(
        |tables| {
            let element = push_integer_type(tables, 8, false);
            push_slice_type(tables, element)
        },
        OwnershipClass::Arena,
    );
    assert!(source.contains("pub struct Holder<'bump> {"), "{source}");
    assert!(source.contains("pub value: &'bump [u8],"), "{source}");
}

#[test]
fn a_static_slice_needs_no_lifetime_parameter() {
    let source = one_field_source(
        |tables| {
            let element = push_integer_type(tables, 8, false);
            push_slice_type(tables, element)
        },
        OwnershipClass::Static,
    );
    assert!(source.contains("pub struct Holder {"), "{source}");
    assert!(source.contains("pub value: &'static [u8],"), "{source}");
}

#[test]
fn an_unknown_pointer_becomes_an_optional_non_null() {
    let source = one_field_source(
        |tables| {
            let element = push_integer_type(tables, 8, false);
            push_pointer_type(tables, element)
        },
        OwnershipClass::Unknown,
    );
    assert!(
        source.contains("pub value: Option<core::ptr::NonNull<u8>>,"),
        "{source}"
    );
}

#[test]
fn a_pointer_to_a_struct_names_that_struct() {
    let source = one_field_source(
        |tables| {
            let name = intern(&mut tables.strings, b"Other");
            let other = push_struct_type(tables, name, 4, 4);
            push_pointer_type(tables, other)
        },
        OwnershipClass::Owned,
    );
    assert!(source.contains("pub value: Box<Other>,"), "{source}");
}

#[test]
fn integer_widths_and_signedness_carry_across() {
    for (bits, signed, expected) in [
        (8u32, false, "u8"),
        (16, false, "u16"),
        (32, true, "i32"),
        (64, true, "i64"),
    ] {
        let source = one_field_source(
            move |tables| push_integer_type(tables, bits, signed),
            OwnershipClass::Value,
        );
        assert!(
            source.contains(&format!("pub value: {expected},")),
            "{source}"
        );
    }
}

#[test]
fn an_integer_width_rust_lacks_widens_to_the_next_one_it_has() {
    for (declared, expected) in [
        (1u32, "u8"),
        (3, "u8"),
        (12, "u16"),
        (48, "u64"),
        (65, "u128"),
    ] {
        let source = one_field_source(
            move |tables| push_integer_type(tables, declared, false),
            OwnershipClass::Value,
        );
        assert!(
            source.contains(&format!("pub value: {expected},")),
            "u{declared} should widen to {expected}: {source}"
        );
    }
}

#[test]
fn a_signed_narrow_integer_stays_signed_when_it_widens() {
    let source = one_field_source(
        |tables| push_integer_type(tables, 5, true),
        OwnershipClass::Value,
    );
    assert!(source.contains("pub value: i8,"), "{source}");
}

#[test]
fn an_integer_no_rust_type_can_hold_falls_back_to_the_unit_type() {
    for declared in [0u32, 129, 4096] {
        let source = one_field_source(
            move |tables| push_integer_type(tables, declared, false),
            OwnershipClass::Value,
        );
        assert!(
            source.contains("pub value: (),"),
            "u{declared} has no Rust spelling: {source}"
        );
    }
}

#[test]
fn a_boolean_field_becomes_a_boolean() {
    let source = one_field_source(
        |tables| {
            let types = &mut tables.types;
            types.kind.push(TypeKind::Bool);
            types.element.push(TypeId(zag_facts::NO_INDEX));
            types.count.push(0);
            types.module.push(zag_facts::tables::ROOT_MODULE);
            types.name.push(zag_facts::StringId(zag_facts::NO_INDEX));
            types.size.push(1);
            types.alignment.push(1);
            types.bit_width.push(1);
            types.flags.push(0);
            TypeId(types.kind.len() as u32 - 1)
        },
        OwnershipClass::Value,
    );
    assert!(source.contains("pub value: bool,"), "{source}");
}

#[test]
fn an_array_keeps_the_length_the_zig_declared() {
    let source = one_field_source(
        |tables| {
            let element = push_integer_type(tables, 32, false);
            push_array_type(tables, element, 4, 16)
        },
        OwnershipClass::Value,
    );
    assert!(source.contains("pub value: [u32; 4],"), "{source}");
}

#[test]
fn an_optional_scalar_becomes_an_option_of_that_scalar() {
    let source = one_field_source(
        |tables| {
            let element = push_integer_type(tables, 32, false);
            push_optional_type(tables, element)
        },
        OwnershipClass::Value,
    );
    assert!(source.contains("pub value: Option<u32>,"), "{source}");
}

#[test]
fn an_owned_optional_slice_is_an_optional_box_rather_than_a_box_of_an_option() {
    let source = one_field_source(
        |tables| {
            let element = push_integer_type(tables, 8, false);
            let slice = push_slice_type(tables, element);
            push_optional_type(tables, slice)
        },
        OwnershipClass::Owned,
    );
    assert!(source.contains("pub value: Option<Box<[u8]>>,"), "{source}");
}

#[test]
fn a_borrowed_optional_slice_still_gives_the_struct_a_lifetime() {
    let source = one_field_source(
        |tables| {
            let element = push_integer_type(tables, 8, false);
            let slice = push_slice_type(tables, element);
            push_optional_type(tables, slice)
        },
        OwnershipClass::Borrowed,
    );
    assert!(source.contains("pub struct Holder<'a> {"), "{source}");
    assert!(source.contains("pub value: Option<&'a [u8]>,"), "{source}");
}

#[test]
fn an_array_of_a_struct_names_that_struct() {
    let source = one_field_source(
        |tables| {
            let name = intern(&mut tables.strings, b"Other");
            let other = push_struct_type(tables, name, 4, 4);
            push_array_type(tables, other, 3, 12)
        },
        OwnershipClass::Value,
    );
    assert!(source.contains("pub value: [Other; 3],"), "{source}");
}

#[test]
fn a_field_whose_type_is_missing_falls_back_rather_than_panicking() {
    let source = one_field_source(|_| TypeId(9999), OwnershipClass::Value);
    assert!(source.contains("pub value: (),"), "{source}");
}

#[test]
fn a_self_referential_type_terminates_at_the_depth_limit() {
    let mut tables = empty_tables();
    let name = intern(&mut tables.strings, b"Holder");
    let holder = push_struct_type(&mut tables, name, 8, 8);
    let owner = push_struct(&mut tables, name, holder, 8, 8, 0);
    let looping = push_slice_type(&mut tables, TypeId(0));
    tables.types.element[looping.0 as usize] = looping;
    let field_name = intern(&mut tables.strings, b"value");
    push_field(&mut tables, owner, field_name, looping, 0);
    let ast = lower(&tables, &ownership_of(&[OwnershipClass::Value]));
    let source = String::from_utf8(render(&ast).expect("the tree must render")).expect("text");
    assert!(
        source.contains("pub value: [[[[[[[[()]]]]]]]],"),
        "{source}"
    );
}

#[test]
fn only_extern_structs_get_layout_assertions() {
    let tables = example_tables();
    let analysis = analyze(&tables);
    let ast = lower(&tables, &analysis.ownership);
    let assertions = ast
        .kind
        .iter()
        .filter(|kind| {
            matches!(
                kind,
                NodeKind::AssertSize | NodeKind::AssertAlignment | NodeKind::AssertOffset
            )
        })
        .count();
    assert_eq!(assertions, 5);
}

#[test]
fn the_extern_flag_reaches_the_syntax_tree() {
    let tables = example_tables();
    let analysis = analyze(&tables);
    let ast = lower(&tables, &analysis.ownership);
    let flags: Vec<u32> = ast
        .kind
        .iter()
        .zip(&ast.flags)
        .filter(|(kind, _)| **kind == NodeKind::Struct)
        .map(|(_, flags)| *flags)
        .collect();
    assert_eq!(
        flags,
        vec![
            0,
            STRUCT_FLAG_REPR_C,
            STRUCT_FLAG_ARENA_LIFETIME,
            STRUCT_FLAG_BORROW_LIFETIME,
            0
        ]
    );
}

#[test]
fn lowering_is_deterministic() {
    let tables = example_tables();
    let analysis = analyze(&tables);
    assert_eq!(
        lower(&tables, &analysis.ownership),
        lower(&tables, &analysis.ownership)
    );
}

#[test]
fn a_struct_with_no_fields_still_lowers() {
    let mut tables = empty_tables();
    let name = intern(&mut tables.strings, b"Empty");
    let kind = push_struct_type(&mut tables, name, 0, 1);
    push_struct(&mut tables, name, kind, 0, 1, 0);
    let ast = lower(&tables, &ownership_of(&[]));
    let source = String::from_utf8(render(&ast).expect("the tree must render")).expect("text");
    assert_eq!(source, "pub struct Empty {\n}\n");
}

#[test]
fn the_report_names_every_field_once() {
    let tables = example_tables();
    let analysis = analyze(&tables);
    let report = String::from_utf8(render_report(&tables, &analysis)).expect("text");
    for name in ["Buffer.data", "Header.magic", "Node.label", "Cache.entries"] {
        assert_eq!(
            report.matches(name).count(),
            1,
            "{name} should appear once in {report}"
        );
    }
}

#[test]
fn the_report_covers_an_empty_program() {
    let tables = empty_tables();
    let report = render_report(&tables, &analysis_with(&tables, ownership_of(&[])));
    let text = String::from_utf8(report).expect("text");
    assert!(text.contains("fields: 0"), "{text}");
}

#[test]
fn the_report_says_so_when_allocator_provenance_did_not_settle() {
    let tables = example_tables();
    let mut analysis = analyze(&tables);
    assert!(analysis.provenance.converged);
    let settled = String::from_utf8(render_report(&tables, &analysis)).expect("text");
    assert!(!settled.contains("warning"), "{settled}");

    analysis.provenance.converged = false;
    let unsettled = String::from_utf8(render_report(&tables, &analysis)).expect("text");
    assert!(
        unsettled.contains("allocator provenance did not reach a fixed point"),
        "{unsettled}"
    );
}

#[test]
fn the_report_is_deterministic() {
    let tables = example_tables();
    let analysis = analyze(&tables);
    assert_eq!(
        render_report(&tables, &analysis),
        render_report(&tables, &analysis)
    );
}

#[test]
fn every_ownership_class_has_a_report_word() {
    let classes = [
        OwnershipClass::Value,
        OwnershipClass::Owned,
        OwnershipClass::Borrowed,
        OwnershipClass::Static,
        OwnershipClass::Arena,
        OwnershipClass::Unknown,
    ];
    let mut tables = empty_tables();
    let name = intern(&mut tables.strings, b"Holder");
    let kind = push_struct_type(&mut tables, name, 8, 8);
    let owner = push_struct(&mut tables, name, kind, 8, 8, 0);
    let element = push_integer_type(&mut tables, 8, false);
    let slice = push_slice_type(&mut tables, element);
    for index in 0..classes.len() {
        let field_name = intern(&mut tables.strings, format!("field{index}").as_bytes());
        push_field(&mut tables, owner, field_name, slice, index as u32 * 16);
    }
    let analysis = analysis_with(&tables, ownership_of(&classes));
    let report = render_report(&tables, &analysis);
    let text = String::from_utf8(report).expect("text");
    for word in ["value", "owned", "borrowed", "static", "arena", "unknown"] {
        assert!(text.contains(&format!("class: {word}")), "{text}");
    }
    assert_eq!(owner, StructId(0));
}
