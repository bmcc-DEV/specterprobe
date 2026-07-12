use crate::behavior::types::{AccessType, DeviceModel, RegisterModel};

pub fn generate_driver(device: &DeviceModel, output_dir: &std::path::Path) -> anyhow::Result<()> {
    let name = device
        .name
        .clone()
        .unwrap_or_else(|| format!("device_{:x}", device.base));
    let dev_dir = output_dir.join(&name);
    std::fs::create_dir_all(dev_dir.join("src"))?;

    generate_cargo_toml(&name, &dev_dir)?;
    generate_main_rs(device, &name, &dev_dir)?;
    generate_build_sh(&dev_dir)?;
    generate_test_harness(device, &name, &dev_dir)?;

    tracing::info!("  Generated driver: {}", name);
    Ok(())
}

fn generate_cargo_toml(name: &str, dir: &std::path::Path) -> anyhow::Result<()> {
    let content = format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"

[dependencies]
redox_syscall = "0.9"
"#
    );
    std::fs::write(dir.join("Cargo.toml"), content)?;
    Ok(())
}

fn generate_build_sh(dir: &std::path::Path) -> anyhow::Result<()> {
    let content = r#"#!/bin/sh
cargo build --target aarch64-unknown-redox
cp target/aarch64-unknown-redox/debug/$(basename $(pwd)) ./$(basename $(pwd)).bin
echo "Driver built: $(basename $(pwd)).bin"
"#;
    std::fs::write(dir.join("build.sh"), content)?;
    Ok(())
}

fn generate_main_rs(
    device: &DeviceModel,
    name: &str,
    dir: &std::path::Path,
) -> anyhow::Result<()> {
    let base = device.base;
    let size = 0x1000u64;
    let reg_defs = generate_reg_defs(device);
    let init_seq = generate_init_sequence(device);
    let poll_handlers = generate_poll_handlers(device);
    let state_machine = generate_state_machine(device);
    let scheme_dispatch = generate_scheme_dispatch(device);

    let dev_name_sanitized: String = name.chars().map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' }).collect();
    let scheme_literal = format!(":{}", dev_name_sanitized);

    let content = format!(
        r#"// Driver gerado pelo EAEA Specter Probe — Redox OS
// Device base: 0x{base:x}
// Classification: {class}
// Confidence: {conf}

extern crate alloc;
use alloc::format;
use alloc::string::String;
use redox_syscall::io::Io;
use redox_syscall::io::mmio::Mmio;
use redox_syscall::data::Map;
use redox_syscall::data::GlobalSchemes;

{reg_defs}

fn main() {{
    // Abre o scheme de memória
    let mem_fd = redox_syscall::call::open(
        GlobalSchemes::Memory.as_str(),
        redox_syscall::flag::O_RDWR,
    ).expect("failed to open memory scheme");

    // Mapeia MMIO
    let map = Map {{
        offset: {base},
        size: {size},
        flags: redox_syscall::data::MapFlags::MAP_SHARED | redox_syscall::data::MapFlags::MAP_PHYSICAL,
        address: 0,
    }};
    let mmio_base = unsafe {{
        redox_syscall::call::fmap(mem_fd, &map).expect("failed to map MMIO")
    }} as *mut u64;

    let mmio = unsafe {{ &mut *(mmio_base as *mut Mmio<u64>) }};

    // Cria o scheme do device
    let scheme_path = "{scheme_literal}";
    let scheme_fd = redox_syscall::call::open(
        scheme_path,
        redox_syscall::flag::O_CREAT | redox_syscall::flag::O_RDWR,
    ).expect("failed to create scheme");
    
    {state_machine}

    // Loop principal de eventos
    loop {{
        let mut sqe = redox_syscall::schemev2::Sqe::default();
        match redox_syscall::call::read(scheme_fd, unsafe {{
            core::slice::from_raw_parts_mut(
                &mut sqe as *mut _ as *mut u8,
                core::mem::size_of::<redox_syscall::schemev2::Sqe>(),
            )
        }}) {{
            Ok(0) => break,
            Ok(_) => {{
                {scheme_dispatch}
            }}
            Err(_) => break,
        }}
    }}
}}

fn handle_read(mmio: &Mmio<u64>, reg_offset: u64) -> u64 {{
    unsafe {{
        let reg = (mmio as *const Mmio<u64> as *mut u8).add(reg_offset as usize) as *mut Mmio<u64>;
        (*reg).read()
    }}
}}

fn handle_write(mmio: &mut Mmio<u64>, reg_offset: u64, value: u64) {{
    unsafe {{
        let reg = (mmio as *mut Mmio<u64> as *mut u8).add(reg_offset as usize) as *mut Mmio<u64>;
        (*reg).write(value);
    }}
}}

{init_seq}

{poll_handlers}
"#,
        base = base,
        size = size,
        class = device.classification.as_deref().unwrap_or("unknown"),
        conf = device.confidence,
        reg_defs = reg_defs,
        init_seq = init_seq,
        poll_handlers = poll_handlers,
        state_machine = state_machine,
        scheme_dispatch = scheme_dispatch,
    );

    std::fs::write(dir.join("src").join("main.rs"), content)?;
    Ok(())
}

