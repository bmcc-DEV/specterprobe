use crate::compat::common;
use crate::compat::types::{IspModel, PipelineStage};
use crate::lift::types::{ArmOperand, InstKind, Instruction};

pub fn detect_isp(instructions: &[Instruction]) -> Option<IspModel> {
    let known = common::detect_known_values(instructions);
    let _ping_pong = common::detect_ping_pong_buffers(instructions);

    if known.absolute_stores.len() < 3 {
        return None;
    }

    let resolution = detect_resolution(&known, instructions);
    let stages = detect_isp_stages(instructions);
    let black_level = detect_black_level(instructions);

    if stages.is_empty() && resolution.is_none() {
        return None;
    }

    let input_format = if resolution.map_or(false, |(w, h)| (w as u64 * h as u64 * 3) > 0) {
        "RAW10".into()
    } else {
        "unknown".into()
    };

    let confidence = calc_confidence(&stages, resolution.is_some());

    Some(IspModel {
        resolution,
        input_format,
        output_format: "JPEG".into(),
        black_level,
        pipeline: stages,
        confidence,
    })
}

fn detect_resolution(known: &common::KnownValues, instructions: &[Instruction]) -> Option<(u32, u32)> {
    let mut resolved = Vec::new();

    for &(w, h, _addr) in &known.resolutions {
        resolved.push((w, h));
    }

    for insn in instructions {
        if let InstKind::Mov(_sz, _dst, op) = &insn.kind {
            if let ArmOperand::Imm(val) = op {
                let v = *val as u32;
                for &(rw, rh) in &common::COMMON_RESOLUTIONS {
                    if v == rw || v == rh {
                        if !resolved.contains(&(rw, rh)) {
                            resolved.push((rw, rh));
                        }
                    }
                }
            }
        }
    }

    resolved.first().copied()
}

fn detect_isp_stages(instructions: &[Instruction]) -> Vec<PipelineStage> {
    let mut stages = Vec::new();

    for insn in instructions {
        if let InstKind::Store(_sz, addr, _) = &insn.kind {
            let off = addr.offset as u64;
            if off == 0x2000 || off == 0x2004 || off == 0x2008 || off == 0x200C {
                let name = match off {
                    0x2000 => "capture",
                    0x2004 => "process",
                    0x2008 => "output",
                    0x200C => "jpeg_encode",
                    _ => "unknown",
                };
                stages.push(PipelineStage {
                    name: name.into(),
                    trigger_register: Some(off),
                    input_buffer_reg: None,
                    output_buffer_reg: None,
                    completion_irq: None,
                    latency_us: None,
                    confidence: 0.5,
                });
                if stages.len() >= 4 {
                    break;
                }
            }
        }
    }

    stages
}

fn detect_black_level(instructions: &[Instruction]) -> Option<u16> {
    for insn in instructions {
        if let InstKind::Mov(_sz, _dst, op) = &insn.kind {
            if let ArmOperand::Imm(val) = op {
                if *val == 256 || *val == 512 || *val == 1024 || *val == 64 || *val == 128 {
                    return Some(*val as u16);
                }
            }
        }
    }
    None
}

fn calc_confidence(stages: &[PipelineStage], has_resolution: bool) -> f32 {
    let mut c: f32 = 0.0;
    if !stages.is_empty() {
        c += 0.4;
    }
    if has_resolution {
        c += 0.3;
    }
    c.min(1.0)
}
