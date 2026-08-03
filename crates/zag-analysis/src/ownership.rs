use crate::call_graph::{CallGraph, reachable_from};
use crate::provenance::{AllocatorClass, Provenance, classify_source, join};
use zag_facts::tables::{
    AssignmentSource, MemoryOperationKind, PlaceKind, Tables, field_count, is_reference_type,
    memory_operation_count, struct_count,
};
use zag_facts::{FieldId, FunctionId, NO_INDEX};

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OwnershipClass {
    Value = 0,
    Owned = 1,
    Borrowed = 2,
    Static = 3,
    Arena = 4,
    Unknown = 5,
}

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Confidence {
    High = 0,
    Medium = 1,
    Low = 2,
}

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EvidenceKind {
    FreedInDeinitClosure = 0,
    FreedOutsideDeinitClosure = 1,
    AssignedFromAllocation = 2,
    AssignedFromParameter = 3,
    AssignedFromLiteral = 4,
    AssignedFromUnknown = 5,
    AllocatorIsGlobal = 6,
    AllocatorIsArena = 7,
    AllocatorIsConflicting = 8,
    NoAssignmentsFound = 9,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Ownership {
    pub class: Vec<OwnershipClass>,
    pub confidence: Vec<Confidence>,
    pub evidence_start: Vec<u32>,
    pub evidence_count: Vec<u32>,
    pub evidence_kind: Vec<EvidenceKind>,
    pub evidence_function: Vec<FunctionId>,
}

/// Rows of the memory operation and field assignment tables, grouped by the
/// field they touch. Without this every field would scan both tables in full.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldIndex {
    pub free_start: Vec<u32>,
    pub free_rows: Vec<u32>,
    pub assignment_start: Vec<u32>,
    pub assignment_rows: Vec<u32>,
}

fn group_rows(fields: usize, pairs: &[(u32, u32)]) -> (Vec<u32>, Vec<u32>) {
    let mut counts = vec![0u32; fields];
    for (_, field) in pairs {
        counts[*field as usize] += 1;
    }
    let mut start = vec![0u32; fields + 1];
    for index in 0..fields {
        start[index + 1] = start[index] + counts[index];
    }
    let mut cursor = start.clone();
    let mut grouped = vec![0u32; pairs.len()];
    for (row, field) in pairs {
        grouped[cursor[*field as usize] as usize] = *row;
        cursor[*field as usize] += 1;
    }
    (start, grouped)
}

fn freed_field(tables: &Tables, row: usize) -> Option<u32> {
    let operations = &tables.memory_operations;
    if operations.kind.get(row) != Some(&MemoryOperationKind::Free) {
        return None;
    }
    if operations.place.get(row) != Some(&PlaceKind::FieldOfParameter) {
        return None;
    }
    let field = operations.place_field.get(row)?.0;
    ((field as usize) < field_count(&tables.fields)).then_some(field)
}

pub fn build_field_index(tables: &Tables) -> FieldIndex {
    let fields = field_count(&tables.fields);
    let free_pairs: Vec<(u32, u32)> = (0..memory_operation_count(&tables.memory_operations))
        .filter_map(|row| freed_field(tables, row).map(|field| (row as u32, field)))
        .collect();
    let assignment_pairs: Vec<(u32, u32)> = (0..tables.field_assignments.field.len())
        .filter_map(|row| {
            let field = tables.field_assignments.field[row].0;
            ((field as usize) < fields).then_some((row as u32, field))
        })
        .collect();
    let (free_start, free_rows) = group_rows(fields, &free_pairs);
    let (assignment_start, assignment_rows) = group_rows(fields, &assignment_pairs);
    FieldIndex {
        free_start,
        free_rows,
        assignment_start,
        assignment_rows,
    }
}

#[derive(Clone, Copy, Default)]
struct FreeFacts {
    freed: bool,
    freed_in_deinit_closure: bool,
}

#[derive(Clone, Copy)]
struct AssignmentFacts {
    has_allocation: bool,
    has_parameter: bool,
    has_literal: bool,
    has_unknown: bool,
    allocator: AllocatorClass,
}

/// Deinit closures are computed on demand. Most structs never have a field
/// freed anywhere, so building one for every struct with a deinit allocates a
/// reachability vector per struct that nothing reads.
fn closure_for(
    tables: &Tables,
    graph: &CallGraph,
    closures: &mut [Option<Vec<bool>>],
    owner: usize,
) -> Option<usize> {
    if owner >= closures.len() {
        return None;
    }
    if closures[owner].is_none() {
        let deinit = tables.structs.deinit[owner];
        if deinit.0 == NO_INDEX {
            closures[owner] = Some(Vec::new());
        } else {
            closures[owner] = Some(reachable_from(graph, deinit));
        }
    }
    Some(owner)
}

fn gather_free_facts(
    tables: &Tables,
    graph: &CallGraph,
    closures: &mut [Option<Vec<bool>>],
    index: &FieldIndex,
    field: FieldId,
    ownership: &mut Ownership,
) -> FreeFacts {
    let slot = field.0 as usize;
    let range = index.free_start[slot] as usize..index.free_start[slot + 1] as usize;
    let mut facts = FreeFacts::default();
    if range.is_empty() {
        return facts;
    }
    let owner = tables.fields.owner[slot].0 as usize;
    let resolved = closure_for(tables, graph, closures, owner);
    for slot in range {
        let row = index.free_rows[slot] as usize;
        let function = tables.memory_operations.function[row];
        let inside = resolved
            .and_then(|owner| closures[owner].as_ref())
            .and_then(|reachable| reachable.get(function.0 as usize).copied())
            .unwrap_or(false);
        facts.freed = true;
        facts.freed_in_deinit_closure |= inside;
        ownership.evidence_kind.push(if inside {
            EvidenceKind::FreedInDeinitClosure
        } else {
            EvidenceKind::FreedOutsideDeinitClosure
        });
        ownership.evidence_function.push(function);
    }
    facts
}

