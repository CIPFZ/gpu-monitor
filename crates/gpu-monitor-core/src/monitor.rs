//! GPU Monitor - main monitoring service

use nvml_wrapper::enum_wrappers::device::TemperatureSensor;
use nvml_wrapper::Nvml;
use std::fs;
use std::path::Path;

use crate::device::{DeviceInfo, MemoryInfo};
use crate::error::{Error, Result};
use crate::metrics::GpuMetrics;
use crate::process::{GpuProcess, ProcessType};
use crate::GpuInfo;

/// GPU Monitor service
///
/// Provides methods to query GPU information through NVML.
pub struct GpuMonitor {
    nvml: Nvml,
}

impl GpuMonitor {
    /// Create a new GPU monitor instance
    ///
    /// Initializes the NVML library. Returns an error if NVML
    /// is not available (e.g., no NVIDIA drivers installed).
    pub fn new() -> Result<Self> {
        let nvml = Nvml::init().map_err(|e| Error::NvmlInit(e.to_string()))?;
        Ok(Self { nvml })
    }

    /// Get the number of GPU devices
    pub fn device_count(&self) -> Result<u32> {
        Ok(self.nvml.device_count()?)
    }

    /// Get information for all GPU devices
    pub fn get_all_gpu_info(&self) -> Result<Vec<GpuInfo>> {
        let count = self.device_count()?;
        if count == 0 {
            return Err(Error::NoDevices);
        }

        let mut gpus = Vec::with_capacity(count as usize);
        for i in 0..count {
            gpus.push(self.get_gpu_info(i)?);
        }
        Ok(gpus)
    }

    /// Get information for a specific GPU device
    pub fn get_gpu_info(&self, index: u32) -> Result<GpuInfo> {
        let device = self.nvml.device_by_index(index)?;

        // Get device info
        let name = device.name()?;
        let uuid = device.uuid()?;
        let pci_info = device.pci_info()?;
        let pci_bus_id = pci_info.bus_id;

        // Get driver version from NVML
        let driver_version = self.nvml.sys_driver_version()?;

        // Get CUDA version (returns version as integer like 12020 for 12.2)
        let cuda_version = self
            .nvml
            .sys_cuda_driver_version()
            .ok()
            .map(|v| {
                let major = v / 1000;
                let minor = (v % 1000) / 10;
                format!("{}.{}", major, minor)
            });

        // Get power info
        let power_limit = device.power_management_limit().unwrap_or(0) / 1000; // mW to W
        let power_limit_max = device.power_management_limit_constraints()
            .map(|c| c.max_limit / 1000)
            .unwrap_or(power_limit);

        let device_info = DeviceInfo {
            index,
            name,
            uuid,
            pci_bus_id,
            driver_version,
            cuda_version,
            power_limit,
            power_limit_max,
        };

        // Get memory info
        let mem_info = device.memory_info()?;
        let memory = MemoryInfo {
            total: mem_info.total,
            used: mem_info.used,
            free: mem_info.free,
        };

        // Get utilization
        let utilization = device.utilization_rates()?;
        let gpu_utilization = utilization.gpu;
        let memory_utilization = utilization.memory;

        // Get encoder/decoder utilization
        let encoder_info = device.encoder_utilization().ok();
        let encoder_utilization = encoder_info.map(|e| e.utilization).unwrap_or(0);
        
        let decoder_info = device.decoder_utilization().ok();
        let decoder_utilization = decoder_info.map(|d| d.utilization).unwrap_or(0);

        // Get temperature
        let temperature = device
            .temperature(TemperatureSensor::Gpu)
            .unwrap_or(0);

        // Get power usage
        let power_usage = device.power_usage().unwrap_or(0);

        // Get fan speed (may not be available on all GPUs)
        let fan_speed = device.fan_speed(0).ok();

        // Get clock speeds
        let clock_graphics = device
            .clock_info(nvml_wrapper::enum_wrappers::device::Clock::Graphics)
            .unwrap_or(0);
        let clock_memory = device
            .clock_info(nvml_wrapper::enum_wrappers::device::Clock::Memory)
            .unwrap_or(0);
        let clock_sm = device
            .clock_info(nvml_wrapper::enum_wrappers::device::Clock::SM)
            .unwrap_or(0);

        let metrics = GpuMetrics {
            gpu_utilization,
            memory_utilization,
            encoder_utilization,
            decoder_utilization,
            temperature,
            power_usage,
            fan_speed,
            clock_graphics,
            clock_memory,
            clock_sm,
        };

        // Get processes
        let processes = self.get_gpu_processes(&device)?;

        Ok(GpuInfo {
            device: device_info,
            metrics,
            memory,
            processes,
        })
    }

