use zag::{PipelineError, generate, generate_from_bytes};
use zag_facts::fixture::example_tables;
use zag_facts::tables::empty_tables;
use zag_facts::wire::{DecodeError, encode};

const EXPECTED_SOURCE: &str = include_str!("../../../fixtures/expected/example.rs");
const EXPECTED_REPORT: &str = include_str!("../../../fixtures/expected/example.report.txt");

fn generated() -> zag_emit::Output {
    generate(&example_tables()).expect("the fixture must run through the whole pipeline")
}

#[test]
fn the_generated_source_matches_what_is_checked_in() {
    let source = String::from_utf8(generated().source).expect("the output is text");
    assert_eq!(
        source,
        EXPECTED_SOURCE.replace("\r\n", "\n"),
        "regenerate with `just regenerate` if this change is intended"
    );
}

#[test]
fn the_generated_report_matches_what_is_checked_in() {
    let report = String::from_utf8(generated().report).expect("the output is text");
    assert_eq!(
        report,
        EXPECTED_REPORT.replace("\r\n", "\n"),
        "regenerate with `just regenerate` if this change is intended"
    );
}

#[test]
fn the_pipeline_survives_the_wire_format() {
    let direct = generated();
    let round_tripped = generate_from_bytes(&encode(&example_tables()))
        .expect("encoding and decoding must not change the result");
    assert_eq!(direct, round_tripped);
}

#[test]
fn the_pipeline_is_deterministic() {
    assert_eq!(generated(), generated());
}

#[test]
fn an_empty_program_produces_empty_output() {
    let output = generate(&empty_tables()).expect("an empty program is valid");
    assert_eq!(output.source, b"");
}

#[test]
fn malformed_tables_are_refused_rather_than_ported() {
    let mut tables = example_tables();
    tables.fields.owner[0] = zag_facts::StructId(99);
    assert!(matches!(
        generate(&tables),
        Err(PipelineError::InvalidTables(_))
    ));
}

#[test]
fn a_corrupt_fact_file_is_refused() {
    assert_eq!(
        generate_from_bytes(b"not a fact file"),
        Err(PipelineError::Decode(DecodeError::BadMagic))
    );
}

#[test]
fn every_error_has_a_message() {
    let errors = [
        PipelineError::Decode(DecodeError::BadMagic),
        PipelineError::InvalidTables(Vec::new()),
        PipelineError::Render(zag_render::RenderError::MissingRoot),
    ];
    for error in errors {
        assert!(!zag::describe(&error).is_empty());
    }
}
