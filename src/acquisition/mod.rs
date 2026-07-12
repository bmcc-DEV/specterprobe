pub mod android;
pub mod dtb;
pub mod extract;
pub mod probe;
pub mod sparse;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcquireConfig {
    pub firmware_path: PathBuf,
    pub output_dir: PathBuf,
    pub extract_fs: bool,
    pub adb_probe: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelInfo {
    pub version: Option<String>,
    pub config: Vec<String>,
    pub dtb: Option<DtbInfo>,
    pub modules: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DtbInfo {
    pub compatible: Vec<String>,
    pub model: Option<String>,
    pub mmio_regions: Vec<MmioRegion>,
    pub irqs: Vec<IrqMap>,
    pub clocks: Vec<ClockMap>,
    pub gpios: Vec<GpioMap>,
    pub i2c_buses: Vec<I2cBus>,
    pub spi_buses: Vec<SpiBus>,
    pub dma_controllers: Vec<DmaController>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MmioRegion {
    pub address: u64,
    pub size: u64,
    pub peripheral: Option<String>,
    pub compatible: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrqMap {
    pub irq: u32,
    pub peripheral: String,
    pub flags: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClockMap {
    pub clock_id: u32,
    pub name: Option<String>,
    pub frequency: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpioMap {
    pub bank: u32,
    pub base: u64,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct I2cBus {
    pub bus_id: u32,
    pub address: u64,
    pub clock: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpiBus {
    pub bus_id: u32,
    pub address: u64,
    pub chip_select: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DmaController {
    pub address: u64,
    pub channels: u32,
    pub interrupts: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverInfo {
    pub name: String,
    pub module: Option<String>,
    pub device_type: String,
    pub dt_compatible: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HalInfo {
    pub name: String,
    pub version: String,
    pub interfaces: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirmwareFile {
    pub path: PathBuf,
    pub size: u64,
    pub sha256: String,
    pub category: FirmwareCategory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FirmwareCategory {
    Kernel,
    Driver,
    Firmware,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcquireOutput {
    pub kernel: KernelInfo,
    pub drivers: Vec<DriverInfo>,
    pub hals: Vec<HalInfo>,
    pub firmwares: Vec<FirmwareFile>,
    pub partitions: Vec<PartitionInfo>,
    pub raw_output: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionInfo {
    pub name: String,
    pub size: u64,
    pub fs_type: String,
    pub extracted_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeOutput {
    pub properties: HashMap<String, String>,
    pub dmesg: String,
    pub logcat: String,
    pub kernel_version: Option<String>,
    pub hardware: Option<String>,
    pub board: Option<String>,
}

pub fn acquire(config: &AcquireConfig) -> anyhow::Result<AcquireOutput> {
    let firmware_path = &config.firmware_path;
    if !firmware_path.exists() {
        anyhow::bail!("Firmware path does not exist: {}", firmware_path.display());
    }

    let output_dir = &config.output_dir;
    std::fs::create_dir_all(output_dir)?;

    let partitions = if firmware_path.is_dir() {
        android::process_firmware_dir(firmware_path, output_dir)?
    } else if firmware_path.extension().map_or(false, |e| e == "zip") {
        android::process_firmware_zip(firmware_path, output_dir)?
    } else {
        anyhow::bail!("Expected a .zip file or directory containing firmware images");
    };

    let dtb_info = partitions
        .iter()
        .find(|p| p.name == "dtb" || p.name == "dtbo")
        .and_then(|p| p.extracted_path.as_ref())
        .and_then(|p| dtb::parse_dtb_file(p).ok());

    let extracted_files = if config.extract_fs {
        extract::extract_all_partitions(&partitions, output_dir)?
    } else {
        vec![]
    };

    let categorized = extract::categorize_files(&extracted_files);

    let probe_output = if config.adb_probe {
        probe::run_adb_probe()?
    } else {
        ProbeOutput::default()
    };

    let kernel_version = probe_output.kernel_version.clone().or_else(|| {
        for p in &partitions {
            if p.name == "boot" || p.name == "kernel" {
                if let Ok(v) = android::detect_kernel_version(p) {
                    return Some(v);
                }
            }
            if p.fs_type == "bootimage" {
                if let Ok(v) = android::detect_kernel_version(p) {
                    return Some(v);
                }
            }
        }
        None
    });

    let kernel = KernelInfo {
        version: kernel_version,
        config: categorized.kernel_configs,
        dtb: dtb_info,
        modules: categorized.modules,
    };

    Ok(AcquireOutput {
        kernel,
        drivers: categorized.drivers,
        hals: categorized.hals,
        firmwares: categorized.firmwares,
        partitions,
        raw_output: probe_output.properties,
    })
}
