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