fn generate_reg_defs(device: &DeviceModel) -> String {
    let mut lines = String::new();
    lines.push_str("// Registradores identificados pelo EAEA\n");

    for reg in &device.registers {
        let purpose = reg.purpose.as_deref().unwrap_or("unknown");
        let access_str = match reg.access {
            AccessType::Read => "READ_ONLY",
            AccessType::Write => "WRITE_ONLY",
        };
        let const_name = format!("REG_{:x}", reg.offset);
        lines.push_str(&format!(
            "/// {purpose} register at +0x{offset:x} ({access_str})\n",
            offset = reg.offset,
        ));
        lines.push_str(&format!(
            "const {const_name}: u64 = 0x{offset:x};\n",
            offset = reg.offset,
        ));
    }

    lines
}

fn generate_init_sequence(device: &DeviceModel) -> String {
    let mut lines = String::new();
    if device.init_sequence.is_empty() {
        lines.push_str("// Nenhuma sequência de init detectada\n");
        return lines;
    }

    lines.push_str("// Sequência de inicialização\n");
    lines.push_str("pub fn init_device(mmio: &mut Mmio<u64>) {\n");
    for step in &device.init_sequence {
        lines.push_str(&format!("    // {}\n", step));
    }
    lines.push_str("}\n");
    lines
}

fn generate_poll_handlers(device: &DeviceModel) -> String {
    let mut lines = String::new();
    let polling_regs: Vec<&RegisterModel> = device.registers.iter().filter(|r| r.polling).collect();

    if polling_regs.is_empty() {
        return lines;
    }

    lines.push_str("// Handlers de polling (detectados pelo EAEA)\n");
    for reg in &polling_regs {
        let fn_name = format!("poll_reg_{:x}", reg.offset);
        let const_name = format!("REG_{:x}", reg.offset);
        lines.push_str(&format!(
            "pub fn {fn_name}(mmio: &Mmio<u64>) -> u64 {{\n",
        ));
        lines.push_str(&format!(
            "    // Polling detectado: {count} leituras em +0x{offset:x}\n",
            count = reg.count,
            offset = reg.offset,
        ));
        lines.push_str(&format!(
            "    handle_read(mmio, {const_name})\n",
        ));
        lines.push_str("}\n\n");
    }

    lines
}

fn generate_state_machine(device: &DeviceModel) -> String {
    let mut lines = String::new();
    if let Some(ref sm) = device.state_machine {
        lines.push_str("// Máquina de estados — início\n");
        lines.push_str(&format!(
            "let mut state = \"{}\";\n",
            sm.states.first().map(|s| s.as_str()).unwrap_or("idle")
        ));
        lines.push_str("loop {\n");
        lines.push_str("    match state {\n");
        for state in &sm.states {
            lines.push_str(&format!("        \"{}\" => {{\n", state));
            lines.push_str(&format!("            // Transições do estado {}\n", state));
            for trans in &sm.transitions {
                if trans.from == *state {
                    lines.push_str(&format!(
                        "            // -> {} via {}\n",
                        trans.to, trans.trigger.kind
                    ));
                }
            }
            lines.push_str("        }\n");
        }
        lines.push_str("        _ => break,\n");
        lines.push_str("    }\n");
        lines.push_str("}\n");
    } else {
        lines.push_str("// Sem máquina de estados\n");
    }
    lines
}

