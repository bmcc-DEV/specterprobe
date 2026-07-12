use crate::lift::types::{ArmOperand, InstKind, Instruction, Reg};
use std::collections::HashMap;

pub const COMMON_SAMPLE_RATES: [u32; 8] = [8000, 11025, 16000, 22050, 44100, 48000, 96000, 192000];
pub const COMMON_RESOLUTIONS: [(u32, u32); 6] = [
    (640, 480),
    (1280, 720),
    (1920, 1080),
    (3264, 2448),
    (4096, 3072),
    (3840, 2160),
];

pub fn detect_known_values(instructions: &[Instruction]) -> KnownValues {
    let mut kv = KnownValues::default();
    let mut seen_reg_vals: HashMap<(u64, u64), usize> = HashMap::new();

    for insn in instructions {
        if let InstKind::Mov(_, reg, op) = &insn.kind {
            if let ArmOperand::Imm(val) = op {
                let imm = *val as u64;
                if let Some(_reg_name) = reg_to_short(reg) {
                    seen_reg_vals
                        .entry((imm, 0))
                        .and_modify(|c| *c += 1)
                        .or_insert(1);

                    if COMMON_SAMPLE_RATES.contains(&(imm as u32)) {
                        kv.sample_rates.push((imm as u32, insn.address));
                    }
                    for &(w, h) in &COMMON_RESOLUTIONS {
                        if imm as u32 == w || imm as u32 == h {
                            kv.resolutions.push((w, h, insn.address));
                        }
                    }
                    if imm == 0xAC44 || imm == 0xBB80 || imm == 0x3E80 || imm == 0x1F40 {
                        kv.sample_rates.push((imm as u32, insn.address));
                    }
                    if imm <= 255 && imm > 0 {
                        kv.gain_values.push((imm, insn.address));
                    }
                }
            }
        }

        if let InstKind::Store(_sz, addr, _) = &insn.kind {
            if addr.offset > 0 {
                kv.store_offsets.push(addr.offset as u64);
            }
            if matches!(addr.base, Reg::Xzr | Reg::Wzr) {
                kv.absolute_stores.push((addr.offset as u64, insn.address));
            }
        }
    }

    kv
}

#[derive(Debug, Default)]
pub struct KnownValues {
    pub sample_rates: Vec<(u32, u64)>,
    pub resolutions: Vec<(u32, u32, u64)>,
    pub gain_values: Vec<(u64, u64)>,
    pub store_offsets: Vec<u64>,
    pub absolute_stores: Vec<(u64, u64)>,
}

pub fn detect_doorbell_pattern(instructions: &[Instruction]) -> Vec<u64> {
    let mut doorbells = Vec::new();
    let mut seq_writes: HashMap<u64, u64> = HashMap::new();

    for insn in instructions {
        if let InstKind::Store(_, addr, _) = &insn.kind {
            let reg_off = addr.offset as u64;
            let counter = seq_writes.entry(reg_off).or_insert(0);
            *counter += 1;
        }
    }

    for (reg, count) in &seq_writes {
        if *count > 3 {
            doorbells.push(*reg);
        }
    }

    doorbells
}

pub fn detect_ping_pong_buffers(instructions: &[Instruction]) -> Vec<(u64, u64)> {
    let mut addrs: Vec<u64> = Vec::new();

    for insn in instructions {
        if let InstKind::Store(_, addr, _) = &insn.kind {
            if matches!(addr.base, Reg::Xzr | Reg::Wzr) {
                addrs.push(addr.offset as u64);
            }
        }
    }

    let mut ping_pong = Vec::new();
    for i in 1..addrs.len() {
        if addrs[i] != addrs[i - 1] && addrs[i] < addrs[i - 1] + 0x1000 && addrs[i - 1] < addrs[i] + 0x1000 {
            if !ping_pong.iter().any(|(a, b)| *a == addrs[i] || *b == addrs[i]) {
                ping_pong.push((addrs[i - 1], addrs[i]));
            }
        }
    }

    ping_pong
}

pub fn detect_irq_timing(instructions: &[Instruction]) -> Vec<(u64, u64)> {
    let mut irq_waits = Vec::new();

    for insn in instructions {
        if let InstKind::Load(_, _, addr) = &insn.kind {
            if addr.offset == 0x10 || addr.offset == 0x14 {
                irq_waits.push((addr.offset as u64, insn.address));
            }
        }
    }

    irq_waits
}

fn reg_to_short(r: &Reg) -> Option<String> {
    match r {
        Reg::X(n) => Some(format!("x{}", n)),
        Reg::W(n) => Some(format!("w{}", n)),
        Reg::R(n) => Some(format!("r{}", n)),
        _ => None,
    }
}
