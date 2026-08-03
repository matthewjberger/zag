use zag_analysis::call_graph::{build_call_graph, callees, reachable_from};
use zag_facts::build::{intern, push_call, push_function};
use zag_facts::fixture::example_tables;
use zag_facts::tables::{Tables, empty_tables};
use zag_facts::validate::validate;
use zag_facts::{FunctionId, NO_INDEX, StructId};

fn chain(length: u32) -> Tables {
    let mut tables = empty_tables();
    for index in 0..length {
        let name = intern(&mut tables.strings, format!("f{index}").as_bytes());
        push_function(&mut tables, name, StructId(NO_INDEX));
    }
    for index in 0..length.saturating_sub(1) {
        push_call(&mut tables, FunctionId(index), FunctionId(index + 1));
    }
    tables
}

#[test]
fn an_empty_program_has_an_empty_graph() {
    let graph = build_call_graph(&empty_tables());
    assert_eq!(graph.edge_target.len(), 0);
    assert_eq!(callees(&graph, FunctionId(0)), 0..0);
}

#[test]
fn every_call_becomes_exactly_one_edge() {
    let tables = example_tables();
    let graph = build_call_graph(&tables);
    assert_eq!(graph.edge_target.len(), tables.calls.caller.len());
}

#[test]
fn edges_are_grouped_under_their_caller() {
    let tables = chain(4);
    assert_eq!(validate(&tables), Ok(()));
    let graph = build_call_graph(&tables);
    for caller in 0..3u32 {
        let range = callees(&graph, FunctionId(caller));
        assert_eq!(range.len(), 1);
        assert_eq!(graph.edge_target[range.start], FunctionId(caller + 1));
    }
    assert_eq!(callees(&graph, FunctionId(3)), 3..3);
}

#[test]
fn a_caller_with_several_callees_gets_a_contiguous_range() {
    let mut tables = chain(0);
    for index in 0..4u32 {
        let name = intern(&mut tables.strings, format!("f{index}").as_bytes());
        push_function(&mut tables, name, StructId(NO_INDEX));
    }
    for callee in 1..4u32 {
        push_call(&mut tables, FunctionId(0), FunctionId(callee));
    }
    assert_eq!(validate(&tables), Ok(()));
    let graph = build_call_graph(&tables);
    let range = callees(&graph, FunctionId(0));
    assert_eq!(range.len(), 3);
    let targets: Vec<u32> = range.map(|edge| graph.edge_target[edge].0).collect();
    assert_eq!(targets, vec![1, 2, 3]);
    assert_eq!(callees(&graph, FunctionId(1)), 3..3);
}

#[test]
fn reachability_follows_the_chain_transitively() {
    let graph = build_call_graph(&chain(4));
    assert_eq!(
        reachable_from(&graph, FunctionId(0)),
        vec![true, true, true, true]
    );
    assert_eq!(
        reachable_from(&graph, FunctionId(2)),
        vec![false, false, true, true]
    );
}

#[test]
fn reachability_terminates_on_a_cycle() {
    let mut tables = chain(3);
    push_call(&mut tables, FunctionId(2), FunctionId(0));
    let graph = build_call_graph(&tables);
    assert_eq!(
        reachable_from(&graph, FunctionId(0)),
        vec![true, true, true]
    );
}

#[test]
fn reachability_terminates_on_direct_recursion() {
    let mut tables = chain(1);
    push_call(&mut tables, FunctionId(0), FunctionId(0));
    let graph = build_call_graph(&tables);
    assert_eq!(reachable_from(&graph, FunctionId(0)), vec![true]);
}

#[test]
fn an_out_of_range_root_reaches_nothing() {
    let graph = build_call_graph(&chain(2));
    assert_eq!(reachable_from(&graph, FunctionId(99)), vec![false, false]);
}

#[test]
fn the_fixture_deinit_reaches_the_helper_that_frees() {
    let tables = example_tables();
    let graph = build_call_graph(&tables);
    let deinit = tables.structs.deinit[0];
    let reachable = reachable_from(&graph, deinit);
    let release = tables
        .functions
        .name
        .iter()
        .position(|name| zag_facts::tables::string_bytes(&tables.strings, *name) == b"release")
        .expect("the fixture defines a release function");
    assert!(reachable[release]);
}
