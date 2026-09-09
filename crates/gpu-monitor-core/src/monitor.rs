//! NVML adapter. Driver-specific errors stop at this boundary.

use crate::{
    service::Backend, DeviceFailure, DeviceInfo, ErrorKind, GpuInfo, GpuMetrics, GpuProcess,
    MemoryInfo, MetricIssue, ProcessType, SampleError,
};
use nvml_wrapper::{
    enum_wrappers::device::{Clock, TemperatureSensor},
    enums::device::UsedGpuMemory,
    error::NvmlError,
    struct_wrappers::device::ProcessInfo,
    Nvml,
};
use std::{collections::HashMap, fs};

pub(crate) struct NvmlBackend {
    nvml: Nvml,
    // UUID is stable when indices change. Only successful static queries are cached.
    identities: HashMap<String, DeviceInfo>,
}

impl NvmlBackend {
    pub(crate) fn new() -> Result<Self, SampleError> {
        Ok(Self {
            nvml: initialize_nvml().map_err(SampleError::from)?,
            identities: HashMap::new(),
        })
    }
}

// Runtime driver packages and containers may ship only the versioned SONAME.
// A genuine driver initialization failure must not be obscured by a fallback.
#[cfg(target_os = "linux")]
fn initialize_nvml() -> Result<Nvml, NvmlError> {
    match Nvml::builder()
        .lib_path(std::ffi::OsStr::new("libnvidia-ml.so.1"))
        .init()
    {
        Err(NvmlError::LibloadingError(_)) | Err(NvmlError::LibraryNotFound) => Nvml::init(),
        result => result,
    }
}

#[cfg(not(target_os = "linux"))]
fn initialize_nvml() -> Result<Nvml, NvmlError> {
    Nvml::init()
}

impl Backend for NvmlBackend {
    fn device_count(&self) -> Result<u32, SampleError> {
        self.nvml.device_count().map_err(Into::into)
    }

