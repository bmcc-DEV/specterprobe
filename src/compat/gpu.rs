use crate::compat::common;
use crate::compat::types::{BufferModel, BufferType, GpuModel, PipelineStage, QueueModel};
use crate::lift::types::{ArmOperand, InstKind, Instruction};

pub fn detect_gpu(instructions: &[Instruction], _mmio_regions: &[crate::mmio::types::MmioRegion]) -> Option<GpuModel> {
    let known = common::detect_known_values(instructions);
    let doorbells = common::detect_doorbell_pattern(instructions);
    let ping_pong = common::detect_ping_pong_buffers(instructions);

    if doorbells.is_empty() && known.absolute_stores.len() < 5 {
        return None;
    }

    let queues = detect_queues(&doorbells, instructions);
    let buffers = detect_buffers(&known, &ping_pong);
    let pipeline = detect_pipeline(instructions);
    let compute_units = detect_compute_units(&known, instructions);
    let has_3d = buffers.iter().any(|b| matches!(b.detected_type, BufferType::Framebuffer));
    let has_compute = compute_units > 0 || queues.len() > 2;

    let confidence = calc_confidence(&queues, &buffers);

    Some(GpuModel {
        queues,
        buffers,
        pipeline,
        compute_units,
        has_compute,
        has_3d,
        confidence,
    })
}

fn detect_queues(doorbells: &[u64], instructions: &[Instruction]) -> Vec<QueueModel> {
    let mut queues = Vec::new();

    for (_i, &doorbell) in doorbells.iter().enumerate() {
        let head = instructions.iter().find_map(|insn| {
            if let InstKind::Store(_, addr, _) = &insn.kind {
                if addr.offset as u64 == doorbell + 4 {
                    return Some(doorbell + 4);
                }
            }
            None
        });

        let tail = instructions.iter().find_map(|insn| {
            if let InstKind::Store(_, addr, _) = &insn.kind {
                if addr.offset as u64 == doorbell + 8 {
                    return Some(doorbell + 8);
                }
            }
            None
        });

        queues.push(QueueModel {
            doorbell_register: doorbell,
            head_register: head.or(Some(doorbell + 4)),
            tail_register: tail.or(Some(doorbell + 8)),
            descriptor_size: 64,
            queue_depth: 128,
            confidence: 0.6,
        });
    }

    if queues.is_empty() && !doorbells.is_empty() {
        for &db in doorbells {
            queues.push(QueueModel {
                doorbell_register: db,
                head_register: None,
                tail_register: None,
                descriptor_size: 64,
                queue_depth: 128,
                confidence: 0.4,
            });
        }
    }

    queues
}

fn detect_buffers(known: &common::KnownValues, ping_pong: &[(u64, u64)]) -> Vec<BufferModel> {
    let mut buffers = Vec::new();

    for (addr_lo, addr_hi) in ping_pong {
        if *addr_hi - *addr_lo < 0x100000 {
            buffers.push(BufferModel {
                address_register: *addr_lo,
                size_register: Some(0),
                stride: None,
                width: None,
                height: None,
                frame_size: Some(*addr_hi - *addr_lo),
                detected_type: BufferType::Framebuffer,
                confidence: 0.5,
            });
        }
    }

    for &(addr, _) in &known.absolute_stores {
        if !buffers.iter().any(|b| b.address_register == addr) {
            buffers.push(BufferModel {
                address_register: addr,
                size_register: None,
                stride: None,
                width: None,
                height: None,
                frame_size: None,
                detected_type: BufferType::CommandBuffer,
                confidence: 0.3,
            });
        }
        if buffers.len() >= 8 {
            break;
        }
    }

    buffers
}

fn detect_pipeline(instructions: &[Instruction]) -> Vec<PipelineStage> {
    let mut stages = Vec::new();
    let mut stage_idx = 0;

    for insn in instructions {
        if let InstKind::Store(_sz, addr, _) = &insn.kind {
            let off = addr.offset as u64;
            if off == 0xC || off == 0x10 {
                stages.push(PipelineStage {
                    name: format!("stage_{}", stage_idx),
                    trigger_register: Some(off),
                    input_buffer_reg: None,
                    output_buffer_reg: None,
                    completion_irq: None,
                    latency_us: None,
                    confidence: 0.4,
                });
                stage_idx += 1;
                if stage_idx > 4 {
                    break;
                }
            }
        }
    }

    stages
}

fn detect_compute_units(_known: &common::KnownValues, instructions: &[Instruction]) -> u32 {
    for insn in instructions {
        if let InstKind::Mov(_, _, op) = &insn.kind {
            if let ArmOperand::Imm(val) = op {
                if *val == 1 || *val == 2 || *val == 4 || *val == 8 {
                    return *val as u32;
                }
            }
        }
    }
    1
}

fn calc_confidence(queues: &[QueueModel], buffers: &[BufferModel]) -> f32 {
    let mut c: f32 = 0.0;
    if !queues.is_empty() {
        c += 0.3;
    }
    if buffers.len() > 2 {
        c += 0.3;
    }
    if buffers.iter().any(|b| matches!(b.detected_type, BufferType::Framebuffer)) {
        c += 0.2;
    }
    c.min(1.0)
}
