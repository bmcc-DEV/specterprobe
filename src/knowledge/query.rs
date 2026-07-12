use crate::knowledge::graph::{KnowledgeGraph, NodeType};

pub struct QueryResult {
    pub description: String,
    pub nodes: Vec<(usize, String)>,
}

pub fn find_mmio_by_address(graph: &KnowledgeGraph, address: u64) -> Option<QueryResult> {
    let matches: Vec<(usize, String)> = graph
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(id, n)| match n {
            NodeType::MmioRegion { base, .. } if *base == address => {
                Some((id, format!("MmioRegion(0x{:x})", base)))
            }
            _ => None,
        })
        .collect();

    if matches.is_empty() {
        None
    } else {
        Some(QueryResult {
            description: format!("MMIO regions at 0x{:x}", address),
            nodes: matches,
        })
    }
}

pub fn find_drivers_for_address(graph: &KnowledgeGraph, address: u64) -> Option<QueryResult> {
    let mmio_ids: Vec<usize> = graph
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(id, n)| match n {
            NodeType::MmioRegion { base, .. } if *base == address => Some(id),
            _ => None,
        })
        .collect();

    let driver_ids: Vec<(usize, String)> = graph
        .edges
        .iter()
        .filter(|e| mmio_ids.contains(&e.source))
        .filter_map(|e| match &graph.nodes[e.target] {
            NodeType::Driver { name, target_os } => {
                Some((e.target, format!("{}({})", name, target_os)))
            }
            _ => None,
        })
        .collect();

    if driver_ids.is_empty() {
        None
    } else {
        Some(QueryResult {
            description: format!("Drivers for MMIO 0x{:x}", address),
            nodes: driver_ids,
        })
    }
}

pub fn find_all_functions(graph: &KnowledgeGraph) -> QueryResult {
    let nodes: Vec<(usize, String)> = graph
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(id, n)| match n {
            NodeType::Function { name, instructions } => {
                Some((id, format!("{}({} instr)", name, instructions)))
            }
            _ => None,
        })
        .collect();

    QueryResult {
        description: format!("All functions ({} total)", nodes.len()),
        nodes,
    }
}

pub fn find_all_drivers(graph: &KnowledgeGraph) -> QueryResult {
    let nodes: Vec<(usize, String)> = graph
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(id, n)| match n {
            NodeType::Driver { name, target_os } => {
                Some((id, format!("{} [{}]", name, target_os)))
            }
            _ => None,
        })
        .collect();

    QueryResult {
        description: format!("All drivers ({} total)", nodes.len()),
        nodes,
    }
}

pub fn find_all_registers(graph: &KnowledgeGraph) -> QueryResult {
    let nodes: Vec<(usize, String)> = graph
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(id, n)| match n {
            NodeType::Register { offset, purpose, polling } => {
                let poll = if *polling { " [POLL]" } else { "" };
                Some((id, format!("+0x{:x} {}{}", offset, purpose, poll)))
            }
            _ => None,
        })
        .collect();

    QueryResult {
        description: format!("All registers ({} total)", nodes.len()),
        nodes,
    }
}
