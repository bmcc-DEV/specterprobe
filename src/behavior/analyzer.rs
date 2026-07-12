use crate::behavior::types::{
    AccessSequence, AccessType, DeviceModel, RegisterModel, SequencedAccess,
};
use crate::mmio::types::{MmioAccess, MmioRegion};
use std::collections::{HashMap, HashSet};

pub fn build_device_models(regions: &[MmioRegion], _raw_accesses: &[MmioAccess]) -> Vec<DeviceModel> {
    let mut models = Vec::new();

    for region in regions {
        let base = region.base;
        let regs = build_register_map(region);

        let sequences = extract_sequences(region);

        let polling_offsets: HashSet<u32> = regs
            .iter()
            .filter(|r| r.polling)
            .map(|r| r.offset)
            .collect();

        let init_sequence = detect_init_sequence(&sequences, &polling_offsets);

        let state_machine = infer_state_machine(&regs, &sequences);

        let confidence = region.confidence;

        models.push(DeviceModel {
            base,
            name: region.classification.clone(),
            classification: region.classification.clone(),
            registers: regs,
            state_machine,
            init_sequence,
            sequences,
            confidence,
        });
    }

    models
}

fn build_register_map(region: &MmioRegion) -> Vec<RegisterModel> {
    let mut reg_map: HashMap<u32, RegisterModel> = HashMap::new();

    for access in &region.accesses {
        let offset = (access.address - region.base) as u32;
        let entry = reg_map.entry(offset).or_insert(RegisterModel {
            offset,
            name: None,
            access: if matches!(access.access_type, crate::mmio::types::AccessType::Read) {
                AccessType::Read
            } else {
                AccessType::Write
            },
            width: access.size,
            observed_writes: Vec::new(),
            observed_reads: Vec::new(),
            bitfields: Vec::new(),
            polling: false,
            count: 0,
            purpose: None,
        });

        entry.count += 1;

        match access.access_type {
            crate::mmio::types::AccessType::Read => entry.observed_reads.push(0),
            crate::mmio::types::AccessType::Write => entry.observed_writes.push(0),
        }
    }

    let mut regs: Vec<RegisterModel> = reg_map.into_values().collect();

    for reg in &mut regs {
        reg.polling = reg.count > 3 && matches!(reg.access, AccessType::Read);
        reg.purpose = guess_purpose(reg);
        reg.bitfields = detect_bitfields(reg);
    }

    regs.sort_by_key(|r| r.offset);
    regs
}

fn guess_purpose(reg: &RegisterModel) -> Option<String> {
    if reg.polling {
        return Some("status".into());
    }
    if reg.count == 1 && matches!(reg.access, AccessType::Write) {
        return Some("control".into());
    }
    if reg.count > 2 && matches!(reg.access, AccessType::Write) {
        return Some("config".into());
    }
    if matches!(reg.access, AccessType::Read) && reg.count <= 2 {
        return Some("version".into());
    }
    None
}

fn detect_bitfields(reg: &RegisterModel) -> Vec<crate::behavior::types::Bitfield> {
    let mut all_values: Vec<u64> = reg.observed_writes.clone();
    all_values.extend(&reg.observed_reads);

    if all_values.len() < 2 {
        return Vec::new();
    }

    let mut mask: u64 = 0;
    for i in 1..all_values.len() {
        mask |= all_values[i - 1] ^ all_values[i];
    }

    if mask == 0 {
        return Vec::new();
    }

    let mut fields = Vec::new();
    let mut bit: usize = 0;
    while bit < 64 {
        if (mask >> bit) & 1 == 1 {
            let mut w: u8 = 1;
            while (bit + (w as usize)) < 64 && ((mask >> (bit + (w as usize))) & 1) == 1 {
                w += 1;
            }
            let mut values = Vec::new();
            for &v in &all_values {
                let field_val = (v >> bit) & ((1u64 << w) - 1);
                if !values.iter().any(|(val, _)| *val == field_val) {
                    values.push((field_val, format!("val_{}", field_val)));
                }
            }
            fields.push(crate::behavior::types::Bitfield {
                offset: bit as u8,
                width: w,
                name: None,
                values,
                observed_mask: (1u64 << w) - 1,
            });
            bit += w as usize;
        } else {
            bit += 1;
        }
    }

    fields
}

fn extract_sequences(region: &MmioRegion) -> Vec<AccessSequence> {
    let mut func_accesses: HashMap<String, Vec<SequencedAccess>> = HashMap::new();

    for access in &region.accesses {
        let offset = (access.address - region.base) as u32;
        let at = if matches!(access.access_type, crate::mmio::types::AccessType::Read) {
            AccessType::Read
        } else {
            AccessType::Write
        };

        func_accesses
            .entry(access.function_name.clone())
            .or_default()
            .push(SequencedAccess {
                offset,
                access_type: at,
                value: None,
                instruction_addr: access.instruction_addr,
            });
    }

    let mut sequences = Vec::new();
    for (func, mut accs) in func_accesses {
        accs.sort_by_key(|a| a.instruction_addr);
        sequences.push(AccessSequence {
            function: func,
            accesses: accs,
        });
    }

    sequences.sort_by(|a, b| a.accesses.first().map(|x| x.instruction_addr).unwrap_or(0)
        .cmp(&b.accesses.first().map(|x| x.instruction_addr).unwrap_or(0)));

    sequences
}

fn detect_init_sequence(
    sequences: &[AccessSequence],
    polling_offsets: &HashSet<u32>,
) -> Vec<String> {
    let mut init = Vec::new();

    for seq in sequences {
        for acc in &seq.accesses {
            if polling_offsets.contains(&acc.offset) {
                continue;
            }
            let line = match acc.access_type {
                AccessType::Write => format!("write(+0x{:x})", acc.offset),
                AccessType::Read => format!("read(+0x{:x})", acc.offset),
            };
            if !init.contains(&line) {
                init.push(line);
            }
        }
    }

    init
}

fn infer_state_machine(
    regs: &[RegisterModel],
    _sequences: &[AccessSequence],
) -> Option<crate::behavior::types::StateMachine> {
    if regs.is_empty() {
        return None;
    }

    let has_polling = regs.iter().any(|r| r.polling);
    let has_writes = regs.iter().any(|r| matches!(r.access, AccessType::Write));

    let mut states = vec!["idle".to_string()];
    let mut transitions = Vec::new();

    if has_writes {
        states.push("init".to_string());
        transitions.push(crate::behavior::types::Transition {
            from: "idle".into(),
            to: "init".into(),
            trigger: crate::behavior::types::Trigger {
                kind: "first_write".into(),
                register_offset: None,
                value: None,
            },
        });
    }

    if has_polling {
        states.push("running".to_string());
        transitions.push(crate::behavior::types::Transition {
            from: "init".into(),
            to: "running".into(),
            trigger: crate::behavior::types::Trigger {
                kind: "polling_start".into(),
                register_offset: None,
                value: None,
            },
        });
    }

    states.push("unknown".to_string());

    Some(crate::behavior::types::StateMachine {
        states,
        transitions,
    })
}
