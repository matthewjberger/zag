use zag_analysis::Analysis;
use zag_analysis::ownership::{
    Confidence, EvidenceKind, Ownership, OwnershipClass, field_evidence,
};
use zag_facts::tables::{MemoryOperationKind, Tables, field_count, string_bytes};
use zag_facts::{FieldId, FunctionId, NO_INDEX};

fn class_text(class: OwnershipClass) -> &'static [u8] {
    match class {
        OwnershipClass::Value => b"value",
        OwnershipClass::Owned => b"owned",
        OwnershipClass::Borrowed => b"borrowed",
        OwnershipClass::Static => b"static",
        OwnershipClass::Arena => b"arena",
        OwnershipClass::Unknown => b"unknown",
    }
}

fn confidence_text(confidence: Confidence) -> &'static [u8] {
    match confidence {
        Confidence::High => b"high",
        Confidence::Medium => b"medium",
        Confidence::Low => b"low",
    }
}

fn evidence_text(kind: EvidenceKind) -> &'static [u8] {
    match kind {
        EvidenceKind::FreedInDeinitClosure => b"freed inside the deinit call closure",
        EvidenceKind::FreedOutsideDeinitClosure => b"freed outside the deinit call closure",
        EvidenceKind::AssignedFromAllocation => b"assigned from an allocation",
        EvidenceKind::AssignedFromParameter => b"assigned from a parameter the caller retains",
        EvidenceKind::AssignedFromLiteral => b"assigned from a static literal",
        EvidenceKind::AssignedFromUnknown => b"assigned from an unrecognised source",
        EvidenceKind::AllocatorIsGlobal => b"allocator resolves to the global allocator",
        EvidenceKind::AllocatorIsArena => b"allocator resolves to an arena",
        EvidenceKind::AllocatorIsConflicting => b"allocator does not resolve to one allocator",
        EvidenceKind::NoAssignmentsFound => b"no assignment to this field was found",
    }
}

fn write_line(out: &mut Vec<u8>, parts: &[&[u8]]) {
    for part in parts {
        out.extend_from_slice(part);
    }
    out.push(b'\n');
}

fn function_name(tables: &Tables, function: FunctionId) -> Option<&[u8]> {
    if function.0 == NO_INDEX || function.0 as usize >= tables.functions.name.len() {
        return None;
    }
    Some(string_bytes(
        &tables.strings,
        tables.functions.name[function.0 as usize],
    ))
}

fn write_field(out: &mut Vec<u8>, tables: &Tables, ownership: &Ownership, row: usize) {
    let owner = tables
        .fields
        .owner
        .get(row)
        .map(|owner| owner.0 as usize)
        .unwrap_or(usize::MAX);
    let owner_name = tables
        .structs
        .name
        .get(owner)
        .map(|name| string_bytes(&tables.strings, *name))
        .unwrap_or(b"");
    let field_name = tables
        .fields
        .name
        .get(row)
        .map(|name| string_bytes(&tables.strings, *name))
        .unwrap_or(b"");
    let (Some(&class), Some(&confidence)) =
        (ownership.class.get(row), ownership.confidence.get(row))
    else {
        return;
    };
    out.push(b'\n');
    write_line(out, &[owner_name, b".", field_name]);
    write_line(out, &[b"  class: ", class_text(class)]);
    write_line(out, &[b"  confidence: ", confidence_text(confidence)]);
    for slot in field_evidence(ownership, FieldId(row as u32)) {
        let Some(&kind) = ownership.evidence_kind.get(slot) else {
            continue;
        };
        let text = evidence_text(kind);
        let function = ownership
            .evidence_function
            .get(slot)
            .copied()
            .unwrap_or(FunctionId(NO_INDEX));
        match function_name(tables, function) {
            Some(name) => write_line(out, &[b"  evidence: ", text, b" (", name, b")"]),
            None => write_line(out, &[b"  evidence: ", text]),
        }
    }
}

/// What became of a Zig function. A reader needs this as much as the field
/// classes, because it is the difference between work already done, work that
/// disappears, and work still to do.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Disposition {
    Constructor,
    SubsumedByDrop,
    Signature,
    NotPorted,
}

fn owner_of(tables: &Tables, function: FunctionId) -> Option<zag_facts::StructId> {
    tables
        .functions
        .owner
        .get(function.0 as usize)
        .copied()
        .filter(|owner| owner.0 != NO_INDEX)
}

