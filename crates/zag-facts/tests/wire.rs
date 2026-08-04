use proptest::prelude::*;
use zag_facts::fixture::example_tables;
use zag_facts::tables::{
    AllocatorSourceKind, MemoryOperationKind, PlaceKind, Tables, TypeKind, empty_tables,
};
use zag_facts::wire::{DecodeError, MAGIC, VERSION, decode, encode};
use zag_facts::{AllocatorSourceId, FieldId, FunctionId, StringId, TypeId};

#[test]
fn an_empty_table_set_round_trips() {
    let tables = empty_tables();
    let bytes = encode(&tables);
    assert_eq!(decode(&bytes), Ok(tables));
}

#[test]
fn the_fixture_round_trips() {
    let tables = example_tables();
    let bytes = encode(&tables);
    assert_eq!(decode(&bytes), Ok(tables));
}

#[test]
fn encoding_is_deterministic() {
    assert_eq!(encode(&example_tables()), encode(&example_tables()));
}

#[test]
fn a_wrong_magic_is_rejected() {
    let mut bytes = encode(&example_tables());
    bytes[0] = b'X';
    assert_eq!(decode(&bytes), Err(DecodeError::BadMagic));
}

#[test]
fn an_empty_buffer_is_rejected_without_panicking() {
    assert_eq!(decode(&[]), Err(DecodeError::BadMagic));
}

#[test]
fn a_future_version_is_rejected() {
    let mut bytes = encode(&example_tables());
    let next = VERSION + 1;
    bytes[MAGIC.len()..MAGIC.len() + 4].copy_from_slice(&next.to_le_bytes());
    assert_eq!(decode(&bytes), Err(DecodeError::UnsupportedVersion(next)));
}

#[test]
fn a_truncated_buffer_is_rejected_without_panicking() {
    let bytes = encode(&example_tables());
    for length in MAGIC.len()..bytes.len() {
        let outcome = decode(&bytes[..length]);
        assert!(
            matches!(outcome, Err(DecodeError::Truncated { .. })),
            "truncating to {length} bytes should report truncation, got {outcome:?}"
        );
    }
}

#[test]
fn trailing_bytes_are_rejected() {
    let mut bytes = encode(&example_tables());
    bytes.push(0);
    assert!(matches!(
        decode(&bytes),
        Err(DecodeError::TrailingBytes { .. })
    ));
}

#[test]
fn an_unknown_enumeration_value_is_rejected() {
    let mut tables = empty_tables();
    tables.types.kind.push(TypeKind::Integer);
    tables.types.element.push(TypeId(0));
    tables.types.count.push(0);
    tables.types.module.push(zag_facts::tables::ROOT_MODULE);
    tables.types.name.push(StringId(0));
    tables.types.size.push(4);
    tables.types.alignment.push(4);
    tables.types.bit_width.push(32);
    tables.types.flags.push(0);
    let mut bytes = encode(&tables);
    let position = find_first_type_kind_offset(&bytes);
    bytes[position..position + 4].copy_from_slice(&99u32.to_le_bytes());
    assert_eq!(
        decode(&bytes),
        Err(DecodeError::UnknownEnumValue {
            column: "types.kind",
            value: 99
        })
    );
}

/// Walks the header and every column ahead of the type table, so a column
/// added in front of it moves this rather than silently pointing at the wrong
/// one. The nine that precede it are the module table, its unresolved imports,
/// and the artifacts, in the order `encode` writes them.
fn find_first_type_kind_offset(bytes: &[u8]) -> usize {
    let mut cursor = MAGIC.len() + 4 + 4;
    let string_byte_count = read_length(bytes, cursor);
    cursor += 4 + string_byte_count;
    for _ in 0..10 {
        let count = read_length(bytes, cursor);
        cursor += 4 + count * 4;
    }
    cursor + 4
}

fn read_length(bytes: &[u8], at: usize) -> usize {
    let mut raw = [0u8; 4];
    raw.copy_from_slice(&bytes[at..at + 4]);
    u32::from_le_bytes(raw) as usize
}

fn arbitrary_words() -> impl Strategy<Value = Vec<u32>> {
    prop::collection::vec(any::<u32>(), 0..24)
}

fn arbitrary_tables() -> impl Strategy<Value = Tables> {
    (
        prop::collection::vec(any::<u8>(), 0..48),
        arbitrary_words(),
        prop::collection::vec(0u32..7, 0..12),
        arbitrary_words(),
        prop::collection::vec(0u32..4, 0..12),
        prop::collection::vec(0u32..3, 0..12),
        prop::collection::vec(0u32..3, 0..12),
        any::<u32>(),
    )
        .prop_map(
            |(
                string_bytes,
                string_offsets,
                type_kinds,
                sizes,
                allocator_kinds,
                operation_kinds,
                place_kinds,
                target,
            )| {
                let mut tables = empty_tables();
                tables.target = StringId(target);
                tables.strings.bytes = string_bytes;
                tables.strings.offsets = string_offsets;
                tables.types.kind = type_kinds.into_iter().map(type_kind_from_index).collect();
                tables.types.size = sizes;
                tables.allocator_sources.kind = allocator_kinds
                    .into_iter()
                    .map(allocator_kind_from_index)
                    .collect();
                tables.memory_operations.kind = operation_kinds
                    .into_iter()
                    .map(operation_kind_from_index)
                    .collect();
                tables.memory_operations.place =
                    place_kinds.into_iter().map(place_kind_from_index).collect();
                tables.memory_operations.place_field = vec![FieldId(3)];
                tables.memory_operations.allocator = vec![AllocatorSourceId(1)];
                tables.memory_operations.function = vec![FunctionId(2)];
                tables
            },
        )
}

fn type_kind_from_index(index: u32) -> TypeKind {
    match index {
        0 => TypeKind::Void,
        1 => TypeKind::Integer,
        2 => TypeKind::Bool,
        3 => TypeKind::Slice,
        4 => TypeKind::Pointer,
        5 => TypeKind::Struct,
        _ => TypeKind::Opaque,
    }
}

fn allocator_kind_from_index(index: u32) -> AllocatorSourceKind {
    match index {
        0 => AllocatorSourceKind::Global,
        1 => AllocatorSourceKind::Arena,
        2 => AllocatorSourceKind::Parameter,
        _ => AllocatorSourceKind::Unknown,
    }
}

fn operation_kind_from_index(index: u32) -> MemoryOperationKind {
    match index {
        0 => MemoryOperationKind::Allocate,
        1 => MemoryOperationKind::Free,
        _ => MemoryOperationKind::Resize,
    }
}

fn place_kind_from_index(index: u32) -> PlaceKind {
    match index {
        0 => PlaceKind::FieldOfParameter,
        1 => PlaceKind::Local,
        _ => PlaceKind::Unknown,
    }
}

proptest! {
    #[test]
    fn every_table_set_round_trips(tables in arbitrary_tables()) {
        let bytes = encode(&tables);
        prop_assert_eq!(decode(&bytes), Ok(tables));
    }

    #[test]
    fn decoding_never_panics_on_arbitrary_input(bytes in prop::collection::vec(any::<u8>(), 0..256)) {
        let _ = decode(&bytes);
    }
}
