pub mod analyzer;
pub mod namedb;
pub mod types;

use crate::behavior::types::BehaviorOutput;
use crate::mmio::types::MmioMap;

pub fn model_devices(mmio_map: &MmioMap) -> BehaviorOutput {
    if mmio_map.regions.is_empty() {
        tracing::info!("Behavioral Modeling: no MMIO regions to model");
        return BehaviorOutput::new();
    }

    tracing::info!(
        "Behavioral Modeling: modeling {} MMIO regions",
        mmio_map.regions.len()
    );

    let devices = analyzer::build_device_models(&mmio_map.regions, &mmio_map.raw_accesses);

    for dev in &devices {
        let name = dev
            .name
            .clone()
            .unwrap_or_else(|| format!("device_{:x}", dev.base));
        tracing::info!(
            "  {}: {} registers, init steps={}, confidence={:.2}",
            name,
            dev.registers.len(),
            dev.init_sequence.len(),
            dev.confidence
        );
    }

    BehaviorOutput { devices }
}
