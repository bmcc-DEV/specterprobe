use crate::lift::types::{
    Arch, ArmOperand, CondCode, InstKind, Instruction, MemAddr, Reg, Size,
};
use capstone::arch::{BuildsCapstone, BuildsCapstoneEndian, DetailsArchInsn};

pub fn detect_arch(data: &[u8]) -> Arch {
    if data.len() < 20 {
        return Arch::Arm64;
    }
    if data[0..4] == [0x7F, 0x45, 0x4C, 0x46] {
        if data[4] == 2 {
            return match data[5] {
                0xB7 => Arch::Arm64,
                0x28 => Arch::Arm32,
                _ => Arch::Arm64,
            };
        }
    }
    Arch::Arm64
}

pub fn disassemble(data: &[u8], arch: Arch) -> Vec<Instruction> {
    match arch {
        Arch::Arm64 => disasm_arm64(data),
        Arch::Arm32 | Arch::Thumb => disasm_arm32(data, arch),
    }
}

fn disasm_arm64(data: &[u8]) -> Vec<Instruction> {
    let cs: capstone::Capstone = match capstone::Capstone::new()
        .arm64()
        .mode(capstone::arch::arm64::ArchMode::Arm)
        .endian(capstone::Endian::Little)
        .detail(true)
        .build()
    {
        Ok(cs) => cs,
        Err(e) => {
            tracing::warn!("Capstone ARM64 init failed: {e}");
            return vec![];
        }
    };

    let insns: capstone::Instructions = match cs.disasm_all(data, 0x0) {
        Ok(i) => i,
        Err(e) => {
            tracing::warn!("Disassembly failed: {e}");
            return vec![];
        }
    };

    let mut instructions = Vec::new();

    for insn in insns.as_ref() {
        let mnemonic = insn.mnemonic().unwrap_or("?").to_string();
        let op_str = insn.op_str().unwrap_or("").to_string();
        let kind = match cs.insn_detail(insn) {
            Ok(d) => {
                let arch_detail = d.arch_detail();
                if let Some(detail) = arch_detail.arm64() {
                    decode_arm64_insn(&mnemonic, detail)
                } else {
                    InstKind::Unknown("no_arm64_detail".into())
                }
            }
            Err(_) => InstKind::Unknown(mnemonic.clone()),
        };

        instructions.push(Instruction {
            address: insn.address(),
            bytes: insn.bytes().to_vec(),
            mnemonic,
            op_str,
            kind,
        });
    }

    instructions
}

fn cap_reg_to_reg64(r: capstone::RegId) -> Reg {
    let n = r.0;
    if n >= 1 && n <= 28 {
        Reg::X(n as u8)
    } else if n == 29 {
        Reg::Fp
    } else if n == 30 {
        Reg::Lr
    } else if n == 31 {
        Reg::Xzr
    } else if n >= 65 && n <= 92 {
        Reg::W((n - 65) as u8)
    } else if n == 93 {
        Reg::Wzr
    } else {
        Reg::X(n.min(31) as u8)
    }
}

fn arm64_op_size(op: &capstone::arch::arm64::Arm64Operand) -> Size {
    match &op.op_type {
        capstone::arch::arm64::Arm64OperandType::Reg(id) => {
            if id.0 >= 65 && id.0 <= 93 {
                Size::B32
            } else {
                Size::B64
            }
        }
        _ => Size::B64,
    }
}

fn arm64_mem(m: &capstone::arch::arm64::Arm64OpMem) -> MemAddr {
    MemAddr {
        base: cap_reg_to_reg64(m.base()),
        offset: m.disp() as i64,
        post_index: false,
        pre_index: false,
    }
}

fn arm64_op(op: &capstone::arch::arm64::Arm64Operand) -> ArmOperand {
    use capstone::arch::arm64::Arm64OperandType;
    match &op.op_type {
        Arm64OperandType::Reg(r) => ArmOperand::Reg(cap_reg_to_reg64(*r)),
        Arm64OperandType::Imm(val) => ArmOperand::Imm(*val),
        Arm64OperandType::Mem(m) => ArmOperand::Mem(arm64_mem(m)),
        _ => ArmOperand::Imm(0),
    }
}