    /// Get processes using a GPU device
    fn get_gpu_processes(
        &self,
        device: &nvml_wrapper::Device,
    ) -> Result<Vec<GpuProcess>> {
        let mut processes = Vec::new();

        // Get compute processes
        if let Ok(compute_procs) = device.running_compute_processes() {
            for proc in compute_procs {
                let name = get_process_name(proc.pid).unwrap_or_else(|| "unknown".to_string());
                let memory = extract_gpu_memory(proc.used_gpu_memory);
                processes.push(GpuProcess {
                    pid: proc.pid,
                    name,
                    gpu_memory: memory,
                    process_type: ProcessType::Compute,
                });
            }
        }

        // Get graphics processes
        if let Ok(graphics_procs) = device.running_graphics_processes() {
            for proc in graphics_procs {
                let memory = extract_gpu_memory(proc.used_gpu_memory);
                // Check if we already have this process as compute
                if let Some(existing) = processes.iter_mut().find(|p| p.pid == proc.pid) {
                    existing.process_type = ProcessType::Mixed;
                    existing.gpu_memory = existing.gpu_memory.max(memory);
                } else {
                    let name =
                        get_process_name(proc.pid).unwrap_or_else(|| "unknown".to_string());
                    processes.push(GpuProcess {
                        pid: proc.pid,
                        name,
                        gpu_memory: memory,
                        process_type: ProcessType::Graphics,
                    });
                }
            }
        }

        // Sort by memory usage (descending)
        processes.sort_by(|a, b| b.gpu_memory.cmp(&a.gpu_memory));

        Ok(processes)
    }
}

/// Extract GPU memory value from UsedGpuMemory enum
fn extract_gpu_memory(used: nvml_wrapper::enums::device::UsedGpuMemory) -> u64 {
    use nvml_wrapper::enums::device::UsedGpuMemory;
    match used {
        UsedGpuMemory::Used(bytes) => bytes,
        UsedGpuMemory::Unavailable => 0,
    }
}

/// Get process name from PID by reading /proc/{pid}/comm
fn get_process_name(pid: u32) -> Option<String> {
    let comm_path = Path::new("/proc").join(pid.to_string()).join("comm");
    fs::read_to_string(comm_path)
        .ok()
        .map(|s| s.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_info_calculations() {
        let mem = MemoryInfo {
            total: 8 * 1024 * 1024 * 1024, // 8 GB
            used: 2 * 1024 * 1024 * 1024,  // 2 GB
            free: 6 * 1024 * 1024 * 1024,  // 6 GB
        };

        assert_eq!(mem.total_mib(), 8192);
        assert_eq!(mem.used_mib(), 2048);
        assert_eq!(mem.free_mib(), 6144);
        assert!((mem.usage_percent() - 25.0).abs() < 0.01);
    }

    #[test]
    fn test_temperature_status() {
        let cool = GpuMetrics {
            gpu_utilization: 0,
            memory_utilization: 0,
            encoder_utilization: 0,
            decoder_utilization: 0,
            temperature: 40,
            power_usage: 0,
            fan_speed: None,
            clock_graphics: 0,
            clock_memory: 0,
            clock_sm: 0,
        };
        assert_eq!(cool.temperature_status(), crate::metrics::TemperatureStatus::Cool);

        let hot = GpuMetrics {
            temperature: 90,
            ..cool.clone()
        };
        assert_eq!(hot.temperature_status(), crate::metrics::TemperatureStatus::Hot);
    }
}
