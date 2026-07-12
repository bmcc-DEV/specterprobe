pub mod qemu;
pub mod redox;

use crate::behavior::types::BehaviorOutput;
use std::path::Path;

pub fn generate_drivers(output: &BehaviorOutput, output_dir: &Path) -> anyhow::Result<()> {
    if output.devices.is_empty() {
        tracing::info!("Driver Generation: no devices to generate drivers for");
        return Ok(());
    }

    let drivers_dir = output_dir.join("drivers");
    std::fs::create_dir_all(&drivers_dir)?;

    tracing::info!(
        "Driver Generation: generating {} Redox OS drivers",
        output.devices.len()
    );

    for device in &output.devices {
        redox::generate_driver(device, &drivers_dir)?;
    }

    tracing::info!("Drivers generated in {}", drivers_dir.display());
    Ok(())
}

pub fn generate_emulators(output: &BehaviorOutput, output_dir: &Path) -> anyhow::Result<()> {
    if output.devices.is_empty() {
        tracing::info!("Emulator Generation: no devices to generate emulators for");
        return Ok(());
    }

    tracing::info!(
        "Emulator Generation: generating {} QEMU devices",
        output.devices.len()
    );

    for device in &output.devices {
        qemu::generate_device(device, output_dir)?;
    }

    tracing::info!("QEMU devices generated in {:?}", output_dir.join("devices"));
    Ok(())
}