    fn sample_device(&mut self, index: u32, sampled_at_ms: u64) -> Result<GpuInfo, DeviceFailure> {
        let device = self
            .nvml
            .device_by_index(index)
            .map_err(|error| DeviceFailure {
                index,
                uuid: None,
                error: error.into(),
            })?;
        let uuid = device.uuid().map_err(|error| DeviceFailure {
            index,
            uuid: None,
            error: error.into(),
        })?;
        let identity_error = |error: NvmlError| DeviceFailure {
            index,
            uuid: Some(uuid.clone()),
            error: error.into(),
        };
        if !self.identities.contains_key(&uuid) {
            self.identities.insert(
                uuid.clone(),
                DeviceInfo {
                    index,
                    name: device.name().map_err(identity_error)?,
                    uuid: uuid.clone(),
                    pci_bus_id: device.pci_info().map_err(identity_error)?.bus_id,
                    driver_version: self.nvml.sys_driver_version().map_err(identity_error)?,
                    cuda_version: None,
                    power_limit: None,
                    power_limit_max: None,
                },
            );
        }
        let identity = self.identities.get_mut(&uuid).expect("inserted above");
        identity.index = index;
        let mut issues = Vec::new();
        if identity.cuda_version.is_none() {
            identity.cuda_version = read_metric(
                "cuda_version",
                self.nvml.sys_cuda_driver_version(),
                &mut issues,
            )
            .map(|version| format!("{}.{}", version / 1000, (version % 1000) / 10));
        }
        let mut device_info = identity.clone();
        // Limits can change while the monitor is running, so sample them afresh.
        device_info.power_limit =
            read_metric("power_limit", device.power_management_limit(), &mut issues)
                .map(|limit| limit / 1000);
        device_info.power_limit_max = read_metric(
            "power_limit_max",
            device.power_management_limit_constraints(),
            &mut issues,
        )
        .map(|limits| limits.max_limit / 1000);
        let memory =
            read_metric("memory", device.memory_info(), &mut issues).map(|memory| MemoryInfo {
                total: memory.total,
                used: memory.used,
                free: memory.free,
            });
        let (gpu_utilization, memory_utilization) = match device.utilization_rates() {
            Ok(utilization) => (Some(utilization.gpu), Some(utilization.memory)),
            Err(error) => {
                let error = SampleError::from(error);
                for metric in ["gpu_utilization", "memory_utilization"] {
                    issues.push(MetricIssue {
                        metric: metric.into(),
                        error: error.clone(),
                    });
                }
                (None, None)
            }
        };
        let metrics = GpuMetrics {
            gpu_utilization,
            memory_utilization,
            encoder_utilization: read_metric(
                "encoder_utilization",
                device.encoder_utilization(),
                &mut issues,
            )
            .map(|value| value.utilization),
            decoder_utilization: read_metric(
                "decoder_utilization",
                device.decoder_utilization(),
                &mut issues,
            )
            .map(|value| value.utilization),
            temperature: read_metric(
                "temperature",
                device.temperature(TemperatureSensor::Gpu),
                &mut issues,
            ),
            power_usage: read_metric("power_usage", device.power_usage(), &mut issues),
            fan_speed: read_metric("fan_speed", device.fan_speed(0), &mut issues),
            clock_graphics: read_metric(
                "clock_graphics",
                device.clock_info(Clock::Graphics),
                &mut issues,
            ),
            clock_memory: read_metric(
                "clock_memory",
                device.clock_info(Clock::Memory),
                &mut issues,
            ),
            clock_sm: read_metric("clock_sm", device.clock_info(Clock::SM), &mut issues),
        };
        let mut processes = Vec::new();
        append_processes(
            device.running_compute_processes(),
            ProcessType::Compute,
            &mut processes,
            &mut issues,
        );
        append_processes(
            device.running_graphics_processes(),
            ProcessType::Graphics,
            &mut processes,
            &mut issues,
        );
        processes.sort_by(|a, b| {
            b.gpu_memory
                .cmp(&a.gpu_memory)
                .then_with(|| a.pid.cmp(&b.pid))
        });
        Ok(GpuInfo {
            device: device_info,
            metrics,
            memory,
            processes,
            sampled_at_ms,
            issues,
        })
    }
}

fn read_metric<T>(
    metric: &str,
    result: Result<T, NvmlError>,
    issues: &mut Vec<MetricIssue>,
) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(error) => {
            issues.push(MetricIssue {
                metric: metric.into(),
                error: error.into(),
            });
            None
        }
    }
}

fn append_processes(
    result: Result<Vec<ProcessInfo>, NvmlError>,
    process_type: ProcessType,
    processes: &mut Vec<GpuProcess>,
    issues: &mut Vec<MetricIssue>,
) {
    let metric = match process_type {
        ProcessType::Compute => "processes_compute",
        _ => "processes_graphics",
    };
    let Some(found) = read_metric(metric, result, issues) else {
        return;
    };
    for process in found {
        let memory = match process.used_gpu_memory {
            UsedGpuMemory::Used(bytes) => Some(bytes),
            UsedGpuMemory::Unavailable => {
                issues.push(MetricIssue {
                    metric: format!("processes.{}.gpu_memory", process.pid),
                    error: SampleError {
                        kind: ErrorKind::NotSupported,
                        message: "Driver does not report this process's GPU memory usage".into(),
                    },
                });
                None
            }
        };
        if let Some(existing) = processes
            .iter_mut()
            .find(|existing| existing.pid == process.pid)
        {
            if existing.process_type != process_type {
                existing.process_type = ProcessType::Mixed;
            }
            // Both APIs report the same process allocation; do not double-count.
            existing.gpu_memory = existing.gpu_memory.max(memory);
        } else {
            processes.push(GpuProcess {
                pid: process.pid,
                name: process_name(process.pid).unwrap_or_else(|| "unknown".into()),
                gpu_memory: memory,
                process_type,
            });
        }
    }
}

