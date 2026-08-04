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
    /// Signature and body both, which is the whole function.
    Ported,
    Signature,
    /// Nothing was written, and the reason names what would have to change.
    NotPorted(crate::function::Refusal),
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
fn frees_and_nothing_else(
    tables: &Tables,
    ownership: &Ownership,
    index: &crate::index::Index,
    function: FunctionId,
) -> bool {
    if crate::index::assigns_anything(index, function) {
        return false;
    }
    let operations = &tables.memory_operations;
    let rows = crate::index::operations_of(index, function);
    for row in rows {
        let row = *row as usize;
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
    }
    !rows.is_empty()
}

pub fn disposition(
    tables: &Tables,
    ownership: &Ownership,
    lowering: crate::lower::Lowering,
    function: FunctionId,
) -> Disposition {
    if let Some(owner) = owner_of(tables, function)
        && crate::constructor::writable_init(tables, ownership, lowering, owner) == Some(function)
    {
        return Disposition::Constructor;
    }
    if declared_deinit_disappears(tables, ownership, function)
        || frees_and_nothing_else(tables, ownership, lowering.index, function)
    {
        return Disposition::SubsumedByDrop;
    }
    if let Some(refusal) = crate::function::signature_refusal(tables, ownership, lowering, function)
    {
        return Disposition::NotPorted(refusal);
    }
    match tables
        .functions
        .body
        .get(function.0 as usize)
        .copied()
        .filter(|body| body.0 != NO_INDEX)
        .is_some_and(|body| crate::body::is_spellable(tables, body, 0))
    {
        true => Disposition::Ported,
        false => Disposition::Signature,
    }
}

/// What the report says about an outcome. Exposed so the guide can be checked
/// against the wording the tool actually writes rather than a copy of it.
pub fn outcome_text(disposition: Disposition) -> &'static [u8] {
    match disposition {
        Disposition::Constructor => b"ported, as the constructor",
        Disposition::SubsumedByDrop => b"disappears, Drop frees what it freed",
        Disposition::Ported => b"ported, signature and body",
        Disposition::Signature => b"ported, signature only, the body is still to write",
        Disposition::NotPorted(refusal) => refusal_text(refusal),
    }
}

fn refusal_text(refusal: crate::function::Refusal) -> &'static [u8] {
    use crate::function::Refusal;
    match refusal {
        Refusal::ReturnTypeUnresolved => b"still to write, what it returns did not resolve",
        Refusal::UnnamedErrorSet => b"still to write, the error set it can fail with has no name",
        Refusal::ReturnBorrowsAnArena => {
            b"still to write, what it returns borrows from an arena the port drops"
        }
        Refusal::ReturnBorrowsWithNothingToTieItTo => {
            b"still to write, what it returns borrows and no parameter can carry the lifetime"
        }
    }
}

/// ` (main.zig:18)`, or nothing when the tables recorded no location. A line
/// with no file to go with it would send a reader nowhere, so both or neither.
fn located(tables: &Tables, function: FunctionId, line: u32) -> Vec<u8> {
    let Some((path, line)) = zag_facts::tables::function_location(tables, function, line) else {
        return Vec::new();
    };
    let mut out = b" (".to_vec();
    out.extend_from_slice(path);
    out.push(b':');
    out.extend_from_slice(line.to_string().as_bytes());
    out.push(b')');
    out
}

fn field_name(tables: &Tables, field: FieldId) -> &[u8] {
    tables
        .fields
        .name
        .get(field.0 as usize)
        .map(|name| string_bytes(&tables.strings, *name))
        .unwrap_or(b"")
}

