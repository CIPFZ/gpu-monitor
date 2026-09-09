//! GPU real-time metrics

use serde::{Deserialize, Serialize};

/// Real-time GPU metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
