use zag_analysis::ownership::{
    Confidence, EvidenceKind, Ownership, OwnershipClass, field_evidence,
};
use zag_facts::tables::{Tables, field_count, string_bytes};
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
    let owner = tables.fields.owner[row].0 as usize;
    let owner_name = string_bytes(&tables.strings, tables.structs.name[owner]);
    let field_name = string_bytes(&tables.strings, tables.fields.name[row]);
    write_line(out, &[owner_name, b".", field_name]);
    write_line(out, &[b"  class: ", class_text(ownership.class[row])]);
    write_line(
        out,
        &[
            b"  confidence: ",
            confidence_text(ownership.confidence[row]),
        ],
    );
    for slot in field_evidence(ownership, FieldId(row as u32)) {
        let text = evidence_text(ownership.evidence_kind[slot]);
        match function_name(tables, ownership.evidence_function[slot]) {
            Some(name) => write_line(out, &[b"  evidence: ", text, b" (", name, b")"]),
            None => write_line(out, &[b"  evidence: ", text]),
        }
    }
}

pub fn render_report(tables: &Tables, ownership: &Ownership) -> Vec<u8> {
    let mut out = Vec::new();
    write_line(&mut out, &[b"zag ownership report"]);
    write_line(
        &mut out,
        &[b"target: ", string_bytes(&tables.strings, tables.target)],
    );
    let fields = field_count(&tables.fields);
    write_line(&mut out, &[b"fields: ", fields.to_string().as_bytes()]);
    for row in 0..fields {
        out.push(b'\n');
        write_field(&mut out, tables, ownership, row);
    }
    out
}
