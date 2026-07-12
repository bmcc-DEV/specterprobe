use crate::acquisition::DtbInfo;
use crate::mmio::types::{AccessType, MmioAccess, MmioMap, MmioRegion};
use std::collections::HashMap;

const PAGE_SIZE: u64 = 0x1000;

pub fn classify(
    accesses: Vec<MmioAccess>,
    dtb: Option<&DtbInfo>,
) -> MmioMap {
    let region_map = aggregate_by_page(&accesses);

    let dtb_ranges: Vec<(u64, u64, String)> = dtb
        .map(|d| {
            d.mmio_regions
                .iter()
                .map(|r| {
                    let compat = r.compatible.first().cloned().unwrap_or_default();
                    (r.address, r.size, compat)
                })
                .collect()
        })
        .unwrap_or_default();

    let mut regions = Vec::new();

    for (base, page_accesses) in &region_map {
        let mut r = MmioRegion {
            base: *base,
            size: PAGE_SIZE,
            accesses: page_accesses.clone(),
            classification: None,
            dtb_compatible: None,
            confidence: 0.0,
        };

        for (dtb_addr, dtb_size, compat) in &dtb_ranges {
            if *base >= *dtb_addr && *base < dtb_addr + dtb_size {
                r.dtb_compatible = Some(compat.clone());
                r.classification = classify_by_compatible(compat);
                r.confidence = (r.confidence + 0.3).min(1.0);
                break;
            }
        }

        if r.classification.is_none() {
            r.classification = classify_by_address(*base);
        }

        r.confidence = compute_confidence(&r, &dtb_ranges);
        regions.push(r);
    }

    regions.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));

    MmioMap {
        regions,
        raw_accesses: accesses,
    }
}

fn aggregate_by_page(accesses: &[MmioAccess]) -> HashMap<u64, Vec<MmioAccess>> {
    let mut map: HashMap<u64, Vec<MmioAccess>> = HashMap::new();
    for acc in accesses {
        let page = acc.address & !(PAGE_SIZE - 1);
        map.entry(page).or_default().push(acc.clone());
    }
    map
}

fn classify_by_compatible(compat: &str) -> Option<String> {
    let compat_lower = compat.to_lowercase();
    if compat_lower.contains("gpio") {
        Some("gpio".into())
    } else if compat_lower.contains("i2c") {
        Some("i2c".into())
    } else if compat_lower.contains("spi") {
        Some("spi".into())
    } else if compat_lower.contains("uart") || compat_lower.contains("serial") {
        Some("uart".into())
    } else if compat_lower.contains("dma") {
        Some("dma".into())
    } else if compat_lower.contains("pwm") {
        Some("pwm".into())
    } else if compat_lower.contains("timer") {
        Some("timer".into())
    } else if compat_lower.contains("watchdog") || compat_lower.contains("wdt") {
        Some("watchdog".into())
    } else if compat_lower.contains("interrupt") || compat_lower.contains("gic") {
        Some("interrupt_controller".into())
    } else if compat_lower.contains("usb") {
        Some("usb".into())
    } else if compat_lower.contains("mmc") || compat_lower.contains("sdhci") {
        Some("mmc".into())
    } else if compat_lower.contains("clock") || compat_lower.contains("clk") {
        Some("clock".into())
    } else if compat_lower.contains("power") || compat_lower.contains("regulator") {
        Some("power".into())
    } else {
        Some(format!("peripheral:{}", compat))
    }
}

fn classify_by_address(address: u64) -> Option<String> {
    let page = address & !(PAGE_SIZE - 1);
    match page {
        0x1C00_0000..=0x1C00_3000 => Some("uart".into()),
        0x1C00_4000..=0x1C00_5000 => Some("timer".into()),
        0x1C00_6000..=0x1C00_7000 => Some("watchdog".into()),
        0x1C00_8000..=0x1C00_9000 => Some("dma".into()),
        0x1C01_0000..=0x1C01_1000 => Some("gpio".into()),
        0x5000_0000..=0x5010_0000 => Some("peripheral".into()),
        0x7000_0000..=0x8000_0000 => Some("dram_mmio".into()),
        _ => None,
    }
}

fn compute_confidence(region: &MmioRegion, dtb_ranges: &[(u64, u64, String)]) -> f32 {
    let mut conf = 0.3;

    let has_dtb = dtb_ranges
        .iter()
        .any(|(addr, size, _)| region.base >= *addr && region.base < addr + size);
    if has_dtb {
        conf += 0.4;
    }

    if region.classification.is_some() {
        conf += 0.2;
    }

    let write_count = region
        .accesses
        .iter()
        .filter(|a| matches!(a.access_type, AccessType::Write))
        .count();
    if write_count > 0 {
        conf += 0.1;
    }

    let high_conf = region
        .accesses
        .iter()
        .filter(|a| a.confidence > 0.7)
        .count();
    if high_conf > 0 {
        conf += 0.1 * (high_conf as f32).min(5.0) / 5.0;
    }

    conf.min(1.0)
}
