use zag_facts::tables::{AllocatorSourceKind, Tables, parameter_count};
use zag_facts::{AllocatorSourceId, NO_INDEX};

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AllocatorClass {
    Unset = 0,
    Global = 1,
    Arena = 2,
    Conflicting = 3,
}

/// Two call sites that handed one allocator parameter different allocators.
/// The classes alone say a parameter is conflicting; this says which callers
/// disagreed, which is the part somebody can go and fix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Disagreement {
    /// The row in `parameters` the two callers filled.
    pub parameter: u32,
    /// The `call_arguments` rows that contributed, in the order they were seen.
    pub first: u32,
    pub second: u32,
    pub first_class: AllocatorClass,
    pub second_class: AllocatorClass,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Provenance {
    pub parameter_class: Vec<AllocatorClass>,
    /// Recorded the first time a parameter goes conflicting, so each pair
    /// appears once however many rounds the fixed point takes.
    pub disagreements: Vec<Disagreement>,
    pub converged: bool,
}

pub fn join(left: AllocatorClass, right: AllocatorClass) -> AllocatorClass {
    match (left, right) {
        (AllocatorClass::Unset, other) => other,
        (other, AllocatorClass::Unset) => other,
        (AllocatorClass::Conflicting, _) => AllocatorClass::Conflicting,
        (_, AllocatorClass::Conflicting) => AllocatorClass::Conflicting,
        (AllocatorClass::Global, AllocatorClass::Global) => AllocatorClass::Global,
        (AllocatorClass::Arena, AllocatorClass::Arena) => AllocatorClass::Arena,
        _ => AllocatorClass::Conflicting,
    }
}

fn global_parameter_index(tables: &Tables, function: u32, parameter_index: u32) -> Option<usize> {
    if function == NO_INDEX || parameter_index == NO_INDEX {
        return None;
    }
    let function = function as usize;
    let start = *tables.functions.parameter_start.get(function)?;
    let count = *tables.functions.parameter_count.get(function)?;
    if parameter_index >= count {
        return None;
    }
    let slot = start as usize + parameter_index as usize;
    (slot < parameter_count(&tables.parameters)).then_some(slot)
}

fn class_of_source(
    tables: &Tables,
    parameter_class: &[AllocatorClass],
    source: AllocatorSourceId,
) -> AllocatorClass {
    let index = source.0 as usize;
    if index >= tables.allocator_sources.kind.len() {
        return AllocatorClass::Conflicting;
    }
    match tables.allocator_sources.kind[index] {
        AllocatorSourceKind::Global => AllocatorClass::Global,
        AllocatorSourceKind::Arena => AllocatorClass::Arena,
        AllocatorSourceKind::Unknown => AllocatorClass::Conflicting,
        AllocatorSourceKind::Parameter => {
            let (Some(function), Some(parameter_index)) = (
                tables
                    .allocator_sources
                    .function
                    .get(index)
                    .map(|value| value.0),
                tables.allocator_sources.parameter_index.get(index).copied(),
            ) else {
                return AllocatorClass::Conflicting;
            };
            match global_parameter_index(tables, function, parameter_index) {
                Some(slot) => parameter_class[slot],
                None => AllocatorClass::Conflicting,
            }
        }
    }
}

pub fn resolve_allocator_provenance(tables: &Tables) -> Provenance {
    let parameters = parameter_count(&tables.parameters);
    let mut parameter_class = vec![AllocatorClass::Unset; parameters];
    // The call argument that put each parameter in the class it currently
    // holds, so a later argument that disagrees can name what it disagreed
    // with rather than only that it did.
    let mut witness = vec![NO_INDEX; parameters];
    let mut disagreements = Vec::new();
    let limit = parameters * 3 + 4;
    let mut converged = false;
    for _ in 0..limit {
        let mut changed = false;
        for row in 0..tables.call_arguments.call.len() {
            let call = tables.call_arguments.call[row].0 as usize;
            let (Some(callee), Some(parameter_index), Some(source)) = (
                tables.calls.callee.get(call).map(|value| value.0),
                tables.call_arguments.parameter_index.get(row).copied(),
                tables.call_arguments.source.get(row).copied(),
            ) else {
                continue;
            };
            let Some(slot) = global_parameter_index(tables, callee, parameter_index) else {
                continue;
            };
            let contributed = class_of_source(tables, &parameter_class, source);
            let held = parameter_class[slot];
            let merged = join(held, contributed);
            if merged == held {
                continue;
            }
            if merged == AllocatorClass::Conflicting
                && held != AllocatorClass::Unset
                && contributed != AllocatorClass::Conflicting
            {
                disagreements.push(Disagreement {
                    parameter: slot as u32,
                    first: witness[slot],
                    second: row as u32,
                    first_class: held,
                    second_class: contributed,
                });
            }
            if merged != AllocatorClass::Conflicting {
                witness[slot] = row as u32;
            }
            parameter_class[slot] = merged;
            changed = true;
        }
        if !changed {
            converged = true;
            break;
        }
    }
    Provenance {
        parameter_class,
        disagreements,
        converged,
    }
}

pub fn classify_source(
    tables: &Tables,
    provenance: &Provenance,
    source: AllocatorSourceId,
) -> AllocatorClass {
    class_of_source(tables, &provenance.parameter_class, source)
}
