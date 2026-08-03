use zag_facts::tables::{Tables, call_count, function_count};
use zag_facts::{CallId, FunctionId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallGraph {
    pub edge_start: Vec<u32>,
    pub edge_target: Vec<FunctionId>,
    pub edge_call: Vec<CallId>,
}

pub fn build_call_graph(tables: &Tables) -> CallGraph {
    let functions = function_count(&tables.functions);
    let edges: Vec<(usize, FunctionId, CallId)> = (0..call_count(&tables.calls))
        .filter_map(|row| {
            let caller = tables.calls.caller[row].0 as usize;
            let callee = *tables.calls.callee.get(row)?;
            (caller < functions).then_some((caller, callee, CallId(row as u32)))
        })
        .collect();
    let mut counts = vec![0u32; functions];
    for (caller, _, _) in &edges {
        counts[*caller] += 1;
    }
    let mut edge_start = vec![0u32; functions + 1];
    for index in 0..functions {
        edge_start[index + 1] = edge_start[index] + counts[index];
    }
    let mut cursor = edge_start.clone();
    let mut edge_target = vec![FunctionId(zag_facts::NO_INDEX); edges.len()];
    let mut edge_call = vec![CallId(zag_facts::NO_INDEX); edges.len()];
    for (caller, callee, call) in edges {
        let slot = cursor[caller] as usize;
        edge_target[slot] = callee;
        edge_call[slot] = call;
        cursor[caller] += 1;
    }
    CallGraph {
        edge_start,
        edge_target,
        edge_call,
    }
}

pub fn callees(graph: &CallGraph, caller: FunctionId) -> std::ops::Range<usize> {
    let index = caller.0 as usize;
    if index + 1 >= graph.edge_start.len() {
        return 0..0;
    }
    graph.edge_start[index] as usize..graph.edge_start[index + 1] as usize
}

/// Stamps everything reachable from `root` with `generation`, leaving whatever
/// earlier generations wrote in place. Callers that walk many roots reuse one
/// buffer this way, where a fresh visited vector per root would cost roots
/// times functions no matter how small each closure turns out to be.
pub fn mark_reachable(graph: &CallGraph, root: FunctionId, stamp: &mut [u32], generation: u32) {
    let functions = graph.edge_start.len().saturating_sub(1);
    let index = root.0 as usize;
    if index >= functions || index >= stamp.len() {
        return;
    }
    stamp[index] = generation;
    let mut pending = vec![root];
    while let Some(current) = pending.pop() {
        for edge in callees(graph, current) {
            let target = graph.edge_target[edge];
            let slot = target.0 as usize;
            if slot < functions && slot < stamp.len() && stamp[slot] != generation {
                stamp[slot] = generation;
                pending.push(target);
            }
        }
    }
}

pub fn reachable_from(graph: &CallGraph, root: FunctionId) -> Vec<bool> {
    let functions = graph.edge_start.len().saturating_sub(1);
    let mut stamp = vec![0u32; functions];
    mark_reachable(graph, root, &mut stamp, 1);
    stamp.into_iter().map(|value| value == 1).collect()
}