fn decode_arm64_insn(
    mnemonic: &str,
    detail: &capstone::arch::arm64::Arm64InsnDetail,
) -> InstKind {
    use capstone::arch::arm64::Arm64OperandType;

    let ops: Vec<_> = detail.operands().collect();

    match mnemonic {
        "str" if ops.len() >= 2 => {
            let val = match &ops[0].op_type {
                Arm64OperandType::Reg(r) => cap_reg_to_reg64(*r),
                _ => return InstKind::Unknown("STR".into()),
            };
            let addr = match &ops[1].op_type {
                Arm64OperandType::Mem(m) => arm64_mem(m),
                _ => return InstKind::Unknown("STR".into()),
            };
            let sz = arm64_op_size(&ops[0]);
            InstKind::Store(sz, addr, val)
        }
        "ldr" if ops.len() >= 2 => {
            let dst = match &ops[0].op_type {
                Arm64OperandType::Reg(r) => cap_reg_to_reg64(*r),
                _ => return InstKind::Unknown("LDR".into()),
            };
            match &ops[1].op_type {
                Arm64OperandType::Mem(m) => {
                    let addr = arm64_mem(m);
                    let sz = arm64_op_size(&ops[0]);
                    InstKind::Load(sz, dst, addr)
                }
                Arm64OperandType::Imm(val) => {
                    InstKind::LoadLiteral(Size::B64, dst, *val as u64)
                }
                _ => InstKind::Unknown("LDR".into()),
            }
        }
        "stp" if ops.len() >= 3 => {
            let r1 = match &ops[0].op_type {
                Arm64OperandType::Reg(r) => cap_reg_to_reg64(*r),
                _ => return InstKind::Unknown("STP".into()),
            };
            let r2 = match &ops[1].op_type {
                Arm64OperandType::Reg(r) => cap_reg_to_reg64(*r),
                _ => return InstKind::Unknown("STP".into()),
            };
            let addr = match &ops[2].op_type {
                Arm64OperandType::Mem(m) => arm64_mem(m),
                _ => return InstKind::Unknown("STP".into()),
            };
            InstKind::StorePair(Size::B64, addr, r1, r2)
        }
        "ldp" if ops.len() >= 3 => {
            let r1 = match &ops[0].op_type {
                Arm64OperandType::Reg(r) => cap_reg_to_reg64(*r),
                _ => return InstKind::Unknown("LDP".into()),
            };
            let r2 = match &ops[1].op_type {
                Arm64OperandType::Reg(r) => cap_reg_to_reg64(*r),
                _ => return InstKind::Unknown("LDP".into()),
            };
            let addr = match &ops[2].op_type {
                Arm64OperandType::Mem(m) => arm64_mem(m),
                _ => return InstKind::Unknown("LDP".into()),
            };
            InstKind::LoadPair(Size::B64, r1, r2, addr)
        }
        "add" | "adds" if ops.len() >= 3 => {
            let dst = match &ops[0].op_type {
                Arm64OperandType::Reg(r) => cap_reg_to_reg64(*r),
                _ => return InstKind::Unknown("ADD".into()),
            };
            let src = match &ops[1].op_type {
                Arm64OperandType::Reg(r) => cap_reg_to_reg64(*r),
                _ => return InstKind::Unknown("ADD".into()),
            };
            let op2 = arm64_op(&ops[2]);
            let sz = arm64_op_size(&ops[0]);
            InstKind::Add(sz, dst, src, op2)
        }
        "sub" | "subs" if ops.len() >= 3 => {
            let dst = match &ops[0].op_type {
                Arm64OperandType::Reg(r) => cap_reg_to_reg64(*r),
                _ => return InstKind::Unknown("SUB".into()),
            };
            let src = match &ops[1].op_type {
                Arm64OperandType::Reg(r) => cap_reg_to_reg64(*r),
                _ => return InstKind::Unknown("SUB".into()),
            };
            let op2 = arm64_op(&ops[2]);
            let sz = arm64_op_size(&ops[0]);
            InstKind::Sub(sz, dst, src, op2)
        }
        "mov" if ops.len() >= 2 => {
            let dst = match &ops[0].op_type {
                Arm64OperandType::Reg(r) => cap_reg_to_reg64(*r),
                _ => return InstKind::Unknown("MOV".into()),
            };
            let src = arm64_op(&ops[1]);
            let sz = arm64_op_size(&ops[0]);
            InstKind::Mov(sz, dst, src)
        }
        "cmp" if ops.len() >= 2 => {
            let r = match &ops[0].op_type {
                Arm64OperandType::Reg(r) => cap_reg_to_reg64(*r),
                _ => return InstKind::Unknown("CMP".into()),
            };
            let op2 = arm64_op(&ops[1]);
            InstKind::Cmp(Size::B64, r, op2)
        }
        "b" if ops.len() >= 1 => {
            if let Arm64OperandType::Imm(val) = &ops[0].op_type {
                InstKind::BranchAlways(*val as u64)
            } else {
                InstKind::Unknown("B".into())
            }
        }
        "bl" if ops.len() >= 1 => {
            if let Arm64OperandType::Imm(val) = &ops[0].op_type {
                InstKind::BranchLink(*val as u64)
            } else {
                InstKind::Unknown("BL".into())
            }
        }
        "b.eq" | "b.ne" | "b.cs" | "b.cc" | "b.mi" | "b.pl" | "b.vs" | "b.vc"
        | "b.hi" | "b.ls" | "b.ge" | "b.lt" | "b.gt" | "b.le" | "b.al" => {
            let cc = match mnemonic.as_ref() {
                "b.eq" => CondCode::Eq,
                "b.ne" => CondCode::Ne,
                "b.cs" => CondCode::Cs,
                "b.cc" => CondCode::Cc,
                "b.mi" => CondCode::Mi,
                "b.pl" => CondCode::Pl,
                "b.vs" => CondCode::Vs,
                "b.vc" => CondCode::Vc,
                "b.hi" => CondCode::Hi,
                "b.ls" => CondCode::Ls,
                "b.ge" => CondCode::Ge,
                "b.lt" => CondCode::Lt,
                "b.gt" => CondCode::Gt,
                "b.le" => CondCode::Le,
                _ => CondCode::Al,
            };
            if let Arm64OperandType::Imm(val) = &ops[0].op_type {
                InstKind::Branch(cc, *val as u64)
            } else {
                InstKind::Unknown("B.cond".into())
            }
        }
        "cbz" | "cbnz" if ops.len() >= 2 => {
            let r = match &ops[0].op_type {
                Arm64OperandType::Reg(r) => cap_reg_to_reg64(*r),
                _ => return InstKind::Unknown("CBZ".into()),
            };
            let is_nz = mnemonic == "cbnz";
            if let Arm64OperandType::Imm(target) = &ops[1].op_type {
                InstKind::CompareBranch(is_nz, r, *target as u64)
            } else {
                InstKind::Unknown("CBZ".into())
            }
        }
        "adrp" if ops.len() >= 2 => {
            let dst = match &ops[0].op_type {
                Arm64OperandType::Reg(r) => cap_reg_to_reg64(*r),
                _ => return InstKind::Unknown("ADRP".into()),
            };
            if let Arm64OperandType::Imm(page) = &ops[1].op_type {
                InstKind::Adrp(dst, *page as u64)
            } else {
                InstKind::Unknown("ADRP".into())
            }
        }
        "ret" | "retab" => InstKind::Ret(None),
        "nop" => InstKind::Nop,
        "svc" => {
            if let Some(op) = ops.first() {
                if let Arm64OperandType::Imm(n) = &op.op_type {
                    return InstKind::Svc(*n as u32);
                }
            }
            InstKind::Svc(0)
        }
        "and" | "ands" if ops.len() >= 3 => decode_arm64_binop("and", &ops),
        "orr" if ops.len() >= 3 => decode_arm64_binop("orr", &ops),
        "eor" if ops.len() >= 3 => decode_arm64_binop("eor", &ops),
        "mul" if ops.len() >= 3 => decode_arm64_binop("mul", &ops),
        "sdiv" if ops.len() >= 3 => decode_arm64_binop("sdiv", &ops),
        "udiv" if ops.len() >= 3 => decode_arm64_binop("udiv", &ops),
        "lsl" if ops.len() >= 3 => decode_arm64_binop("lsl", &ops),
        "lsr" if ops.len() >= 3 => decode_arm64_binop("lsr", &ops),
        "asr" if ops.len() >= 3 => decode_arm64_binop("asr", &ops),
        _ => InstKind::Unknown(mnemonic.to_string()),
    }
}

