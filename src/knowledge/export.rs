use crate::knowledge::graph::{EdgeType, KnowledgeGraph, NodeType};

pub fn export_json(graph: &KnowledgeGraph) -> String {
    serde_json::to_string_pretty(graph).unwrap_or_default()
}

pub fn export_cypher(graph: &KnowledgeGraph) -> String {
    let mut cypher = String::new();
    cypher.push_str("// Knowledge Graph — EAEA Specter Probe\n");
    cypher.push_str("// Generated from firmware analysis\n\n");

    for (id, node) in graph.nodes.iter().enumerate() {
        match node {
            NodeType::Soc(name) => {
                cypher.push_str(&format!(
                    "CREATE (n{}:Soc {{name: '{}'}});\n",
                    id, name
                ));
            }
            NodeType::Partition(name) => {
                cypher.push_str(&format!(
                    "CREATE (n{}:Partition {{name: '{}'}});\n",
                    id, name
                ));
            }
            NodeType::Firmware(path) => {
                cypher.push_str(&format!(
                    "CREATE (n{}:Firmware {{path: '{}'}});\n",
                    id, path
                ));
            }
            NodeType::MmioRegion { base, size, classification } => {
                cypher.push_str(&format!(
                    "CREATE (n{}:MmioRegion {{base: {}, size: {}, classification: '{}'}});\n",
                    id,
                    base,
                    size,
                    classification.as_deref().unwrap_or("unknown")
                ));
            }
            NodeType::Register { offset, purpose, polling } => {
                cypher.push_str(&format!(
                    "CREATE (n{}:Register {{offset: {}, purpose: '{}', polling: {}}});\n",
                    id, offset, purpose, polling
                ));
            }
            NodeType::Irq(n) => {
                cypher.push_str(&format!("CREATE (n{}:Irq {{number: {}}});\n", id, n));
            }
            NodeType::Function { name, instructions } => {
                cypher.push_str(&format!(
                    "CREATE (n{}:Function {{name: '{}', instructions: {}}});\n",
                    id, name, instructions
                ));
            }
            NodeType::Driver { name, target_os } => {
                cypher.push_str(&format!(
                    "CREATE (n{}:Driver {{name: '{}', os: '{}'}});\n",
                    id, name, target_os
                ));
            }
            NodeType::DtbNode { path, compatible } => {
                cypher.push_str(&format!(
                    "CREATE (n{}:DtbNode {{path: '{}', compatible: [{}]}});\n",
                    id,
                    path,
                    compatible
                        .iter()
                        .map(|c| format!("'{}'", c))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            NodeType::Call(name) => {
                cypher.push_str(&format!("CREATE (n{}:Call {{name: '{}'}});\n", id, name));
            }
        }
    }

    cypher.push('\n');
    for edge in &graph.edges {
        let rel_type = match edge.edge_type {
            EdgeType::Contains => "CONTAINS",
            EdgeType::Controls => "CONTROLS",
            EdgeType::Calls => "CALLS",
            EdgeType::Implements => "IMPLEMENTS",
            EdgeType::MapsTo { .. } => "MAPS_TO",
            EdgeType::Triggers { .. } => "TRIGGERS",
        };

        cypher.push_str(&format!(
            "MATCH (a),(b) WHERE id(a) = {} AND id(b) = {} CREATE (a)-[:{}]->(b);\n",
            edge.source, edge.target, rel_type
        ));
    }

    cypher
}
