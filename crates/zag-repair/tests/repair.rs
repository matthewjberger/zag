use proptest::prelude::*;
use zag_repair::{Diagnostic, DiagnosticCode, Repair, RepairError, apply_repairs, plan_repairs};

fn mismatch(span_start: u32, span_end: u32, expected: &[u8], found: &[u8]) -> Diagnostic {
    Diagnostic {
        code: DiagnosticCode::MismatchedTypes,
        span_start,
        span_end,
        expected: expected.to_vec(),
        found: found.to_vec(),
    }
}

fn repaired(source: &[u8], diagnostics: &[Diagnostic]) -> Vec<u8> {
    apply_repairs(source, &plan_repairs(diagnostics)).expect("the repair must apply")
}

#[test]
fn narrowing_becomes_a_checked_conversion() {
    let source = b"let value: u32 = count;";
    let diagnostics = [mismatch(17, 22, b"u32", b"usize")];
    assert_eq!(
        repaired(source, &diagnostics),
        b"let value: u32 = u32::try_from(count).unwrap();".to_vec()
    );
}

#[test]
fn widening_becomes_an_infallible_conversion() {
    let source = b"let value: u32 = small;";
    let diagnostics = [mismatch(17, 22, b"u32", b"u8")];
    assert_eq!(
        repaired(source, &diagnostics),
        b"let value: u32 = u32::from(small);".to_vec()
    );
}

#[test]
fn a_signedness_change_is_treated_as_narrowing() {
    let source = b"value";
    let diagnostics = [mismatch(0, 5, b"i64", b"u8")];
    assert_eq!(
        repaired(source, &diagnostics),
        b"i64::try_from(value).unwrap()".to_vec()
    );
}

#[test]
fn diagnostics_that_are_not_type_mismatches_are_left_alone() {
    let diagnostic = Diagnostic {
        code: DiagnosticCode::Unhandled,
        span_start: 0,
        span_end: 1,
        expected: b"u32".to_vec(),
        found: b"u8".to_vec(),
    };
    assert_eq!(plan_repairs(&[diagnostic]), Vec::new());
}

#[test]
fn mismatches_between_types_that_are_not_integers_are_left_alone() {
    let diagnostics = [
        mismatch(0, 1, b"String", b"&str"),
        mismatch(0, 1, b"u32", b"&str"),
        mismatch(0, 1, b"u7", b"u8"),
        mismatch(0, 1, b"uphill", b"u8"),
    ];
    assert_eq!(plan_repairs(&diagnostics), Vec::new());
}

#[test]
fn a_mismatch_between_identical_types_is_left_alone() {
    assert_eq!(plan_repairs(&[mismatch(0, 1, b"u32", b"u32")]), Vec::new());
}

#[test]
fn an_inverted_span_is_left_alone() {
    assert_eq!(plan_repairs(&[mismatch(9, 2, b"u32", b"u8")]), Vec::new());
}

#[test]
fn applying_no_repairs_leaves_the_source_alone() {
    let source = b"unchanged";
    assert_eq!(apply_repairs(source, &[]), Ok(source.to_vec()));
}

#[test]
fn several_disjoint_repairs_all_apply() {
    let source = b"f(a, b)";
    let diagnostics = [mismatch(2, 3, b"u32", b"u8"), mismatch(5, 6, b"u32", b"u8")];
    assert_eq!(
        repaired(source, &diagnostics),
        b"f(u32::from(a), u32::from(b))".to_vec()
    );
}

#[test]
fn the_order_repairs_arrive_in_does_not_change_the_result() {
    let source = b"f(a, b)";
    let forwards = [
        Repair {
            span_start: 2,
            span_end: 3,
            prefix: b"<".to_vec(),
            suffix: b">".to_vec(),
        },
        Repair {
            span_start: 5,
            span_end: 6,
            prefix: b"[".to_vec(),
            suffix: b"]".to_vec(),
        },
    ];
    let mut backwards = forwards.clone();
    backwards.reverse();
    assert_eq!(
        apply_repairs(source, &forwards),
        apply_repairs(source, &backwards)
    );
}

