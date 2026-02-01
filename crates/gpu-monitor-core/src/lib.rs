//! GPU Monitor Core Library
//!
//! Provides GPU monitoring functionality through NVIDIA Management Library (NVML).
//!
//! # Features
//! - GPU device information
//! - Real-time metrics (usage, memory, temperature, power)
//! - Process monitoring
//!
//! # Example
//! ```no_run
//! use gpu_monitor_core::GpuMonitor;
//!
//! let monitor = GpuMonitor::new()?;
//! let gpus = monitor.get_all_gpu_info()?;
//! for gpu in gpus {
//!     println!("{}: {}% usage", gpu.name, gpu.metrics.gpu_utilization);
//! }
//! ```

mod device;
mod error;
pub mod metrics;
mod monitor;
mod process;

pub use device::{DeviceInfo, MemoryInfo};
pub use error::{Error, Result};
pub use metrics::GpuMetrics;
pub use monitor::GpuMonitor;
pub use process::GpuProcess;

/// Complete GPU information including device info, metrics, and processes
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GpuInfo {
    /// Device information (name, UUID, etc.)
    pub device: DeviceInfo,
    /// Current metrics (usage, temperature, etc.)
    pub metrics: GpuMetrics,
    /// Memory information
    pub memory: MemoryInfo,
    /// Processes using this GPU
    pub processes: Vec<GpuProcess>,
}