/// A deinit disappears when every field it frees is one `Box` already drops.
/// A deinit that does anything else has to be read by a person.
fn declared_deinit_disappears(
    tables: &Tables,
    ownership: &Ownership,
    function: FunctionId,
) -> bool {
    let Some(owner) = owner_of(tables, function) else {
        return false;
    };
    if tables.structs.deinit.get(owner.0 as usize).copied() != Some(function) {
        return false;
    }
    let mut freed = 0;
    for row in zag_facts::tables::struct_fields(&tables.structs, owner) {
        let frees = field_evidence(ownership, FieldId(row as u32)).any(|slot| {
            ownership.evidence_kind.get(slot) == Some(&EvidenceKind::FreedInDeinitClosure)
        });
        if !frees {
            continue;
        }
        freed += 1;
        if ownership.class.get(row) != Some(&OwnershipClass::Owned) {
            return false;
        }
    }
    freed > 0
}

/// A helper the deinit calls disappears on the same grounds the deinit does.
/// Its whole effect is to free fields `Drop` now frees, so writing it out
/// would leave a function with nothing left to do.
fn frees_and_nothing_else(tables: &Tables, ownership: &Ownership, function: FunctionId) -> bool {
    if tables.field_assignments.function.contains(&function) {
        return false;
    }
    let operations = &tables.memory_operations;
    let mut freed = 0;
    for row in 0..zag_facts::tables::memory_operation_count(operations) {
        if operations.function.get(row).copied() != Some(function) {
            continue;
        }
        if operations.kind.get(row) != Some(&MemoryOperationKind::Free) {
            return false;
        }
        let field = operations
            .place_field
            .get(row)
            .copied()
            .unwrap_or(FieldId(NO_INDEX));
        if ownership.class.get(field.0 as usize) != Some(&OwnershipClass::Owned) {
            return false;
        }
        freed += 1;
    }
    freed > 0
}

pub fn disposition(
    tables: &Tables,
    ownership: &Ownership,
    lifetimes: &crate::lower::Lowering,
    function: FunctionId,
) -> Disposition {
    if let Some(owner) = owner_of(tables, function)
        && crate::constructor::writable_init(tables, ownership, owner) == Some(function)
    {
        return Disposition::Constructor;
    }
    if declared_deinit_disappears(tables, ownership, function)
        || frees_and_nothing_else(tables, ownership, function)
    {
        return Disposition::SubsumedByDrop;
    }
    if crate::function::writes_a_signature(tables, ownership, lifetimes, function) {
        return Disposition::Signature;
    }
    Disposition::NotPorted
}

fn write_functions(out: &mut Vec<u8>, tables: &Tables, ownership: &Ownership) {
    let count = zag_facts::tables::function_count(&tables.functions);
    if count == 0 {
        return;
    }
    let lifetimes = crate::lower::lifetimes_by_type(tables, ownership);
    out.push(b'\n');
    write_line(out, &[b"functions: ", count.to_string().as_bytes()]);
    for index in 0..count {
        let handle = FunctionId(index as u32);
        let name = tables
            .functions
            .name
            .get(index)
            .map(|name| string_bytes(&tables.strings, *name))
            .unwrap_or(b"");
        let owner = owner_of(tables, handle)
            .and_then(|owner| tables.structs.name.get(owner.0 as usize))
            .map(|name| string_bytes(&tables.strings, *name))
            .unwrap_or(b"");
        let outcome: &[u8] = match disposition(tables, ownership, &lifetimes, handle) {
            Disposition::Constructor => b"ported, as the constructor",
            Disposition::SubsumedByDrop => b"disappears, Drop frees what it freed",
            Disposition::Signature => b"ported, signature only, the body is still to write",
            Disposition::NotPorted => b"still to write, the port cannot spell what it gives back",
        };
        if owner.is_empty() {
            write_line(out, &[b"  ", name, b": ", outcome]);
        } else {
            write_line(out, &[b"  ", owner, b".", name, b": ", outcome]);
        }
    }
}

pub fn render_report(tables: &Tables, analysis: &Analysis) -> Vec<u8> {
    let mut out = Vec::new();
    write_line(&mut out, &[b"zag ownership report"]);
    write_line(
        &mut out,
        &[b"target: ", string_bytes(&tables.strings, tables.target)],
    );
    let fields = field_count(&tables.fields);
    write_line(&mut out, &[b"fields: ", fields.to_string().as_bytes()]);
    if !analysis.provenance.converged {
        write_line(
            &mut out,
            &[b"warning: allocator provenance did not reach a fixed point, so every"],
        );
        write_line(&mut out, &[b"         allocator below may be understated"]);
    }
    for row in 0..fields {
        write_field(&mut out, tables, &analysis.ownership, row);
    }
    write_functions(&mut out, tables, &analysis.ownership);
    out
}
