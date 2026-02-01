//! GPU device information types

use serde::{Deserialize, Serialize};

/// Static information about a GPU device
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// Device index (0-based)
    pub index: u32,
    /// Device name (e.g., "NVIDIA GeForce RTX 4060 Ti")
    pub name: String,
    /// Unique device identifier
    pub uuid: String,
    /// PCI bus ID
    pub pci_bus_id: String,
    /// Driver version
    pub driver_version: String,
    /// CUDA version (if available)
    pub cuda_version: Option<String>,
    /// Power limit in watts
    pub power_limit: u32,
    /// Maximum power limit in watts
    pub power_limit_max: u32,
}

/// GPU memory information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryInfo {
    /// Total memory in bytes
    pub total: u64,
    /// Used memory in bytes
    pub used: u64,
    /// Free memory in bytes
    pub free: u64,
}

impl MemoryInfo {
    /// Get memory usage as percentage (0-100)
    pub fn usage_percent(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            (self.used as f32 / self.total as f32) * 100.0
        }
    }

    /// Get total memory in MiB
    pub fn total_mib(&self) -> u64 {
        self.total / (1024 * 1024)
    }

    /// Get used memory in MiB
    pub fn used_mib(&self) -> u64 {
        self.used / (1024 * 1024)
    }

    /// Get free memory in MiB
    pub fn free_mib(&self) -> u64 {
        self.free / (1024 * 1024)
    }

    /// Get total memory in GiB
    pub fn total_gib(&self) -> f32 {
        self.total as f32 / (1024.0 * 1024.0 * 1024.0)
    }

    /// Get used memory in GiB
    pub fn used_gib(&self) -> f32 {
        self.used as f32 / (1024.0 * 1024.0 * 1024.0)
    }
}
