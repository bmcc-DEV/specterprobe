pub mod backend;
pub mod common;
pub mod dsp;
pub mod gpu;
pub mod isp;
pub mod types;

use crate::compat::types::CompatOutput;
use crate::lift::types::Function;
use crate::mmio::types::MmioMap;

pub fn detect_all(functions: &[Function], mmio: &MmioMap) -> CompatOutput {
    let instructions: Vec<&crate::lift::types::Instruction> = functions
        .iter()
        .flat_map(|f| f.blocks.iter().flat_map(|b| b.instructions.iter()))
        .collect();

    let flattened: Vec<crate::lift::types::Instruction> = instructions.iter().map(|i| (*i).clone()).collect();

    tracing::info!("Compatibility Mode: scanning {} instructions", flattened.len());

    let gpu = gpu::detect_gpu(&flattened, &mmio.regions);
    let isp = isp::detect_isp(&flattened);
    let dsp = dsp::detect_dsp(&flattened);

    let mut backend_code = String::new();

    if let Some(ref g) = gpu {
        tracing::info!("  GPU detected: {} queues, {} buffers", g.queues.len(), g.buffers.len());
        backend_code.push_str(&backend::generate_gpu_backend(g));
    }
    if let Some(ref is) = isp {
        tracing::info!("  ISP detected: {} stages, res={:?}", is.pipeline.len(), is.resolution);
        backend_code.push_str(&backend::generate_isp_backend(is));
    }
    if let Some(ref d) = dsp {
        tracing::info!("  DSP detected: {}Hz, {}ch, pingpong={}", d.sample_rate.unwrap_or(0), d.channels.unwrap_or(0), d.ping_pong);
        backend_code.push_str(&backend::generate_dsp_backend(d));
    }

    let output = CompatOutput {
        gpu,
        isp,
        dsp,
        backend_code: if backend_code.is_empty() {
            None
        } else {
            Some(backend_code)
        },
    };

    if !output.has_any() {
        tracing::info!("  No GPU/ISP/DSP patterns detected");
    }

    output
}