/// Why the struct this `init` belongs to got no constructor. The reason names
/// the field that stopped it, because that is what somebody writing the body
/// has to deal with first.
fn write_constructor_refusal(
    out: &mut Vec<u8>,
    tables: &Tables,
    ownership: &Ownership,
    lowering: crate::lower::Lowering,
    function: FunctionId,
) {
    use crate::constructor::Refusal;
    let Some(owner) = owner_of(tables, function) else {
        return;
    };
    let Err(refusal) = crate::constructor::constructor_for(tables, ownership, lowering, owner)
    else {
        return;
    };
    match refusal {
        // Neither is about this function: a struct with no init of its own, or
        // an init that fills nothing, has nothing to explain here.
        Refusal::NoInit | Refusal::NoFields => {}
        // The Zig may well assign it. What the tables say is that nothing the
        // frontend could read does, which is a different claim and the one
        // worth making.
        Refusal::NothingAssigns(field) => write_line(
            out,
            &[
                b"    no constructor: nothing the port could read assigns ",
                field_name(tables, field),
            ],
        ),
        Refusal::NotSpellable(field) => {
            let at = located(
                tables,
                function,
                crate::constructor::unspellable_line(tables, ownership, lowering, owner),
            );
            match crate::constructor::unspellable_text(tables, ownership, lowering, owner) {
                Some(text) => write_line(
                    out,
                    &[
                        b"    no constructor: ",
                        field_name(tables, field),
                        b" is set from ",
                        &text,
                        b", which the port cannot spell",
                        &at,
                    ],
                ),
                None => write_line(
                    out,
                    &[
                        b"    no constructor: ",
                        field_name(tables, field),
                        b" is set from something the port cannot spell",
                    ],
                ),
            }
        }
        Refusal::OwnershipUnknown(field) => write_line(
            out,
            &[
                b"    no constructor: who owns ",
                field_name(tables, field),
                b" was not decided",
            ],
        ),
    }
}

fn class_text_of(class: zag_analysis::provenance::AllocatorClass) -> &'static [u8] {
    use zag_analysis::provenance::AllocatorClass;
    match class {
        AllocatorClass::Unset => b"nothing",
        AllocatorClass::Global => b"the global allocator",
        AllocatorClass::Arena => b"an arena",
        AllocatorClass::Conflicting => b"more than one allocator",
    }
}

/// Which callers disagreed about an allocator parameter. The field section
/// says a field's allocator does not resolve; this says who to go and look at.
fn write_disagreements(out: &mut Vec<u8>, tables: &Tables, analysis: &Analysis) {
    let disagreements = &analysis.provenance.disagreements;
    if disagreements.is_empty() {
        return;
    }
    out.push(b'\n');
    write_line(
        out,
        &[
            b"allocator conflicts: ",
            disagreements.len().to_string().as_bytes(),
        ],
    );
    for entry in disagreements {
        let holder = tables
            .parameters
            .owner
            .get(entry.parameter as usize)
            .copied()
            .unwrap_or(FunctionId(NO_INDEX));
        let parameter = tables
            .parameters
            .name
            .get(entry.parameter as usize)
            .map(|name| string_bytes(&tables.strings, *name))
            .unwrap_or(b"");
        write_line(
            out,
            &[
                b"  ",
                function_name(tables, holder).unwrap_or(b""),
                b" takes ",
                parameter,
                b" from callers that disagree",
            ],
        );
        for (row, class) in [
            (entry.first, entry.first_class),
            (entry.second, entry.second_class),
        ] {
            let caller = tables
                .call_arguments
                .call
                .get(row as usize)
                .and_then(|call| tables.calls.caller.get(call.0 as usize))
                .copied()
                .unwrap_or(FunctionId(NO_INDEX));
            match function_name(tables, caller) {
                Some(name) => write_line(out, &[b"    from ", name, b": ", class_text_of(class)]),
                None => write_line(
                    out,
                    &[b"    from an unknown caller: ", class_text_of(class)],
                ),
            }
        }
    }
}

