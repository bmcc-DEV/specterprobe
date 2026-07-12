use crate::acquisition::AcquireOutput;
use crate::behavior::types::BehaviorOutput;
use crate::knowledge::graph::{EdgeType, KnowledgeGraph, NodeType};
use crate::lift::types::LiftOutput;
use crate::mmio::types::MmioMap;

type NodeId = usize;

pub fn build(graph: &mut KnowledgeGraph, acquire: &AcquireOutput, lift: &LiftOutput, mmio: &MmioMap, behavior: &BehaviorOutput) {
    build_from_acquire(graph, acquire);
    build_from_lift(graph, lift);
    build_from_mmio(graph, mmio);
    build_from_behavior(graph, behavior);
    build_queries(graph);
}

fn add_soc_node(graph: &mut KnowledgeGraph) -> NodeId {
    let name = "qcom,sm6225";
    let existing = graph.find_nodes(|n| matches!(n, NodeType::Soc(n) if n == "qcom,sm6225"));
    existing.first().map(|(id, _)| *id).unwrap_or_else(|| graph.add_node(NodeType::Soc(name.to_string())))
}

fn build_from_acquire(graph: &mut KnowledgeGraph, acquire: &AcquireOutput) {
    let soc = add_soc_node(graph);

    for p in &acquire.partitions {
        let part_node = graph.add_node(NodeType::Partition(p.name.clone()));
        graph.add_edge(soc, part_node, EdgeType::Contains);
    }

    for fw in &acquire.firmwares {
        let fw_node = graph.add_node(NodeType::Firmware(
            fw.path.to_string_lossy().to_string(),
        ));
        graph.add_edge(soc, fw_node, EdgeType::Contains);
    }

    if let Some(ref dtb) = acquire.kernel.dtb {
        for mmio_reg in &dtb.mmio_regions {
            let compat = mmio_reg.compatible.clone();
            if let Some(ref periph) = mmio_reg.peripheral {
                let dtb_node = graph.add_node(NodeType::DtbNode {
                    path: periph.clone(),
                    compatible: compat,
                });
                graph.add_edge(soc, dtb_node, EdgeType::Contains);
            }
        }
    }
}

fn build_from_lift(graph: &mut KnowledgeGraph, lift: &LiftOutput) {
    let soc = add_soc_node(graph);

    for func in &lift.functions {
        let func_node = graph.add_node(NodeType::Function {
            name: func.name.clone(),
            instructions: func.blocks.iter().map(|b| b.instructions.len()).sum(),
        });
        graph.add_edge(soc, func_node, EdgeType::Contains);
    }

    let analysis = crate::lift::analysis::analyze(&lift.functions);
    for call in &analysis.call_graph {
        let from_idx = graph.find_nodes(|n| matches!(n, NodeType::Function { name, .. } if *name == call.from));
        let to_idx = graph.find_nodes(|n| matches!(n, NodeType::Function { name, .. } if *name == call.to));

        if let (Some((from, _)), Some((to, _))) = (from_idx.first(), to_idx.first()) {
            graph.add_edge(*from, *to, EdgeType::Calls);
        }
    }
}

fn build_from_mmio(graph: &mut KnowledgeGraph, mmio: &MmioMap) {
    let soc = add_soc_node(graph);

    for region in &mmio.regions {
        let mmio_node = graph.add_node(NodeType::MmioRegion {
            base: region.base,
            size: region.size,
            classification: region.classification.clone(),
        });
        graph.add_edge(soc, mmio_node, EdgeType::Contains);

        for access in &region.accesses {
            let func_idx = graph.find_nodes(|n| {
                matches!(n, NodeType::Function { name, .. } if *name == access.function_name)
            });
            if let Some((func_id, _)) = func_idx.first() {
                graph.add_edge(mmio_node, *func_id, EdgeType::Controls);
            }
        }
    }

    for _access in &mmio.raw_accesses {
    }
}

fn build_from_behavior(graph: &mut KnowledgeGraph, behavior: &BehaviorOutput) {
    for device in &behavior.devices {
        let dev_nodes: Vec<NodeId> = graph
            .find_nodes(|n| matches!(n, NodeType::MmioRegion { base, .. } if *base == device.base))
            .iter()
            .map(|(id, _)| *id)
            .collect();

        let driver_node = graph.add_node(NodeType::Driver {
            name: device.name.clone().unwrap_or_else(|| format!("device_{:x}", device.base)),
            target_os: "redox".into(),
        });

        for &mmio_id in &dev_nodes {
            graph.add_edge(mmio_id, driver_node, EdgeType::Implements);
        }

        for reg in &device.registers {
            let reg_node = graph.add_node(NodeType::Register {
                offset: reg.offset,
                purpose: reg.purpose.clone().unwrap_or_else(|| "unknown".into()),
                polling: reg.polling,
            });

            if let Some(&mmio_id) = dev_nodes.first() {
                graph.add_edge(mmio_id, reg_node, EdgeType::MapsTo { offset: reg.offset });
            }
        }
    }
}

fn build_queries(graph: &mut KnowledgeGraph) {
    let mmio_regions: Vec<serde_json::Value> = graph
        .nodes
        .iter()
        .filter_map(|n| match n {
            NodeType::MmioRegion { base, size, classification } => {
                Some(serde_json::json!({
                    "base": base,
                    "size": size,
                    "classification": classification
                }))
            }
            _ => None,
        })
        .collect();

    let drivers: Vec<serde_json::Value> = graph
        .nodes
        .iter()
        .filter_map(|n| match n {
            NodeType::Driver { name, target_os } => {
                Some(serde_json::json!({
                    "name": name,
                    "os": target_os
                }))
            }
            _ => None,
        })
        .collect();

    let functions: Vec<serde_json::Value> = graph
        .nodes
        .iter()
        .filter_map(|n| match n {
            NodeType::Function { name, instructions } => {
                Some(serde_json::json!({
                    "name": name,
                    "instructions": instructions
                }))
            }
            _ => None,
        })
        .collect();

    graph.queries = serde_json::json!({
        "all_mmio_regions": mmio_regions,
        "all_drivers": drivers,
        "all_functions": functions,
        "node_count": graph.node_count(),
        "edge_count": graph.edge_count(),
    });
}
