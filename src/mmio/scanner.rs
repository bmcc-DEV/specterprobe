use crate::lift::types::{Function, InstKind, Instruction, Reg};
use crate::mmio::types::{AccessType, MmioAccess};
use std::collections::HashMap;

pub fn scan_functions(functions: &[Function]) -> Vec<MmioAccess> {
    let mut accesses = Vec::new();

    for func in functions {
        let func_name = func.name.clone();

        for block in &func.blocks {
            let mut reg_values: HashMap<String, u64> = HashMap::new();

            for insn in &block.instructions {
                track_reg_defs(insn, &mut reg_values);

                match &insn.kind {
                    InstKind::Store(sz, addr, _) => {
                        if let Some(absolute_addr) = resolve_addr(&addr.base, addr.offset, &reg_values) {
                            let confidence = if matches!(addr.base, Reg::Xzr | Reg::Wzr) {
                                0.9
                            } else if reg_values.contains_key(&reg_key(&addr.base)) {
                                0.7
                            } else {
                                0.4
                            };
                            accesses.push(MmioAccess {
                                address: absolute_addr,
                                size: size_bytes(*sz),
                                access_type: AccessType::Write,
                                instruction_addr: insn.address,
                                function_name: func_name.clone(),
                                confidence,
                            });
                        }
                    }
                    InstKind::Load(sz, _, addr) => {
                        if let Some(absolute_addr) = resolve_addr(&addr.base, addr.offset, &reg_values) {
                            let confidence = if matches!(addr.base, Reg::Xzr | Reg::Wzr) {
                                0.9
                            } else if reg_values.contains_key(&reg_key(&addr.base)) {
                                0.7
                            } else {
                                0.4
                            };
                            accesses.push(MmioAccess {
                                address: absolute_addr,
                                size: size_bytes(*sz),
                                access_type: AccessType::Read,
                                instruction_addr: insn.address,
                                function_name: func_name.clone(),
                                confidence,
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    accesses
}

fn reg_key(r: &Reg) -> String {
    match r {
        Reg::X(n) => format!("x{}", n),
        Reg::W(n) => format!("w{}", n),
        Reg::R(n) => format!("r{}", n),
        _ => format!("{:?}", r),
    }
}

fn resolve_addr(base: &Reg, offset: i64, reg_values: &HashMap<String, u64>) -> Option<u64> {
    match base {
        Reg::Xzr | Reg::Wzr => Some(offset as u64),
        Reg::Sp | Reg::Fp => None,
        _ => {
            let key = reg_key(base);
            if let Some(base_val) = reg_values.get(&key) {
                Some(base_val.wrapping_add(offset as u64))
            } else {
                None
            }
        }
    }
}

fn track_reg_defs(insn: &Instruction, reg_values: &mut HashMap<String, u64>) {
    match &insn.kind {
        InstKind::Adrp(dst, page) => {
            reg_values.insert(reg_key(dst), *page);
        }
        InstKind::Add(_sz, dst, src, op) => {
            let src_val = get_reg_value(src, reg_values);
            let imm_val = get_imm_op(op);
            if let (Some(sv), Some(iv)) = (src_val, imm_val) {
                reg_values.insert(reg_key(dst), sv.wrapping_add(iv));
            }
        }
        InstKind::Mov(_sz, dst, op) => {
            if let Some(val) = get_imm_op(op) {
                reg_values.insert(reg_key(dst), val);
            }
        }
        InstKind::Sub(_sz, dst, src, op) => {
            let src_val = get_reg_value(src, reg_values);
            let imm_val = get_imm_op(op);
            if let (Some(sv), Some(iv)) = (src_val, imm_val) {
                reg_values.insert(reg_key(dst), sv.wrapping_sub(iv));
            }
        }
        _ => {}
    }
}

fn get_reg_value(r: &Reg, reg_values: &HashMap<String, u64>) -> Option<u64> {
    match r {
        Reg::Xzr | Reg::Wzr => Some(0),
        Reg::Sp => Some(0xffff_ffff_ffff_e000), // approximate SP
        _ => reg_values.get(&reg_key(r)).copied(),
    }
}

fn get_imm_op(op: &crate::lift::types::ArmOperand) -> Option<u64> {
    match op {
        crate::lift::types::ArmOperand::Imm(val) => Some(*val as u64),
        crate::lift::types::ArmOperand::Reg(r) => match r {
            Reg::Xzr | Reg::Wzr => Some(0),
            _ => None,
        },
        _ => None,
    }
}

fn size_bytes(sz: crate::lift::types::Size) -> u8 {
    match sz {
        crate::lift::types::Size::B8 => 1,
        crate::lift::types::Size::B16 => 2,
        crate::lift::types::Size::B32 => 4,
        crate::lift::types::Size::B64 => 8,
        crate::lift::types::Size::B128 => 16,
    }
}
