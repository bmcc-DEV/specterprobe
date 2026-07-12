use crate::compat::common;
use crate::compat::types::DspModel;
use crate::lift::types::{ArmOperand, InstKind, Instruction};

pub fn detect_dsp(instructions: &[Instruction]) -> Option<DspModel> {
    let known = common::detect_known_values(instructions);
    let ping_pong = common::detect_ping_pong_buffers(instructions);

    if known.sample_rates.is_empty() && known.gain_values.is_empty() && ping_pong.is_empty() {
        return None;
    }

    let sample_rate = known.sample_rates.first().map(|(rate, _)| *rate);
    let channels = detect_channels(&known, instructions);
    let bit_depth = detect_bit_depth(instructions);
    let buffer_size = detect_buffer_size(instructions, sample_rate, channels, bit_depth);
    let volume_reg = detect_volume_register(instructions);
    let has_modem = detect_modem(instructions);

    let confidence = calc_confidence(&known, &ping_pong);

    Some(DspModel {
        sample_rate,
        channels: Some(channels),
        bit_depth: Some(bit_depth),
        buffer_size: Some(buffer_size),
        ping_pong: !ping_pong.is_empty(),
        volume_register: volume_reg,
        has_modem,
        confidence,
    })
}

fn detect_channels(known: &common::KnownValues, instructions: &[Instruction]) -> u8 {
    for insn in instructions {
        if let InstKind::Mov(_sz, _dst, op) = &insn.kind {
            if let ArmOperand::Imm(val) = op {
                if *val == 1 || *val == 2 || *val == 6 || *val == 8 {
                    return *val as u8;
                }
            }
        }
    }

    if known.gain_values.len() > 1 {
        2
    } else {
        2
    }
}

fn detect_bit_depth(instructions: &[Instruction]) -> u8 {
    for insn in instructions {
        if let InstKind::Store(_, addr, _) = &insn.kind {
            let off = addr.offset;
            if off == 16 || off == 24 || off == 32 {
                return off as u8;
            }
        }
    }
    16
}

fn detect_buffer_size(
    instructions: &[Instruction],
    sample_rate: Option<u32>,
    channels: u8,
    bit_depth: u8,
) -> u32 {
    let mut sizes = Vec::new();

    for insn in instructions {
        if let InstKind::Mov(_sz, _dst, op) = &insn.kind {
            if let ArmOperand::Imm(val) = op {
                let v = *val;
                if v > 100 && v < 1000000 && (v as u32) % 32 == 0 {
                    sizes.push(v as u32);
                }
            }
        }
    }

    sizes.sort();
    sizes.reverse();

    if let Some(&sz) = sizes.first() {
        return sz;
    }

    if let Some(rate) = sample_rate {
        rate * channels as u32 * (bit_depth as u32 / 8)
    } else {
        176400
    }
}

fn detect_volume_register(instructions: &[Instruction]) -> Option<u64> {
    for insn in instructions {
        if let InstKind::Store(_, addr, _) = &insn.kind {
            let off = addr.offset;
            if off == 0x3000 || off == 0x3004 || off == 0x3008 || off == 0x300C {
                return Some(off as u64);
            }
        }
    }
    None
}

fn detect_modem(instructions: &[Instruction]) -> bool {
    let mut found = false;
    for insn in instructions {
        if let InstKind::Store(_, addr, _) = &insn.kind {
            let off = addr.offset;
            if off == 0x4000 || off == 0x4004 || off == 0x4008 || off == 0x400C {
                found = true;
            }
        }
    }
    found
}

fn calc_confidence(known: &common::KnownValues, ping_pong: &[(u64, u64)]) -> f32 {
    let mut c: f32 = 0.0;
    if !known.sample_rates.is_empty() {
        c += 0.4;
    }
    if !known.gain_values.is_empty() {
        c += 0.2;
    }
    if !ping_pong.is_empty() {
        c += 0.3;
    }
    c.min(1.0)
}
