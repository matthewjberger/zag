pub mod call_graph;
pub mod ownership;
pub mod provenance;

use zag_facts::tables::Tables;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Analysis {
    pub graph: call_graph::CallGraph,
    pub provenance: provenance::Provenance,
    pub ownership: ownership::Ownership,
}

pub fn analyze(tables: &Tables) -> Analysis {
    let graph = call_graph::build_call_graph(tables);
    let provenance = provenance::resolve_allocator_provenance(tables);
    let ownership = ownership::classify_ownership(tables, &graph, &provenance);
    Analysis {
        graph,
        provenance,
        ownership,
    }
}