fn decode_arm64_binop(_op: &str, ops: &[capstone::arch::arm64::Arm64Operand]) -> InstKind {
    use capstone::arch::arm64::Arm64OperandType;
    let dst = match &ops[0].op_type {
        Arm64OperandType::Reg(r) => cap_reg_to_reg64(*r),
        _ => return InstKind::Unknown("BINOP".into()),
    };
    let src = match &ops[1].op_type {
        Arm64OperandType::Reg(r) => cap_reg_to_reg64(*r),
        _ => return InstKind::Unknown("BINOP".into()),
    };
    InstKind::Unknown(format!("{}_{}_{}", _op, reg_to_short(&dst), reg_to_short(&src)))
}

fn reg_to_short(r: &Reg) -> String {
    match r {
        Reg::X(n) => format!("x{n}"), Reg::W(n) => format!("w{n}"),
        Reg::R(n) => format!("r{n}"),
        Reg::Sp => "sp".into(), Reg::Fp => "fp".into(), Reg::Lr => "lr".into(),
        Reg::Xzr => "xzr".into(), Reg::Wzr => "wzr".into(),
        Reg::Pc => "pc".into(),
    }
}

// ─── ARM32 ─────────────────────────────────────────────

fn disasm_arm32(data: &[u8], arch: Arch) -> Vec<Instruction> {
    use capstone::arch::arm::ArchMode;

    let mode = match arch {
        Arch::Thumb => ArchMode::Thumb,
        _ => ArchMode::Arm,
    };

    let cs: capstone::Capstone = match capstone::Capstone::new()
        .arm()
        .mode(mode)
        .endian(capstone::Endian::Little)
        .detail(true)
        .build()
    {
        Ok(cs) => cs,
        Err(e) => {
            tracing::warn!("Capstone ARM32 init failed: {e}");
            return vec![];
        }
    };

    let insns: capstone::Instructions = match cs.disasm_all(data, 0x0) {
        Ok(i) => i,
        Err(e) => {
            tracing::warn!("ARM32 disassembly failed: {e}");
            return vec![];
        }
    };

    let mut instructions = Vec::new();
    for insn in insns.as_ref() {
        let mnemonic = insn.mnemonic().unwrap_or("?").to_string();
        let op_str = insn.op_str().unwrap_or("").to_string();
        let kind = match cs.insn_detail(insn) {
            Ok(d) => {
                let arch_detail = d.arch_detail();
                if let Some(detail) = arch_detail.arm() {
                    decode_arm32_insn(&mnemonic, detail)
                } else {
                    InstKind::Unknown(mnemonic.clone())
                }
            }
            Err(_) => InstKind::Unknown(mnemonic.clone()),
        };

        instructions.push(Instruction {
            address: insn.address(),
            bytes: insn.bytes().to_vec(),
            mnemonic,
            op_str,
            kind,
        });
    }
    instructions
}

