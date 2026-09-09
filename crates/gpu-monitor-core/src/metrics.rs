//! GPU real-time metrics

use serde::{Deserialize, Serialize};

/// Real-time GPU metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
pub struct GpuMetrics {
    /// GPU utilization percentage (0-100)
    pub gpu_utilization: Option<u32>,
    /// Memory controller busy percentage (0-100), not capacity usage
    pub memory_utilization: Option<u32>,
    /// Encoder utilization percentage (0-100)
    pub encoder_utilization: Option<u32>,
    /// Decoder utilization percentage (0-100)
    pub decoder_utilization: Option<u32>,
    /// Current temperature in Celsius
    pub temperature: Option<u32>,
    /// Current power usage in milliwatts
    pub power_usage: Option<u32>,
    /// Fan speed percentage (0-100), None if not available
    pub fan_speed: Option<u32>,
    /// Current graphics clock in MHz
    pub clock_graphics: Option<u32>,
    /// Current memory clock in MHz
    pub clock_memory: Option<u32>,
    /// Current SM clock in MHz
    pub clock_sm: Option<u32>,
    /// NVML performance state, P0 (highest performance) through P15.
    #[serde(default)]
    pub performance_state: Option<String>,
    /// Active NVML clock-limiting reasons. Empty means no reason is active.
    #[serde(default)]
    pub throttle_reasons: Option<Vec<String>>,
    /// Currently negotiated PCIe generation and lane count.
    #[serde(default)]
    pub pcie_generation: Option<u32>,
    #[serde(default)]
    pub pcie_width: Option<u32>,
    /// PCIe receive throughput in NVML's documented KB/s, over a 20 ms window.
    #[serde(default)]
    pub pcie_rx_kb_per_second: Option<u32>,
    /// PCIe transmit throughput in NVML's documented KB/s, over a 20 ms window.
    #[serde(default)]
    pub pcie_tx_kb_per_second: Option<u32>,
}

impl GpuMetrics {
    /// Get power usage in watts
    pub fn power_watts(&self) -> Option<f32> {
        self.power_usage.map(|value| value as f32 / 1000.0)
    }

    /// Check if GPU is idle (less than 5% utilization)
    pub fn is_idle(&self) -> Option<bool> {
        self.gpu_utilization.map(|value| value < 5)
    }

    /// Check if GPU is under heavy load (more than 80% utilization)
    pub fn is_heavy_load(&self) -> Option<bool> {
        self.gpu_utilization.map(|value| value > 80)
    }

    /// Get temperature status
    pub fn temperature_status(&self) -> Option<TemperatureStatus> {
        self.temperature.map(|value| match value {
            0..=50 => TemperatureStatus::Cool,
            51..=70 => TemperatureStatus::Normal,
            71..=85 => TemperatureStatus::Warm,
            _ => TemperatureStatus::Hot,
        })
    }
}

/// Temperature status categories
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
pub enum TemperatureStatus {
    /// Below 50°C
    Cool,
    /// 51-70°C
    Normal,
    /// 71-85°C
    Warm,
    /// Above 85°C
    Hot,
}

impl TemperatureStatus {
    /// Get color hint for UI (CSS color name)
    pub fn color(&self) -> &'static str {
        match self {
            Self::Cool => "green",
            Self::Normal => "blue",
            Self::Warm => "orange",
            Self::Hot => "red",
        }
    }
}
