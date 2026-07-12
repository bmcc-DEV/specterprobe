use crate::lift::types::{ArmOperand, Function, InstKind, Reg, Size};
use std::collections::HashMap;

pub fn generate_ir(functions: &[Function], module_name: &str) -> String {
    let mut ir = String::new();
    ir.push_str(&format!("; ModuleID = '{}'\n", module_name));
    ir.push_str("target datalayout = \"e-m:e-i8:8:32-i16:16:32-i64:64-i128:128-n32:64-S128\"\n");
    ir.push_str("target triple = \"aarch64-unknown-linux-android\"\n\n");

    ir.push_str("declare void @putchar(i8)\n");
    ir.push_str("declare i64 @read()\n");
    ir.push_str("declare void @unimplemented()\n\n");

    for func in functions {
        ir.push_str(&emit_function(func));
        ir.push('\n');
    }
    ir
}

fn emit_function(func: &Function) -> String {
    let mut ir = String::new();
    let mut next_var = 0u64;
    let mut block_names = HashMap::new();

    for (i, block) in func.blocks.iter().enumerate() {
        block_names.insert(block.address, format!("L{}", i));
    }

    let fn_name = sanitize(&func.name);
    ir.push_str(&format!("define i64 @{}() {{\n", fn_name));

    for block in &func.blocks {
        let label = block_names.get(&block.address).map(|s| s.as_str()).unwrap_or("entry");
        ir.push_str(&format!("{}:\n", label));

        for insn in &block.instructions {
            let (line, nv) = emit_ir(insn, next_var);
            next_var = nv;
            if !line.is_empty() {
                ir.push_str(&format!("  {}\n", line));
            }
        }
    }

    ir.push_str("  ret i64 0\n");
    ir.push_str("}\n");
    ir
}

fn var(c: u64) -> String { format!("%{}", c) }

fn emit_ir(insn: &crate::lift::types::Instruction, v: u64) -> (String, u64) {
    let mut c = v;
    match &insn.kind {
        InstKind::Sub(_, _, _, op2) => {
            let (s, nc) = arm_op_ir(op2, c);
            c = nc;
            (format!("{} = sub i64 %sp, {}", var(c), s), c + 1)
        }
        InstKind::Mov(_, _, src) => {
            let s = arm_op_short(src);
            (format!("; mov {}", s), c + 1)
        }
        InstKind::Cmp(_, r, op2) => {
            (format!("; cmp {} {}", reg_name(r), arm_op_short(op2)), c + 1)
        }
        InstKind::Load(sz, _, addr) => {
            let ptr = var(c); c += 1;
            let _ = format!("{} = inttoptr i64 {} to ptr", ptr, reg_name(&addr.base));
            let ld = format!("{} = load {}, {}", var(c), size_t(sz), ptr);
            (format!("{} ; ldr {}", ld, reg_name(&addr.base)), c + 1)
        }
        InstKind::Store(sz, addr, val) => {
            let ptr = var(c); c += 1;
            let _ = format!("{} = inttoptr i64 {} to ptr", ptr, reg_name(&addr.base));
            (format!("store {} {}, ptr {} ; str {}", size_t(sz), reg_name(val), ptr, reg_name(&addr.base)), c + 1)
        }
        InstKind::StorePair(_sz, _, r1, r2) => {
            (format!("; stp {} {}", reg_name(r1), reg_name(r2)), c + 1)
        }
        InstKind::LoadPair(_sz, r1, r2, addr) => {
            (format!("; ldp {} {} ; from [{}]", reg_name(r1), reg_name(r2), reg_name(&addr.base)), c + 1)
        }
        InstKind::Adrp(_, page) => {
            (format!("{} = add i64 0, {}", var(c), page), c + 1)
        }
        InstKind::BranchAlways(target) => {
            (format!("br label %L{:x}", target), c + 1)
        }
        InstKind::Branch(_, target) => {
            (format!("br label %L{:x}", target), c + 1)
        }
        InstKind::CompareBranch(_, _, target) => {
            (format!("br label %L{:x}", target), c + 1)
        }
        InstKind::BranchLink(target) => {
            (format!("call void @sub_{:x}()", target), c + 1)
        }
        InstKind::Ret(_) => (String::new(), c + 1),
        InstKind::Svc(n) => (format!("; svc #{}", n), c + 1),
        InstKind::Nop => (String::new(), c + 1),
        InstKind::Unknown(s) => (format!("; unimplemented: {}", s), c + 1),
        _ => (format!("; {}", insn.mnemonic), c + 1),
    }
}

fn reg_name(r: &Reg) -> String {
    match r {
        Reg::X(n) => format!("%x{}", n), Reg::W(n) => format!("%w{}", n),
        Reg::R(n) => format!("%r{}", n),
        Reg::Sp => "%sp".into(), Reg::Fp => "%fp".into(), Reg::Lr => "%lr".into(),
        Reg::Xzr => "%xzr".into(), Reg::Wzr => "%wzr".into(), Reg::Pc => "%pc".into(),
    }
}

fn size_t(s: &Size) -> &'static str {
    match s { Size::B8 => "i8", Size::B16 => "i16", Size::B32 => "i32", Size::B64 => "i64", Size::B128 => "i128" }
}

fn arm_op_ir(op: &ArmOperand, v: u64) -> (String, u64) {
    match op {
        ArmOperand::Imm(val) => (format!("{}", val), v),
        ArmOperand::Reg(r) => (reg_name(r), v),
        ArmOperand::Mem(addr) => {
            let p = var(v);
            (format!("{} = inttoptr i64 {} to ptr", p, reg_name(&addr.base)), v + 1)
        }
        _ => ("0".into(), v),
    }
}

fn arm_op_short(op: &ArmOperand) -> String {
    match op {
        ArmOperand::Imm(val) => format!("#{}", val),
        ArmOperand::Reg(r) => reg_name(r).replace('%', "x"),
        ArmOperand::Mem(addr) => format!("[{}, #{}]", reg_name(&addr.base).replace('%', "x"), addr.offset),
        _ => "?".into(),
    }
}

fn sanitize(name: &str) -> String {
    name.replace(|c: char| !c.is_alphanumeric() && c != '_', "_")
}
