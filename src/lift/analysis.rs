use crate::lift::types::{Function, InstKind};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub function_count: usize,
    pub total_blocks: usize,
    pub total_instructions: usize,
    pub lifted_count: usize,
    pub unknown_count: usize,
    pub mmio_candidates: Vec<MmioCandidate>,
    pub syscalls: Vec<u64>,
    pub call_graph: Vec<CallEdge>,
    pub mapped_regions: Vec<MappedRegion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MmioCandidate {
    pub address: u64,
    pub function: String,
    pub access_type: String,
    pub instruction_addr: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallEdge {
    pub from: String,
    pub to: String,
    pub instruction_addr: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MappedRegion {
    pub address: u64,
    pub size: u64,
    pub function: String,
}

pub fn analyze(functions: &[Function]) -> AnalysisResult {
    let mut mmio_candidates = Vec::new();
    let mut syscalls = Vec::new();
    let mut call_graph = Vec::new();
    let mut mapped_regions = Vec::new();
    let mut total_blocks = 0;
    let mut lifted_count = 0;
    let mut unknown_count = 0;
    let mut total_instructions = 0;

    for func in functions {
        for block in &func.blocks {
            total_blocks += 1;
            for insn in &block.instructions {
                total_instructions += 1;

                match &insn.kind {
                    InstKind::Unknown(_) => {
                        unknown_count += 1;
                    }
                    InstKind::Store(_, addr, _) | InstKind::Load(_, _, addr) => {
                        let access_type = if matches!(&insn.kind, InstKind::Store(..)) {
                            "write"
                        } else {
                            "read"
                        };
                        if addr.offset > 0x1000 || addr.offset == 0 {
                            let is_mmio = matches!(&addr.base,
                                crate::lift::types::Reg::X(_)
                            );
                            if is_mmio {
                                mmio_candidates.push(MmioCandidate {
                                    address: addr.offset as u64,
                                    function: func.name.clone(),
                                    access_type: access_type.into(),
                                    instruction_addr: insn.address,
                                });
                            }
                        }
                    }
                    InstKind::Adrp(_, page) => {
                        mapped_regions.push(MappedRegion {
                            address: *page,
                            size: 0x1000,
                            function: func.name.clone(),
                        });
                    }
                    InstKind::Svc(n) => {
                        syscalls.push(*n as u64);
                    }
                    InstKind::BranchLink(target) => {
                        call_graph.push(CallEdge {
                            from: func.name.clone(),
                            to: format!("sub_{:x}", target),
                            instruction_addr: insn.address,
                        });
                    }
                    _ => {
                        lifted_count += 1;
                    }
                }
            }
        }
    }

    AnalysisResult {
        function_count: functions.len(),
        total_blocks,
        total_instructions,
        lifted_count,
        unknown_count,
        mmio_candidates,
        syscalls,
        call_graph,
        mapped_regions,
    }
}
