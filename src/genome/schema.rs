use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BitfieldDef {
    pub offset: u8,
    pub width: u8,
    pub name: Option<String>,
    pub values: Vec<(u64, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterGenome {
    pub offset: u32,
    pub name: Option<String>,
    pub width: u8,
    pub purpose: String,
    pub access: String,
    pub polling: bool,
    pub observed_reads: usize,
    pub observed_writes: usize,
    pub bitfields: Vec<BitfieldDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusInfo {
    pub bus_type: String,
    pub address: u64,
    pub size: u64,
    pub irq: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorProfile {
    pub interrupt: String,
    pub has_polling: bool,
    pub state_machine: Vec<String>,
    pub init_sequence: Vec<String>,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverRef {
    pub os: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphRef {
    pub node_count: usize,
    pub edge_count: usize,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirmwareRef {
    pub name: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceGenome {
    pub version: u32,
    pub device: DeviceInfo,
    pub bus: BusInfo,
    pub behavior: BehaviorProfile,
    pub registers: Vec<RegisterGenome>,
    pub firmware: Option<FirmwareRef>,
    pub driver: Option<DriverRef>,
    pub graph: Option<GraphRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub name: String,
    pub base: u64,
    pub device_type: String,
    pub classification: Option<String>,
    pub confidence: f32,
}
