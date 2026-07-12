use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AccessType {
    Read,
    Write,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MmioAccess {
    pub address: u64,
    pub size: u8,
    pub access_type: AccessType,
    pub instruction_addr: u64,
    pub function_name: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MmioRegion {
    pub base: u64,
    pub size: u64,
    pub accesses: Vec<MmioAccess>,
    pub classification: Option<String>,
    pub dtb_compatible: Option<String>,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MmioMap {
    pub regions: Vec<MmioRegion>,
    pub raw_accesses: Vec<MmioAccess>,
}

impl MmioMap {
    pub fn new() -> Self {
        Self {
            regions: Vec::new(),
            raw_accesses: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.regions.is_empty() && self.raw_accesses.is_empty()
    }
}