#[test]
fn overlapping_repairs_are_refused_rather_than_silently_merged() {
    let source = b"abcdef";
    let repairs = [
        Repair {
            span_start: 0,
            span_end: 4,
            prefix: b"<".to_vec(),
            suffix: b">".to_vec(),
        },
        Repair {
            span_start: 2,
            span_end: 6,
            prefix: b"[".to_vec(),
            suffix: b"]".to_vec(),
        },
    ];
    assert_eq!(
        apply_repairs(source, &repairs),
        Err(RepairError::OverlappingRepairs {
            earlier: 0,
            later: 1
        })
    );
}

#[test]
fn abutting_repairs_are_allowed() {
    let source = b"abcd";
    let repairs = [
        Repair {
            span_start: 0,
            span_end: 2,
            prefix: b"<".to_vec(),
            suffix: b">".to_vec(),
        },
        Repair {
            span_start: 2,
            span_end: 4,
            prefix: b"[".to_vec(),
            suffix: b"]".to_vec(),
        },
    ];
    assert_eq!(apply_repairs(source, &repairs), Ok(b"<ab>[cd]".to_vec()));
}

#[test]
fn a_span_past_the_end_of_the_source_is_refused() {
    let repairs = [Repair {
        span_start: 0,
        span_end: 99,
        prefix: Vec::new(),
        suffix: Vec::new(),
    }];
    assert_eq!(
        apply_repairs(b"short", &repairs),
        Err(RepairError::SpanOutOfRange {
            span_start: 0,
            span_end: 99
        })
    );
}

#[test]
fn an_inverted_span_is_refused_at_apply_time_too() {
    let repairs = [Repair {
        span_start: 4,
        span_end: 1,
        prefix: Vec::new(),
        suffix: Vec::new(),
    }];
    assert!(matches!(
        apply_repairs(b"source", &repairs),
        Err(RepairError::SpanOutOfRange { .. })
    ));
}

#[test]
fn an_empty_span_inserts_without_replacing() {
    let repairs = [Repair {
        span_start: 3,
        span_end: 3,
        prefix: b"|".to_vec(),
        suffix: b"|".to_vec(),
    }];
    assert_eq!(apply_repairs(b"abcdef", &repairs), Ok(b"abc||def".to_vec()));
}

proptest! {
    #[test]
    fn applying_repairs_never_panics(
        source in prop::collection::vec(any::<u8>(), 0..32),
        spans in prop::collection::vec((any::<u32>(), any::<u32>()), 0..8),
    ) {
        let repairs: Vec<Repair> = spans
            .into_iter()
            .map(|(span_start, span_end)| Repair {
                span_start,
                span_end,
                prefix: b"(".to_vec(),
                suffix: b")".to_vec(),
            })
            .collect();
        let _ = apply_repairs(&source, &repairs);
    }

    #[test]
    fn a_repair_preserves_every_byte_it_does_not_span(
        source in prop::collection::vec(any::<u8>(), 1..32),
        start in 0usize..16,
    ) {
        let span_start = start.min(source.len()) as u32;
        let repairs = [Repair {
            span_start,
            span_end: span_start,
            prefix: b"<".to_vec(),
            suffix: b">".to_vec(),
        }];
        let out = apply_repairs(&source, &repairs).expect("an empty span always applies");
        prop_assert_eq!(out.len(), source.len() + 2);
        let filtered: Vec<u8> = out.into_iter().filter(|byte| *byte != b'<' && *byte != b'>').collect();
        let expected: Vec<u8> = source.into_iter().filter(|byte| *byte != b'<' && *byte != b'>').collect();
        prop_assert_eq!(filtered, expected);
    }
}
