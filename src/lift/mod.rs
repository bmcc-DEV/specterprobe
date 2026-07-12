pub mod analysis;
pub mod cfg;
pub mod disasm;
pub mod ir;
pub mod pe;
pub mod types;

use crate::lift::types::LiftOutput;

fn is_pe(data: &[u8]) -> bool {
    data.len() > 0x40 && data[0..2] == [0x4D, 0x5A]
}

fn extract_pe_code(data: &[u8]) -> Vec<u8> {
    let mut cursor = std::io::Cursor::new(data);
    match pe::parse_pe(&mut cursor) {
        Ok(info) => {
            tracing::info!("PE image: {} sections, entry RVA=0x{:x}", info.sections.len(), info.entry_point_rva);
            let mut cursor = std::io::Cursor::new(data);
            match pe::extract_arm64_code(&mut cursor, &info) {
                Ok(code) => {
                    tracing::info!("Extracted .text section: {} bytes", code.len());
                    code
                }
                Err(e) => {
                    tracing::warn!("PE .text extraction failed: {e}");
                    data.to_vec()
                }
            }
        }
        Err(e) => {
            tracing::debug!("Not a PE image: {e}");
            data.to_vec()
        }
    }
}

pub fn lift_binary(data: &[u8]) -> LiftOutput {
    let arch = disasm::detect_arch(data);

    let (lift_data, _is_pe) = if is_pe(data) {
        tracing::info!("Detected PE image, extracting code section");
        (extract_pe_code(data), true)
    } else {
        (data.to_vec(), false)
    };

    tracing::info!("Disassembling {:?} binary ({} bytes)", arch, lift_data.len());

    let instructions = disasm::disassemble(&lift_data, arch);
    if instructions.is_empty() {
        tracing::warn!("No instructions disassembled");
        return LiftOutput {
            arch,
            functions: vec![],
            total_instructions: 0,
            lifted_functions: 0,
            ir_text: String::new(),
            entry_point: 0,
        };
    }

    tracing::info!("Disassembled {} instructions", instructions.len());

    let functions = cfg::reconstruct_cfg(&instructions);
    tracing::info!("Found {} functions", functions.len());

    let ir_text = ir::generate_ir(&functions, "lifted");
    tracing::info!("Generated {} bytes of LLVM IR", ir_text.len());

    let analysis = analysis::analyze(&functions);
    tracing::info!(
        "Analysis: {} lifted, {} unknown, {} MMIO candidates, {} syscalls",
        analysis.lifted_count,
        analysis.unknown_count,
        analysis.mmio_candidates.len(),
        analysis.syscalls.len()
    );

    let entry_point = functions.first().map(|f| f.entry).unwrap_or(0);
    let func_count = functions.len();

    LiftOutput {
        arch,
        functions,
        total_instructions: instructions.len(),
        lifted_functions: func_count,
        ir_text,
        entry_point,
    }
}
