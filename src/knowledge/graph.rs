
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeType {
    Soc(String),
    Partition(String),
    Firmware(String),
    MmioRegion { base: u64, size: u64, classification: Option<String> },
    Register { offset: u32, purpose: String, polling: bool },
    Irq(u32),
    Function { name: String, instructions: usize },
    Driver { name: String, target_os: String },
    DtbNode { path: String, compatible: Vec<String> },
    Call(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EdgeType {
    Contains,
    Controls,
    Calls,
    Implements,
    MapsTo { offset: u32 },
    Triggers { irq: u32 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeEdge {
    pub source: usize,
    pub target: usize,
    pub edge_type: EdgeType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeGraph {
    pub nodes: Vec<NodeType>,
    pub edges: Vec<KnowledgeEdge>,
    pub queries: serde_json::Value,
}

impl KnowledgeGraph {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            queries: serde_json::json!({}),
        }
    }

    pub fn add_node(&mut self, node: NodeType) -> usize {
        let idx = self.nodes.len();
        self.nodes.push(node);
        idx
    }

    pub fn add_edge(&mut self, source: usize, target: usize, edge_type: EdgeType) {
        self.edges.push(KnowledgeEdge {
            source,
            target,
            edge_type,
        });
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    pub fn find_nodes<F>(&self, predicate: F) -> Vec<(usize, &NodeType)>
    where
        F: Fn(&NodeType) -> bool,
    {
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| predicate(n))
            .collect()
    }
}
