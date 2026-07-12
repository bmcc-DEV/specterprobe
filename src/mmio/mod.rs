pub mod classify;
pub mod scanner;
pub mod types;

use crate::acquisition::DtbInfo;
use crate::lift::types::Function;
use crate::mmio::types::MmioMap;

pub fn discover(
    functions: &[Function],
    dtb: Option<&DtbInfo>,
) -> MmioMap {
    tracing::info!("MMIO Discovery: scanning {} functions", functions.len());

    let accesses = scanner::scan_functions(functions);

    if accesses.is_empty() {
        tracing::info!("No MMIO accesses detected");
        return MmioMap::new();
    }

    tracing::info!("Found {} raw MMIO accesses", accesses.len());

    let result = classify::classify(accesses, dtb);

    let dtb_hits = result.regions.iter().filter(|r| r.dtb_compatible.is_some()).count();
    tracing::info!(
        "MMIO Discovery: {} regions ({} DTB-matched, {} classified)",
        result.regions.len(),
        dtb_hits,
        result.regions.iter().filter(|r| r.classification.is_some()).count()
    );

    result
}
