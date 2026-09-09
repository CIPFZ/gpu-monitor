//! GPU process information

use serde::{Deserialize, Serialize};

/// Information about a process using the GPU
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
pub struct GpuProcess {
    /// Process ID
    pub pid: u32,
    /// Process name (executable name)
    pub name: String,
    /// GPU memory used by this process in bytes
    #[cfg_attr(feature = "typescript", ts(type = "number | null"))]
    pub gpu_memory: Option<u64>,
    /// Process type
    pub process_type: ProcessType,
    /// Local account name from /etc/passwd; remote-only accounts retain the UID.
    #[serde(default)]
    pub user: Option<String>,
    /// Real UID reported by the operating system.
    #[serde(default)]
    pub uid: Option<u32>,
    /// Argument boundaries are preserved; restricted or over-1-MiB values are unknown.
    #[serde(default)]
    pub command: Option<Vec<String>>,
    /// Unix start time in milliseconds. Combine with PID to identify a process.
    #[serde(default)]
    #[cfg_attr(feature = "typescript", ts(type = "number | null"))]
    pub started_at_ms: Option<u64>,
    /// Age at sampling time, measured against system uptime.
    #[serde(default)]
    #[cfg_attr(feature = "typescript", ts(type = "number | null"))]
    pub elapsed_seconds: Option<u64>,
}

impl GpuProcess {
    /// Get GPU memory usage in MiB
    pub fn gpu_memory_mib(&self) -> Option<u64> {
        self.gpu_memory.map(|value| value / (1024 * 1024))
    }
}

/// Type of GPU process
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
pub enum ProcessType {
    /// Graphics/rendering process
    Graphics,
    /// Compute process (CUDA, OpenCL)
    Compute,
    /// Both graphics and compute
    Mixed,
    /// Unknown process type
    #[default]
    Unknown,
}

impl std::fmt::Display for ProcessType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Graphics => write!(f, "Graphics"),
            Self::Compute => write!(f, "Compute"),
            Self::Mixed => write!(f, "Mixed"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

impl ProcessType {
    /// Short label for UI display
    pub fn short_label(&self) -> &'static str {
        match self {
            Self::Graphics => "Gfx",
            Self::Compute => "Comp",
            Self::Mixed => "Mix",
            Self::Unknown => "?",
        }
    }
}