fn generate_scheme_dispatch(device: &DeviceModel) -> String {
    let mut lines = String::new();
    lines.push_str("use redox_syscall::schemev2::{Opcode, Cqe, CqeOpcode};\n");
    lines.push_str("let opcode = sqe.opcode;\n");
    lines.push_str("let mut cqe = Cqe::default();\n");
    lines.push_str("match Opcode::try_from_raw(opcode) {\n");
    lines.push_str("    Some(Opcode::Read) => {\n");

    let polling_regs: Vec<&RegisterModel> =
        device.registers.iter().filter(|r| r.polling).collect();
    if polling_regs.is_empty() {
        lines.push_str("        cqe.result = 0;\n");
    } else {
        let reg = &polling_regs[0];
        let fn_name = format!("poll_reg_{:x}", reg.offset);
        lines.push_str(&format!(
            "        let val = {fn_name}(&mmio);\n",
        ));
        lines.push_str("        cqe.result = val;\n");
    }

    lines.push_str("    }\n");
    lines.push_str("    Some(Opcode::Write) => {\n");
    if device.init_sequence.is_empty() {
        lines.push_str("        cqe.result = 0;\n");
    } else {
        lines.push_str("        init_device(&mut mmio);\n");
        lines.push_str("        cqe.result = 0;\n");
    }
    lines.push_str("    }\n");
    lines.push_str("    Some(Opcode::Close) => {\n");
    lines.push_str("        cqe.result = 0;\n");
    lines.push_str("    }\n");
    lines.push_str("    _ => {}\n");
    lines.push_str("}\n");

    lines.push_str("cqe.flags = CqeOpcode::RespondRegular as u8;\n");
    lines.push_str("cqe.tag = sqe.tag;\n");
    lines.push_str("let _ = redox_syscall::call::write(scheme_fd, unsafe {\n");
    lines.push_str("    core::slice::from_raw_parts(\n");
    lines.push_str("        &cqe as *const _ as *const u8,\n");
    lines.push_str("        core::mem::size_of::<Cqe>(),\n");
    lines.push_str("    )\n");
    lines.push_str("});\n");

    lines
}

fn generate_test_harness(
    device: &DeviceModel,
    name: &str,
    dir: &std::path::Path,
) -> anyhow::Result<()> {
    let base = device.base;
    let polling_regs: Vec<&RegisterModel> = device.registers.iter().filter(|r| r.polling).collect();

    let mut poll_test = String::new();
    for reg in &polling_regs {
        let const_name = format!("REG_{:x}", reg.offset);
        poll_test.push_str(&format!(
            "    println!(\"--- Polling +0x{:x} (detectado: {} reads) ---\");\n",
            reg.offset, reg.count
        ));
        let iterations = reg.count.min(5);
        poll_test.push_str(&format!(
            "    for i in 0..{} {{\n", iterations
        ));
        poll_test.push_str(&format!(
            "        let v = handle_read(&mmio_mem, {const_name});\n",
        ));
        poll_test.push_str("        println!(\"  poll #{i} => 0x{v:x}\", i=i, v=v);\n");
        poll_test.push_str("    }\n");
    }

    let mut reg_consts = String::new();
    for reg in &device.registers {
        let const_name = format!("REG_{:x}", reg.offset);
        reg_consts.push_str(&format!(
            "const {const_name}: u64 = 0x{offset:x};\n",
            offset = reg.offset,
        ));
    }

    let mut reg_setup = String::new();
    for reg in &device.registers {
        let _cn = format!("REG_{:x}", reg.offset);
        if reg.purpose.as_deref() == Some("status") {
            reg_setup.push_str(&format!(
                "        let reg_{:x} = mmio_mem.as_mut_ptr().add({} as usize) as *mut u64;\n",
                reg.offset, reg.offset / 8
            ));
            reg_setup.push_str(&format!(
                "        *reg_{:x} = 0x01; // bit 0 = ready\n",
                reg.offset
            ));
        }
    }

    let content = format!(
        r#"// Test harness — {name}
// Roda em Linux: rustc test_harness.rs -o test_harness && ./test_harness
// Device base: 0x{base:x}

{reg_consts}

fn main() {{
    println!("=== Test Harness: {name} (base=0x{base:x}) ===");

    let mut mmio_mem = vec![0u64; 512];

    unsafe {{
{reg_setup}
    }}

    println!("\\n--- Leituras de registradores detectados ---");
    for reg in &[
{polling_iter}
    ] {{
        let v = handle_read(&mmio_mem, *reg);
        println!("  +0x{{reg:x}} => 0x{{v:x}}", reg=reg, v=v);
    }}

{poll_test}
    println!("\\n=== Teste concluído ===");
}}

fn handle_read(mmio: &[u64], reg_offset: u64) -> u64 {{
    unsafe {{
        let idx = (reg_offset / 8) as usize;
        if idx < mmio.len() {{
            mmio.as_ptr().add(idx).read_volatile()
        }} else {{
            u64::MAX
        }}
    }}
}}

fn handle_write(mmio: &mut [u64], reg_offset: u64, value: u64) {{
    unsafe {{
        let idx = (reg_offset / 8) as usize;
        if idx < mmio.len() {{
            mmio.as_mut_ptr().add(idx).write_volatile(value);
        }}
    }}
}}
"#,
        name = name,
        base = base,
        reg_consts = reg_consts,
        reg_setup = reg_setup,
        polling_iter = polling_regs.iter().map(|r| format!("        REG_{:x},", r.offset)).collect::<Vec<_>>().join("\n"),
        poll_test = poll_test,
    );

    std::fs::write(dir.join("test_harness.rs"), content)?;
    Ok(())
}
