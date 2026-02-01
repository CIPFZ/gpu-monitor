//! GPU process information

use serde::{Deserialize, Serialize};

/// Information about a process using the GPU
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuProcess {
    /// Process ID
    pub pid: u32,
    /// Process name (executable name)
    pub name: String,
    /// GPU memory used by this process in bytes
    pub gpu_memory: u64,
    /// Process type
    pub process_type: ProcessType,
}

impl GpuProcess {
    /// Get GPU memory usage in MiB
    pub fn gpu_memory_mib(&self) -> u64 {
        self.gpu_memory / (1024 * 1024)
    }
}

/// Type of GPU process
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessType {
    /// Graphics/rendering process
    Graphics,
    /// Compute process (CUDA, OpenCL)
    Compute,
    /// Both graphics and compute
    Mixed,
    /// Unknown process type
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
