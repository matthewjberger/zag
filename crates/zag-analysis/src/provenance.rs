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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Provenance {
    pub parameter_class: Vec<AllocatorClass>,
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
            let merged = join(parameter_class[slot], contributed);
            if merged != parameter_class[slot] {
                parameter_class[slot] = merged;
                changed = true;
            }
        }
        if !changed {
            converged = true;
            break;
        }
    }
    Provenance {
        parameter_class,
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