fn cap_reg_to_arm_reg(r: capstone::RegId) -> Reg {
    let n = r.0;
    if n >= 1 && n <= 12 {
        Reg::R(n as u8)
    } else if n == 13 {
        Reg::Sp
    } else if n == 14 {
        Reg::Lr
    } else if n == 15 {
        Reg::Pc
    } else {
        Reg::R(n.min(15) as u8)
    }
}

fn decode_arm32_insn(
    mnemonic: &str,
    detail: &capstone::arch::arm::ArmInsnDetail,
) -> InstKind {
    use capstone::arch::arm::ArmOperandType;

    let ops: Vec<_> = detail.operands().collect();

    match mnemonic {
        "str" if ops.len() >= 2 => {
            let val = match &ops[0].op_type {
                ArmOperandType::Reg(r) => cap_reg_to_arm_reg(*r),
                _ => return InstKind::Unknown("ARM32_STR".into()),
            };
            let addr = match &ops[1].op_type {
                ArmOperandType::Mem(m) => MemAddr {
                    base: cap_reg_to_arm_reg(m.base()),
                    offset: m.disp() as i64,
                    post_index: false,
                    pre_index: false,
                },
                _ => return InstKind::Unknown("ARM32_STR".into()),
            };
            InstKind::Store(Size::B32, addr, val)
        }
        "ldr" if ops.len() >= 2 => {
            let dst = match &ops[0].op_type {
                ArmOperandType::Reg(r) => cap_reg_to_arm_reg(*r),
                _ => return InstKind::Unknown("ARM32_LDR".into()),
            };
            match &ops[1].op_type {
                ArmOperandType::Mem(m) => {
                    let addr = MemAddr {
                        base: cap_reg_to_arm_reg(m.base()),
                        offset: m.disp() as i64,
                        post_index: false,
                        pre_index: false,
                    };
                    InstKind::Load(Size::B32, dst, addr)
                }
                _ => InstKind::Unknown("ARM32_LDR".into()),
            }
        }
        "mov" if ops.len() >= 2 => {
            let dst = match &ops[0].op_type {
                ArmOperandType::Reg(r) => cap_reg_to_arm_reg(*r),
                _ => return InstKind::Unknown("ARM32_MOV".into()),
            };
            let src = match &ops[1].op_type {
                ArmOperandType::Reg(r) => ArmOperand::Reg(cap_reg_to_arm_reg(*r)),
                ArmOperandType::Imm(val) => ArmOperand::Imm(*val as i64),
                _ => ArmOperand::Imm(0),
            };
            InstKind::Mov(Size::B32, dst, src)
        }
        "add" if ops.len() >= 3 => {
            let dst = match &ops[0].op_type {
                ArmOperandType::Reg(r) => cap_reg_to_arm_reg(*r),
                _ => return InstKind::Unknown("ARM32_ADD".into()),
            };
            let src = match &ops[1].op_type {
                ArmOperandType::Reg(r) => cap_reg_to_arm_reg(*r),
                _ => return InstKind::Unknown("ARM32_ADD".into()),
            };
            let op2 = match &ops[2].op_type {
                ArmOperandType::Reg(r) => ArmOperand::Reg(cap_reg_to_arm_reg(*r)),
                ArmOperandType::Imm(val) => ArmOperand::Imm(*val as i64),
                _ => ArmOperand::Imm(0),
            };
            InstKind::Add(Size::B32, dst, src, op2)
        }
        "b" | "bl" => {
            if let ArmOperandType::Imm(val) = &ops[0].op_type {
                if mnemonic == "bl" {
                    InstKind::BranchLink(*val as u64)
                } else {
                    InstKind::BranchAlways(*val as u64)
                }
            } else {
                InstKind::Unknown("ARM32_BR".into())
            }
        }
        "nop" => InstKind::Nop,
        _ => InstKind::Unknown(format!("ARM32_{}", mnemonic)),
    }
}
