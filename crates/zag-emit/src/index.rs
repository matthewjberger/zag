//! Cross-table lookups the emitter and the report both need, built once.
//!
//! Every question here has an obvious linear answer, and every one of them is
//! asked once per function or once per field. Scanning would make the report
//! quadratic in the size of the program, which does not show on an example and
//! is unusable on a real codebase.
//!
//! Each index is a counting sort into compressed sparse row, the same shape
//! `build_call_graph` and `build_field_index` use.

use zag_facts::tables::{Tables, field_count, function_count, string_bytes, struct_count};
use zag_facts::{FieldId, FunctionId, NO_INDEX, StructId};

#[derive(Clone, Debug, Default)]
struct Rows {
    start: Vec<u32>,
    count: Vec<u32>,
    rows: Vec<u32>,
}

fn group(owners: impl Iterator<Item = usize> + Clone, buckets: usize) -> Rows {
    let mut count = vec![0u32; buckets];
    for owner in owners.clone() {
        if let Some(slot) = count.get_mut(owner) {
            *slot += 1;
        }
    }
    let mut start = vec![0u32; buckets];
    let mut running = 0u32;
    for bucket in 0..buckets {
        start[bucket] = running;
        running = running.saturating_add(count[bucket]);
    }
    let mut filled = vec![0u32; buckets];
    let mut rows = vec![0u32; running as usize];
    for (row, owner) in owners.enumerate() {
        let Some(base) = start.get(owner).copied() else {
            continue;
        };
        let at = (base + filled[owner]) as usize;
        if let Some(slot) = rows.get_mut(at) {
            *slot = row as u32;
            filled[owner] += 1;
        }
    }
    Rows { start, count, rows }
}

fn rows_of(grouped: &Rows, owner: usize) -> &[u32] {
    let (Some(&start), Some(&count)) = (grouped.start.get(owner), grouped.count.get(owner)) else {
        return &[];
    };
    let start = start as usize;
    let end = start.saturating_add(count as usize).min(grouped.rows.len());
    grouped.rows.get(start.min(end)..end).unwrap_or(&[])
}

#[derive(Clone, Debug, Default)]
pub struct Index {
    /// The `init` each struct declares, absent where it declares none.
    init: Vec<FunctionId>,
    assignments_by_field: Rows,
    operations_by_function: Rows,
    methods_by_owner: Rows,
    /// Functions that belong to no struct, keyed by the module that declares
    /// them. The emitter walks one module at a time, so without this it would
    /// walk every function once per module.
    free_by_module: Rows,
    structs_by_module: Rows,
    /// Whether a function assigns any field at all, which is what separates a
    /// helper that only frees from one that does work as well.
    assigns: Vec<bool>,
}

pub fn build_index(tables: &Tables) -> Index {
    let structs = struct_count(&tables.structs);
    let functions = function_count(&tables.functions);

    let mut init = vec![FunctionId(NO_INDEX); structs];
    for row in 0..functions {
        let Some(owner) = tables.functions.owner.get(row).copied() else {
            continue;
        };
        if owner.0 == NO_INDEX
            || string_bytes(&tables.strings, tables.functions.name[row]) != b"init"
        {
            continue;
        }
        // The first `init` wins, which matches the order a reader would find
        // them in and keeps two of them from silently swapping places.
        if let Some(slot) = init.get_mut(owner.0 as usize)
            && slot.0 == NO_INDEX
        {
            *slot = FunctionId(row as u32);
        }
    }

    let mut assigns = vec![false; functions];
    for owner in &tables.field_assignments.function {
        if let Some(slot) = assigns.get_mut(owner.0 as usize) {
            *slot = true;
        }
    }

    let modules = zag_facts::tables::module_count(&tables.modules).max(1);
    // A function with no owner is bucketed by its module, and one with an
    // owner by that owner, so neither pass has to look at the other's rows.
    let absent_owner = structs;
    let methods_by_owner = group(
        (0..functions).map(|row| {
            tables
                .functions
                .owner
                .get(row)
                .map(|owner| owner.0 as usize)
                .filter(|owner| *owner < structs)
                .unwrap_or(absent_owner)
        }),
        structs + 1,
    );
    let free_by_module = group(
        (0..functions).map(|row| {
            let owned = tables
                .functions
                .owner
                .get(row)
                .is_some_and(|owner| (owner.0 as usize) < structs);
            if owned {
                return modules;
            }
            tables
                .functions
                .module
                .get(row)
                .map(|module| module.0 as usize)
                .unwrap_or(modules)
        }),
        modules + 1,
    );

    Index {
        init,
        methods_by_owner,
        free_by_module,
        structs_by_module: group(
            (0..structs).map(|row| {
                tables
                    .structs
                    .module
                    .get(row)
                    .map(|module| module.0 as usize)
                    .unwrap_or(0)
            }),
            modules,
        ),
        assignments_by_field: group(
            tables
                .field_assignments
                .field
                .iter()
                .map(|field| field.0 as usize),
            field_count(&tables.fields),
        ),
        operations_by_function: group(
            tables
                .memory_operations
                .function
                .iter()
                .map(|function| function.0 as usize),
            functions,
        ),
        assigns,
    }
}

pub fn init_of(index: &Index, owner: StructId) -> Option<FunctionId> {
    index
        .init
        .get(owner.0 as usize)
        .copied()
        .filter(|function| function.0 != NO_INDEX)
}

pub fn assignments_of(index: &Index, field: FieldId) -> &[u32] {
    rows_of(&index.assignments_by_field, field.0 as usize)
}

pub fn operations_of(index: &Index, function: FunctionId) -> &[u32] {
    rows_of(&index.operations_by_function, function.0 as usize)
}

pub fn assigns_anything(index: &Index, function: FunctionId) -> bool {
    index
        .assigns
        .get(function.0 as usize)
        .copied()
        .unwrap_or(false)
}

pub fn methods_of(index: &Index, owner: StructId) -> &[u32] {
    rows_of(&index.methods_by_owner, owner.0 as usize)
}

pub fn free_functions_of(index: &Index, module: zag_facts::ModuleId) -> &[u32] {
    rows_of(&index.free_by_module, module.0 as usize)
}

pub fn structs_of(index: &Index, module: zag_facts::ModuleId) -> &[u32] {
    rows_of(&index.structs_by_module, module.0 as usize)
}
