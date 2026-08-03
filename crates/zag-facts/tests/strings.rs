use zag_facts::build::intern;
use zag_facts::tables::{Strings, string_bytes, string_count};
use zag_facts::{NO_INDEX, StringId};

#[test]
fn interning_assigns_sequential_identifiers() {
    let mut strings = Strings::default();
    let first = intern(&mut strings, b"alpha");
    let second = intern(&mut strings, b"beta");
    assert_eq!(first, StringId(0));
    assert_eq!(second, StringId(1));
    assert_eq!(string_count(&strings), 2);
}

#[test]
fn interning_deduplicates_identical_text() {
    let mut strings = Strings::default();
    let first = intern(&mut strings, b"alpha");
    let again = intern(&mut strings, b"alpha");
    assert_eq!(first, again);
    assert_eq!(string_count(&strings), 1);
    assert_eq!(strings.bytes.len(), 5);
}

#[test]
fn interning_round_trips_the_text() {
    let mut strings = Strings::default();
    let identifiers: Vec<StringId> = [b"one".as_slice(), b"two".as_slice(), b"".as_slice()]
        .iter()
        .map(|text| intern(&mut strings, text))
        .collect();
    assert_eq!(string_bytes(&strings, identifiers[0]), b"one");
    assert_eq!(string_bytes(&strings, identifiers[1]), b"two");
    assert_eq!(string_bytes(&strings, identifiers[2]), b"");
}

#[test]
fn out_of_range_identifiers_read_as_empty_rather_than_panicking() {
    let mut strings = Strings::default();
    intern(&mut strings, b"alpha");
    assert_eq!(string_bytes(&strings, StringId(7)), b"");
    assert_eq!(string_bytes(&strings, StringId(NO_INDEX)), b"");
}

#[test]
fn an_empty_table_has_no_strings() {
    let strings = Strings::default();
    assert_eq!(string_count(&strings), 0);
    assert_eq!(string_bytes(&strings, StringId(0)), b"");
}
