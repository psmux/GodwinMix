//! `gpu.status` and `machine.status`: what the machine a station runs on is
//! doing, for a headend wall. The GPU is read from NVML, or from
//! `nvidia-smi` where NVML cannot be opened, once a second and only while a
//! client keeps asking; a machine with no NVIDIA GPU answers with no GPUs
//! and a note saying so.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// `gpu.status`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GpuStatusRequest {
    /// Only the history after this time, unix milliseconds: what a client
    /// that already holds the rest asks for. Left out, the whole history.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since_ms: Option<u64>,
}

/// One reading of one GPU. A number the driver would not give is left out.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GpuSample {
    /// Unix milliseconds.
    pub at_ms: u64,
    /// The 3D and compute engines, percent of the last sample period.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu_pct: Option<u32>,
    /// NVENC, percent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoder_pct: Option<u32>,
    /// NVDEC, percent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decoder_pct: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_used_mib: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_total_mib: Option<u64>,
    /// Open NVENC sessions, every process on the machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoder_sessions: Option<u32>,
    /// The sessions' average frame rate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoder_fps: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature_c: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power_w: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power_limit_w: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graphics_mhz: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_mhz: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video_mhz: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fan_pct: Option<u32>,
}

/// One GPU: what it is, its newest reading and the ones before it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GpuDevice {
    pub index: u32,
    /// `NVIDIA GeForce RTX 3060`.
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub driver: Option<String>,
    pub now: GpuSample,
    /// One a second, oldest first, the last two minutes at most.
    #[serde(default)]
    pub history: Vec<GpuSample>,
}

/// `gpu.status`'s answer.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GpuStatus {
    /// `nvml`, `nvidia-smi`, or `none`.
    pub source: String,
    /// Why there are no GPUs, or what is missing, in one sentence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// How often a reading is taken while someone asks.
    pub every_ms: u32,
    pub gpus: Vec<GpuDevice>,
}

/// `machine.status`'s answer: the box, not one process.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MachineStatus {
    pub host_name: String,
    pub cores: u32,
    /// Every process on the machine, percent of all cores, over the last
    /// second. Left out until two readings have been taken.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_pct: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_total_mib: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_available_mib: Option<u64>,
    /// Since the machine booted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uptime_secs: Option<u64>,
    /// Since this process started.
    pub process_uptime_secs: u64,
    /// One, five and fifteen minutes, where the system keeps them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load_average: Option<[f64; 3]>,
}