fn process_name(pid: u32) -> Option<String> {
    fs::read_to_string(format!("/proc/{pid}/comm"))
        .ok()
        .map(|value| value.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process(pid: u32, memory: UsedGpuMemory) -> ProcessInfo {
        ProcessInfo {
            pid,
            used_gpu_memory: memory,
            gpu_instance_id: None,
            compute_instance_id: None,
        }
    }

    #[test]
    fn unavailable_metrics_are_distinct_from_successful_zero() {
        let mut issues = Vec::new();
        assert_eq!(
            read_metric(
                "temperature",
                Err::<u32, _>(NvmlError::NotSupported),
                &mut issues
            ),
            None
        );
        assert_eq!(read_metric("power_usage", Ok(0), &mut issues), Some(0));
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].error.kind, ErrorKind::NotSupported);
        assert_eq!(
            serde_json::to_value(&issues).unwrap()[0]["error"]["kind"],
            "not_supported"
        );
        let metrics = GpuMetrics::default();
        let serialized = serde_json::to_value(&metrics).unwrap();
        assert!(serialized["temperature"].is_null());
        assert_eq!(metrics.temperature_status(), None);
        assert_eq!(metrics.power_watts(), None);
        assert_eq!(metrics.is_idle(), None);
    }

    #[test]
    fn process_query_failure_preserves_other_results_and_error() {
        let mut processes = Vec::new();
        let mut issues = Vec::new();
        append_processes(
            Err(NvmlError::NoPermission),
            ProcessType::Compute,
            &mut processes,
            &mut issues,
        );
        append_processes(
            Ok(vec![process(999999, UsedGpuMemory::Used(1024))]),
            ProcessType::Graphics,
            &mut processes,
            &mut issues,
        );
        assert_eq!(processes.len(), 1);
        assert_eq!(processes[0].process_type, ProcessType::Graphics);
        assert_eq!(issues[0].metric, "processes_compute");
        assert_eq!(issues[0].error.kind, ErrorKind::PermissionDenied);
    }

    #[test]
    fn unavailable_process_memory_remains_unknown_and_mixed_processes_are_deduplicated() {
        let mut processes = Vec::new();
        let mut issues = Vec::new();
        append_processes(
            Ok(vec![process(999999, UsedGpuMemory::Unavailable)]),
            ProcessType::Compute,
            &mut processes,
            &mut issues,
        );
        assert_eq!(processes[0].gpu_memory, None);
        assert_eq!(processes[0].gpu_memory_mib(), None);
        assert_eq!(issues[0].metric, "processes.999999.gpu_memory");
        append_processes(
            Ok(vec![process(999999, UsedGpuMemory::Used(2 * 1024 * 1024))]),
            ProcessType::Graphics,
            &mut processes,
            &mut issues,
        );
        assert_eq!(processes.len(), 1);
        assert_eq!(processes[0].process_type, ProcessType::Mixed);
        assert_eq!(processes[0].gpu_memory_mib(), Some(2));
    }

    #[test]
    fn memory_and_temperature_helpers_preserve_units() {
        let memory = MemoryInfo {
            total: 8 * 1024 * 1024 * 1024,
            used: 2 * 1024 * 1024 * 1024,
            free: 6 * 1024 * 1024 * 1024,
        };
        assert_eq!(memory.total_mib(), 8192);
        assert_eq!(memory.used_mib(), 2048);
        assert_eq!(memory.free_mib(), 6144);
        assert_eq!(memory.usage_percent(), 25.0);
        let metrics = GpuMetrics {
            temperature: Some(90),
            power_usage: Some(125_500),
            ..Default::default()
        };
        assert_eq!(
            metrics.temperature_status(),
            Some(crate::metrics::TemperatureStatus::Hot)
        );
        assert_eq!(metrics.power_watts(), Some(125.5));
    }
}
