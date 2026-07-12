pub mod builders;
pub mod export;
pub mod graph;
pub mod query;

use crate::acquisition::AcquireOutput;
use crate::behavior::types::BehaviorOutput;
use crate::knowledge::graph::KnowledgeGraph;
use crate::lift::types::LiftOutput;
use crate::mmio::types::MmioMap;

pub fn build_knowledge_graph(
    acquire: &AcquireOutput,
    lift: &LiftOutput,
    mmio: &MmioMap,
    behavior: &BehaviorOutput,
    export_neo4j: bool,
) -> anyhow::Result<KnowledgeGraph> {
    let mut graph = KnowledgeGraph::new();

    builders::build(&mut graph, acquire, lift, mmio, behavior);

    tracing::info!(
        "Knowledge Graph: {} nodes, {} edges",
        graph.node_count(),
        graph.edge_count()
    );

    if export_neo4j {
        let cypher = export::export_cypher(&graph);
        tracing::info!("CYPHER export generated: {} bytes", cypher.len());
    }

    Ok(graph)
}
