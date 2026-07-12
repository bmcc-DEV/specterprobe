pub mod export;
pub mod schema;

use crate::behavior::types::BehaviorOutput;
use crate::compat::types::CompatOutput;
use crate::genome::export::to_yaml_pretty;
use crate::genome::schema::*;
use crate::knowledge::graph::KnowledgeGraph;
use crate::lift::types::LiftOutput;
use crate::mmio::types::MmioMap;
use std::path::Path;

pub fn generate_all(
    behavior: &BehaviorOutput,
    compat: &CompatOutput,
    kg: &KnowledgeGraph,
    _lift: &LiftOutput,
    mmio: &MmioMap,
    output_dir: &Path,
) -> anyhow::Result<Vec<DeviceGenome>> {
    let genomes_dir = output_dir.join("genomes");
    std::fs::create_dir_all(&genomes_dir)?;

    let mut all_genomes = Vec::new();

    for device in &behavior.devices {
        let genome = build_device_genome(device, kg, compat, mmio);
        let yaml = to_yaml_pretty(&genome);
        let name = device.name.clone().unwrap_or_else(|| format!("device_{:x}", device.base));
        std::fs::write(genomes_dir.join(format!("{}.yaml", name)), &yaml)?;
        all_genomes.push(genome);
    }

    if let Some(ref gpu) = compat.gpu {
        for (i, q) in gpu.queues.iter().enumerate() {
            let genome = DeviceGenome {
                version: 1,
                device: DeviceInfo {
                    name: format!("gpu_queue_{}", i),
                    base: q.doorbell_register,
                    device_type: "gpu_queue".into(),
                    classification: None,
                    confidence: q.confidence,
                },
                bus: BusInfo {
                    bus_type: "mmio".into(),
                    address: q.doorbell_register,
                    size: 64,
                    irq: None,
                },
                behavior: BehaviorProfile {
                    interrupt: "doorbell".into(),
                    has_polling: false,
                    state_machine: vec!["idle".into(), "submitted".into(), "done".into()],
                    init_sequence: vec![],
                    confidence: q.confidence,
                },
                registers: vec![],
                firmware: None,
                driver: None,
                graph: kg_node_ref(kg, "gpu"),
                };
            let yaml = to_yaml_pretty(&genome);
            std::fs::write(genomes_dir.join(format!("gpu_queue_{}.yaml", i)), &yaml)?;
            all_genomes.push(genome);
        }
    }

    let summary = export::generate_summary(&all_genomes);
    std::fs::write(output_dir.join("genome_summary.yaml"), &summary)?;

    tracing::info!(
        "Device Genome: {} genomes generated in {:?}",
        all_genomes.len(),
        genomes_dir
    );

    Ok(all_genomes)
}

fn build_device_genome(
    device: &crate::behavior::types::DeviceModel,
    kg: &KnowledgeGraph,
    _compat: &CompatOutput,
    _mmio: &MmioMap,
) -> DeviceGenome {
    let device_name = device.name.clone().unwrap_or_else(|| format!("device_{:x}", device.base));

    DeviceGenome {
        version: 1,
        device: DeviceInfo {
            name: device_name.clone(),
            base: device.base,
            device_type: device.classification.clone().unwrap_or_else(|| "peripheral".into()),
            classification: device.classification.clone(),
            confidence: device.confidence,
        },
        bus: BusInfo {
            bus_type: "mmio".into(),
            address: device.base,
            size: 4096,
            irq: None,
        },
        behavior: BehaviorProfile {
            interrupt: if device.registers.iter().any(|r| r.polling) {
                "polling".into()
            } else {
                "unknown".into()
            },
            has_polling: device.registers.iter().any(|r| r.polling),
            state_machine: device
                .state_machine
                .as_ref()
                .map(|sm| sm.states.clone())
                .unwrap_or_default(),
            init_sequence: device.init_sequence.clone(),
            confidence: device.confidence,
        },
        registers: device
            .registers
            .iter()
            .map(|r| RegisterGenome {
                offset: r.offset,
                name: r.name.clone(),
                width: r.width,
                purpose: r.purpose.clone().unwrap_or_else(|| "unknown".into()),
                access: match r.access {
                    crate::behavior::types::AccessType::Read => "read",
                    crate::behavior::types::AccessType::Write => "write",
                }.into(),
                polling: r.polling,
                observed_reads: r.observed_reads.len(),
                observed_writes: r.observed_writes.len(),
                bitfields: r
                    .bitfields
                    .iter()
                    .map(|b| BitfieldDef {
                        offset: b.offset,
                        width: b.width,
                        name: b.name.clone(),
                        values: b.values.clone(),
                    })
                    .collect(),
            })
            .collect(),
        firmware: None,
        driver: None,
        graph: kg_node_ref(kg, &device_name),
    }
}

fn kg_node_ref(kg: &KnowledgeGraph, tag: &str) -> Option<GraphRef> {
    Some(GraphRef {
        node_count: kg.node_count(),
        edge_count: kg.edge_count(),
        tags: vec![tag.to_string()],
    })
}
