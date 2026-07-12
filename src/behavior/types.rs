use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AccessType {
    Read,
    Write,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bitfield {
    pub offset: u8,
    pub width: u8,
    pub name: Option<String>,
    pub values: Vec<(u64, String)>,
    pub observed_mask: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterModel {
    pub offset: u32,
    pub name: Option<String>,
    pub access: AccessType,
    pub width: u8,
    pub observed_writes: Vec<u64>,
    pub observed_reads: Vec<u64>,
    pub bitfields: Vec<Bitfield>,
    pub polling: bool,
    pub count: usize,
    pub purpose: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trigger {
    pub kind: String,
    pub register_offset: Option<u32>,
    pub value: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transition {
    pub from: String,
    pub to: String,
    pub trigger: Trigger,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateMachine {
    pub states: Vec<String>,
    pub transitions: Vec<Transition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessSequence {
    pub function: String,
    pub accesses: Vec<SequencedAccess>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequencedAccess {
    pub offset: u32,
    pub access_type: AccessType,
    pub value: Option<u64>,
    pub instruction_addr: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceModel {
    pub base: u64,
    pub name: Option<String>,
    pub classification: Option<String>,
    pub registers: Vec<RegisterModel>,
    pub state_machine: Option<StateMachine>,
    pub init_sequence: Vec<String>,
    pub sequences: Vec<AccessSequence>,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorOutput {
    pub devices: Vec<DeviceModel>,
}

impl BehaviorOutput {
    pub fn new() -> Self {
        Self { devices: Vec::new() }
    }
}
