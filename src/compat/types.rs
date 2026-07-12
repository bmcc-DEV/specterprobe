use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueModel {
    pub doorbell_register: u64,
    pub head_register: Option<u64>,
    pub tail_register: Option<u64>,
    pub descriptor_size: u32,
    pub queue_depth: u32,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BufferModel {
    pub address_register: u64,
    pub size_register: Option<u64>,
    pub stride: Option<u32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub frame_size: Option<u64>,
    pub detected_type: BufferType,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BufferType {
    CommandBuffer,
    Framebuffer,
    RawImage,
    JpegOutput,
    AudioPlayback,
    AudioCapture,
    IqSample,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStage {
    pub name: String,
    pub trigger_register: Option<u64>,
    pub input_buffer_reg: Option<u64>,
    pub output_buffer_reg: Option<u64>,
    pub completion_irq: Option<u32>,
    pub latency_us: Option<u64>,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuModel {
    pub queues: Vec<QueueModel>,
    pub buffers: Vec<BufferModel>,
    pub pipeline: Vec<PipelineStage>,
    pub compute_units: u32,
    pub has_compute: bool,
    pub has_3d: bool,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IspModel {
    pub resolution: Option<(u32, u32)>,
    pub input_format: String,
    pub output_format: String,
    pub black_level: Option<u16>,
    pub pipeline: Vec<PipelineStage>,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DspModel {
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub bit_depth: Option<u8>,
    pub buffer_size: Option<u32>,
    pub ping_pong: bool,
    pub volume_register: Option<u64>,
    pub has_modem: bool,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatOutput {
    pub gpu: Option<GpuModel>,
    pub isp: Option<IspModel>,
    pub dsp: Option<DspModel>,
    pub backend_code: Option<String>,
}

impl CompatOutput {
    pub fn new() -> Self {
        Self {
            gpu: None,
            isp: None,
            dsp: None,
            backend_code: None,
        }
    }

    pub fn has_any(&self) -> bool {
        self.gpu.is_some() || self.isp.is_some() || self.dsp.is_some()
    }
}
