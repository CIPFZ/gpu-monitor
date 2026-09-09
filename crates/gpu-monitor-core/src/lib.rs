//! GPU monitoring through NVIDIA Management Library (NVML).
//!
//! Each sample carries a timestamp, per-device failures and unavailable metrics.
//! Initialization happens on the first sample and is retried after failures.
//!
//! # Example
//! ```no_run
//! use gpu_monitor_core::MonitorService;
//!
//! let mut monitor = MonitorService::new();
//! let snapshot = monitor.sample();
//! for gpu in snapshot.gpus {
//!     println!("{}: {:?}% usage", gpu.device.name, gpu.metrics.gpu_utilization);
//! }
//! if let Some(error) = snapshot.error {
//!     eprintln!("{error}");
//! }
//! ```

mod device;
mod error;
pub mod metrics;
mod monitor;
mod process;
mod process_metadata;
mod service;

pub use device::{DeviceInfo, MemoryInfo};
pub use error::{ErrorKind, SampleError};
pub use metrics::GpuMetrics;
pub use process::{GpuProcess, ProcessType};
pub use service::MonitorService;

/// Complete GPU information for a single sample. Missing metrics are never zero-filled.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
pub struct GpuInfo {
    pub device: DeviceInfo,
    pub metrics: GpuMetrics,
    pub memory: Option<MemoryInfo>,
    /// Results from successful process queries; consult `issues` for partial failures.
    pub processes: Vec<GpuProcess>,
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub sampled_at_ms: u64,
    pub issues: Vec<MetricIssue>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
pub struct MetricIssue {
    pub metric: String,
    pub error: SampleError,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
pub struct DeviceFailure {
    pub index: u32,
    pub uuid: Option<String>,
    pub error: SampleError,
}

/// A sampling round. Healthy devices remain available when other devices fail.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
pub struct MonitorSnapshot {
    /// Wire format version. Omitted by older recordings and defaulted to version 1.
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    /// Unix timestamp in milliseconds. Unchanged cached responses have the same timestamp.
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub sampled_at_ms: u64,
    pub gpus: Vec<GpuInfo>,
    pub failures: Vec<DeviceFailure>,
    /// Initialization/enumeration error, independent of per-device failures.
    pub error: Option<SampleError>,
}

/// Current version of the snapshot JSON contract.
pub const SCHEMA_VERSION: u32 = 1;

fn schema_version() -> u32 {
    SCHEMA_VERSION
}
