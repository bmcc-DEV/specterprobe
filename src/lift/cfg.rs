use crate::lift::types::{BasicBlock, Function, InstKind, Instruction, Reg};

pub fn reconstruct_cfg(instructions: &[Instruction]) -> Vec<Function> {
    let prologue_starts = find_functions(instructions);
    let mut functions = Vec::new();

    for (i, &entry) in prologue_starts.iter().enumerate() {
        let end = if i + 1 < prologue_starts.len() {
            prologue_starts[i + 1]
        } else {
            instructions.len()
        };

        let func_insns = &instructions[entry..end];
        if func_insns.is_empty() {
            continue;
        }

        let blocks = build_blocks(func_insns);
        let exit_blocks = find_exit_blocks(&blocks);
        let name = guess_function_name(entry, &blocks);

        functions.push(Function {
            entry: instructions[entry].address,
            name,
            blocks,
            exit_blocks,
        });
    }

    if functions.is_empty() && !instructions.is_empty() {
        let blocks = build_blocks(instructions);
        let exit_blocks = find_exit_blocks(&blocks);
        functions.push(Function {
            entry: instructions[0].address,
            name: "entry".into(),
            blocks,
            exit_blocks,
        });
    }

    functions
}

fn find_functions(instructions: &[Instruction]) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut i = 0;

    while i < instructions.len() {
        let insn = &instructions[i];

        if is_function_prologue(insn) {
            if starts.last().map_or(true, |&last| i - last > 4) {
                starts.push(i);
            }
        }

        if is_function_epilogue(insn) {
            if i + 1 < instructions.len() {
                let next = &instructions[i + 1];
                if !is_function_prologue(next) && i - starts.last().copied().unwrap_or(0) > 4 {
                    starts.push(i + 1);
                }
            }
        }

        i += 1;
    }

    starts
}

fn is_function_prologue(insn: &Instruction) -> bool {
    match &insn.kind {
        InstKind::StorePair(_, _, Reg::Fp, Reg::Lr) => true,
        InstKind::StorePair(_, _, Reg::Lr, Reg::Fp) => true,
        InstKind::Sub(_, Reg::Sp, Reg::Sp, _) => true,
        _ => false,
    }
}

fn is_function_epilogue(insn: &Instruction) -> bool {
    matches!(&insn.kind, InstKind::Ret(_))
}

fn build_blocks(instructions: &[Instruction]) -> Vec<BasicBlock> {
    let mut leaders = vec![0usize];

    for (i, insn) in instructions.iter().enumerate() {
        match &insn.kind {
            InstKind::Branch(_, target)
            | InstKind::BranchAlways(target)
            | InstKind::BranchLink(target)
            | InstKind::CompareBranch(_, _, target) => {
                    let target_idx = find_target_index(instructions, *target);
                if let Some(ti) = target_idx {
                    if ti < instructions.len() && !leaders.contains(&ti) {
                        leaders.push(ti);
                    }
                }
                if i + 1 < instructions.len() {
                    let next = i + 1;
                    if !leaders.contains(&next) {
                        leaders.push(next);
                    }
                }
            }
            InstKind::Ret(_) => {
                if i + 1 < instructions.len() {
                    let next = i + 1;
                    if !leaders.contains(&next) {
                        leaders.push(next);
                    }
                }
            }
            _ => {}
        }
    }

    leaders.sort();
    leaders.dedup();

    let mut blocks = Vec::new();
    for (i, &start) in leaders.iter().enumerate() {
        let end = if i + 1 < leaders.len() {
            leaders[i + 1]
        } else {
            instructions.len()
        };

        let block_insns: Vec<Instruction> = instructions[start..end].to_vec();
        let (successors, cond_succ) = get_successors(&block_insns);

        let addr_idx = start.min(instructions.len().saturating_sub(1));
        blocks.push(BasicBlock {
            address: instructions[addr_idx].address,
            instructions: block_insns,
            successors,
            cond_successor: cond_succ,
        });
    }

    blocks
}

fn find_target_index(instructions: &[Instruction], target: u64) -> Option<usize> {
    if instructions.is_empty() {
        return None;
    }
    let first_addr = instructions[0].address;
    if target < first_addr {
        return None;
    }
    let idx = (target - first_addr) as usize;
    if idx < instructions.len() && instructions[idx].address == target {
        Some(idx)
    } else {
        instructions.iter().position(|i| i.address == target)
    }
}

fn get_successors(block: &[Instruction]) -> (Vec<u64>, Option<u64>) {
    if let Some(last) = block.last() {
        match &last.kind {
            InstKind::BranchAlways(target) => {
                return (vec![*target], None);
            }
            InstKind::Branch(_, target) => {
                return (vec![*target], None);
            }
            InstKind::CompareBranch(_, _, target) => {
                return (vec![*target], None);
            }
            InstKind::BranchLink(target) => {
                return (vec![*target], None);
            }
            InstKind::BranchReg(_) => {
                return (vec![], None);
            }
            InstKind::Ret(_) => {
                return (vec![], None);
            }
            _ => {}
        }
    }
    (vec![], None)
}

fn find_exit_blocks(blocks: &[BasicBlock]) -> Vec<u64> {
    blocks
        .iter()
        .filter(|b| {
            b.instructions.last().map_or(false, |last| {
                matches!(last.kind, InstKind::Ret(_))
            })
        })
        .map(|b| b.address)
        .collect()
}

fn guess_function_name(entry_idx: usize, blocks: &[BasicBlock]) -> String {
    if let Some(first) = blocks.first() {
        if let Some(insn) = first.instructions.first() {
            let addr = insn.address;
            if addr == 0 {
                return "entry".into();
            }
            return format!("sub_{:x}", addr);
        }
    }
    format!("func_{}", entry_idx)
}