fn gather_assignment_facts(
    tables: &Tables,
    provenance: &Provenance,
    index: &FieldIndex,
    field: FieldId,
    ownership: &mut Ownership,
) -> AssignmentFacts {
    let slot = field.0 as usize;
    let mut facts = AssignmentFacts {
        has_allocation: false,
        has_parameter: false,
        has_literal: false,
        has_unknown: false,
        allocator: AllocatorClass::Unset,
    };
    for entry in index.assignment_start[slot] as usize..index.assignment_start[slot + 1] as usize {
        let row = index.assignment_rows[entry] as usize;
        let function = tables.field_assignments.function[row];
        match tables.field_assignments.source[row] {
            AssignmentSource::Allocation => {
                facts.has_allocation = true;
                ownership
                    .evidence_kind
                    .push(EvidenceKind::AssignedFromAllocation);
                let operation = tables.field_assignments.memory_operation[row].0 as usize;
                if operation < tables.memory_operations.allocator.len() {
                    let source = tables.memory_operations.allocator[operation];
                    facts.allocator =
                        join(facts.allocator, classify_source(tables, provenance, source));
                } else {
                    facts.allocator = join(facts.allocator, AllocatorClass::Conflicting);
                }
            }
            AssignmentSource::Parameter => {
                facts.has_parameter = true;
                ownership
                    .evidence_kind
                    .push(EvidenceKind::AssignedFromParameter);
            }
            AssignmentSource::StaticLiteral => {
                facts.has_literal = true;
                ownership
                    .evidence_kind
                    .push(EvidenceKind::AssignedFromLiteral);
            }
            AssignmentSource::Unknown => {
                facts.has_unknown = true;
                ownership
                    .evidence_kind
                    .push(EvidenceKind::AssignedFromUnknown);
            }
        }
        ownership.evidence_function.push(function);
    }
    if facts.has_allocation {
        ownership.evidence_kind.push(match facts.allocator {
            AllocatorClass::Arena => EvidenceKind::AllocatorIsArena,
            AllocatorClass::Global => EvidenceKind::AllocatorIsGlobal,
            _ => EvidenceKind::AllocatorIsConflicting,
        });
        ownership.evidence_function.push(FunctionId(NO_INDEX));
    }
    if !facts.has_allocation && !facts.has_parameter && !facts.has_literal && !facts.has_unknown {
        ownership
            .evidence_kind
            .push(EvidenceKind::NoAssignmentsFound);
        ownership.evidence_function.push(FunctionId(NO_INDEX));
    }
    facts
}

fn only(flag: bool, others: [bool; 3]) -> bool {
    flag && !others[0] && !others[1] && !others[2]
}

fn decide(free: FreeFacts, assignment: AssignmentFacts) -> (OwnershipClass, Confidence) {
    let AssignmentFacts {
        has_allocation,
        has_parameter,
        has_literal,
        has_unknown,
        allocator,
    } = assignment;
    let allocation_only = only(has_allocation, [has_parameter, has_literal, has_unknown]);
    if has_allocation && allocator == AllocatorClass::Arena {
        let confidence = if allocation_only && !free.freed {
            Confidence::High
        } else {
            Confidence::Medium
        };
        return (OwnershipClass::Arena, confidence);
    }
    if free.freed && allocation_only && allocator == AllocatorClass::Global {
        let confidence = if free.freed_in_deinit_closure {
            Confidence::High
        } else {
            Confidence::Medium
        };
        return (OwnershipClass::Owned, confidence);
    }
    if !free.freed && only(has_literal, [has_allocation, has_parameter, has_unknown]) {
        return (OwnershipClass::Static, Confidence::High);
    }
    if !free.freed && only(has_parameter, [has_allocation, has_literal, has_unknown]) {
        return (OwnershipClass::Borrowed, Confidence::Medium);
    }
    (OwnershipClass::Unknown, Confidence::Low)
}

pub fn classify_ownership(
    tables: &Tables,
    graph: &CallGraph,
    provenance: &Provenance,
) -> Ownership {
    let index = build_field_index(tables);
    let mut closures: Vec<Option<Vec<bool>>> = vec![None; struct_count(&tables.structs)];
    let mut ownership = Ownership::default();
    for row in 0..field_count(&tables.fields) {
        let field = FieldId(row as u32);
        let start = ownership.evidence_kind.len() as u32;
        let (class, confidence) = if is_reference_type(&tables.types, tables.fields.field_type[row])
        {
            let free =
                gather_free_facts(tables, graph, &mut closures, &index, field, &mut ownership);
            let assignment =
                gather_assignment_facts(tables, provenance, &index, field, &mut ownership);
            decide(free, assignment)
        } else {
            (OwnershipClass::Value, Confidence::High)
        };
        ownership.class.push(class);
        ownership.confidence.push(confidence);
        ownership.evidence_start.push(start);
        ownership
            .evidence_count
            .push(ownership.evidence_kind.len() as u32 - start);
    }
    ownership
}

pub fn field_evidence(ownership: &Ownership, field: FieldId) -> std::ops::Range<usize> {
    let index = field.0 as usize;
    if index >= ownership.evidence_start.len() {
        return 0..0;
    }
    let start = ownership.evidence_start[index] as usize;
    let count = ownership.evidence_count[index] as usize;
    start..start + count
}