fn write_functions(out: &mut Vec<u8>, tables: &Tables, ownership: &Ownership) {
    let count = zag_facts::tables::function_count(&tables.functions);
    if count == 0 {
        return;
    }
    let lifetimes = crate::lower::lifetimes_by_type(tables, ownership);
    let lookups = crate::index::build_index(tables);
    let qualified = zag_facts::tables::has_submodules(&tables.modules);
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
        let module = tables
            .functions
            .module
            .get(index)
            .copied()
            .unwrap_or(zag_facts::tables::ROOT_MODULE);
        let lowering = crate::lower::lowering(&lifetimes, &lookups, module, qualified);
        let outcome = outcome_text(disposition(tables, ownership, lowering, handle));
        let at = located(
            tables,
            handle,
            tables.functions.line.get(index).copied().unwrap_or(0),
        );
        if owner.is_empty() {
            write_line(out, &[b"  ", name, b": ", outcome, &at]);
        } else {
            write_line(out, &[b"  ", owner, b".", name, b": ", outcome, &at]);
        }
        if name == b"init" {
            write_constructor_refusal(out, tables, ownership, lowering, handle);
        }
    }
}

/// Which files the program was read from, and which `@import` reached nothing.
/// An unresolved import means declarations the analysis never saw, so it is
/// said out loud rather than left for a reader to notice from what is missing.
fn write_artifacts(out: &mut Vec<u8>, tables: &Tables) {
    let count = zag_facts::tables::artifact_count(&tables.artifacts);
    if count == 0 {
        return;
    }
    write_line(out, &[b"artifacts: ", count.to_string().as_bytes()]);
    for row in 0..count {
        let name = tables
            .artifacts
            .name
            .get(row)
            .map(|name| string_bytes(&tables.strings, *name))
            .unwrap_or(b"");
        let kind = match tables.artifacts.kind.get(row) {
            Some(zag_facts::tables::ArtifactKind::Executable) => &b"executable"[..],
            Some(zag_facts::tables::ArtifactKind::Library) => b"library",
            Some(zag_facts::tables::ArtifactKind::Test) => b"test",
            None => b"unknown",
        };
        let root = tables
            .artifacts
            .root
            .get(row)
            .filter(|root| root.0 != zag_facts::NO_INDEX)
            .and_then(|root| tables.modules.path.get(root.0 as usize))
            .map(|path| string_bytes(&tables.strings, *path));
        match root {
            Some(path) => write_line(out, &[b"  ", kind, b" ", name, b": ", path]),
            None => write_line(
                out,
                &[
                    b"  ",
                    kind,
                    b" ",
                    name,
                    b": the build script names a root source file the crawl could not open",
                ],
            ),
        }
    }
}

fn write_modules(out: &mut Vec<u8>, tables: &Tables) {
    write_artifacts(out, tables);
    let count = zag_facts::tables::module_count(&tables.modules);
    if count <= 1 && tables.unresolved_imports.owner.is_empty() {
        return;
    }
    // A project read through its build script has no root file, so the top
    // level namespace is an empty module with nothing to say about itself.
    let listed: Vec<usize> = (0..count)
        .filter(|index| {
            tables
                .modules
                .path
                .get(*index)
                .is_some_and(|path| !string_bytes(&tables.strings, *path).is_empty())
        })
        .collect();
    write_line(out, &[b"modules: ", listed.len().to_string().as_bytes()]);
    for index in listed {
        let path = tables
            .modules
            .path
            .get(index)
            .map(|path| string_bytes(&tables.strings, *path))
            .unwrap_or(b"");
        write_line(out, &[b"  ", path]);
        for slot in
            zag_facts::tables::module_unresolved(&tables.modules, zag_facts::ModuleId(index as u32))
        {
            let Some(name) = tables.unresolved_imports.name.get(slot) else {
                continue;
            };
            write_line(
                out,
                &[
                    b"    unresolved import: ",
                    string_bytes(&tables.strings, *name),
                ],
            );
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
    write_modules(&mut out, tables);
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
    write_disagreements(&mut out, tables, analysis);
    out
}
