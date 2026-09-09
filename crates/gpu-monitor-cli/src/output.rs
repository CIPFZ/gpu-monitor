use crate::{selection::Selection, ui};
use gpu_monitor_core::{GpuProcess, MonitorSnapshot};
use std::fmt::Write;

#[derive(serde::Serialize)]
pub struct ProcessSnapshot<'a> {
    schema_version: u32,
    sampled_at_ms: u64,
    gpus: Vec<ProcessGpu<'a>>,
    failures: &'a [gpu_monitor_core::DeviceFailure],
    error: &'a Option<gpu_monitor_core::SampleError>,
}
#[derive(serde::Serialize)]
struct ProcessGpu<'a> {
    device: ProcessDevice<'a>,
    sampled_at_ms: u64,
    processes: Vec<ProcessOutput<'a>>,
    issues: &'a [gpu_monitor_core::MetricIssue],
}
#[derive(serde::Serialize)]
struct ProcessDevice<'a> {
    index: u32,
    uuid: &'a str,
    name: &'a str,
}
#[derive(serde::Serialize)]
struct ProcessOutput<'a> {
    #[serde(flatten)]
    process: &'a GpuProcess,
    gpu_memory_mib: Option<u64>,
}
pub fn process_snapshot_json(snapshot: &MonitorSnapshot) -> ProcessSnapshot<'_> {
    ProcessSnapshot {
        schema_version: snapshot.schema_version,
        sampled_at_ms: snapshot.sampled_at_ms,
        gpus: snapshot
            .gpus
            .iter()
            .map(|gpu| ProcessGpu {
                device: ProcessDevice {
                    index: gpu.device.index,
                    uuid: &gpu.device.uuid,
                    name: &gpu.device.name,
                },
                sampled_at_ms: gpu.sampled_at_ms,
                processes: gpu
                    .processes
                    .iter()
                    .map(|process| ProcessOutput {
                        process,
                        gpu_memory_mib: process.gpu_memory_mib(),
                    })
                    .collect(),
                issues: &gpu.issues,
            })
            .collect(),
        failures: &snapshot.failures,
        error: &snapshot.error,
    }
}
pub fn snapshot_text(snapshot: &MonitorSnapshot, processes_only: bool, filtered: bool) -> String {
    let mut text = format!(
        "GPU sample at {} ms since Unix epoch (schema {})\n",
        snapshot.sampled_at_ms, snapshot.schema_version
    );
    if let Some(error) = &snapshot.error {
        let _ = writeln!(text, "Monitor unavailable: {}", safe_text(&error.message));
    }
    for failure in &snapshot.failures {
        let _ = writeln!(
            text,
            "GPU {} unavailable: {}",
            failure.index,
            safe_text(&failure.error.message)
        );
    }
    if snapshot.gpus.is_empty() && snapshot.failures.is_empty() && snapshot.error.is_none() {
        text.push_str(if filtered {
            "No GPUs match the current filters.\n"
        } else {
            "No GPU devices detected.\n"
        });
    }
    for gpu in &snapshot.gpus {
        let _ = writeln!(
            text,
            "GPU {}: {} ({})",
            gpu.device.index,
            safe_text(&gpu.device.name),
            gpu.device.uuid
        );
        if !processes_only {
            let _ = writeln!(
                text,
                "  GPU load: {} | {}",
                ui::value(gpu.metrics.gpu_utilization, "%"),
                ui::memory_label(gpu)
            );
            let _ = writeln!(
                text,
                "  Temperature: {} | Power: {}/{} | Fan: {}",
                ui::value(gpu.metrics.temperature, "°C"),
                ui::value(
                    gpu.metrics.power_watts().map(|power| format!("{power:.1}")),
                    " W"
                ),
                ui::value(gpu.device.power_limit, " W"),
                ui::value(gpu.metrics.fan_speed, "%")
            );
            let _ = writeln!(
                text,
                "  Clocks: graphics {}, SM {}, memory {}",
                ui::value(gpu.metrics.clock_graphics, " MHz"),
                ui::value(gpu.metrics.clock_sm, " MHz"),
                ui::value(gpu.metrics.clock_memory, " MHz")
            );
            let _ = writeln!(
                text,
                "  Memory I/O: {} | Encoder: {} | Decoder: {}",
                ui::value(gpu.metrics.memory_utilization, "%"),
                ui::value(gpu.metrics.encoder_utilization, "%"),
                ui::value(gpu.metrics.decoder_utilization, "%")
            );
            let _ = writeln!(
                text,
                "  State: {} | Throttle: {} | PCIe: Gen {} ×{} | RX: {} | TX: {}",
                gpu.metrics.performance_state.as_deref().unwrap_or("N/A"),
                gpu.metrics
                    .throttle_reasons
                    .as_ref()
                    .map(|reasons| if reasons.is_empty() {
                        "none".into()
                    } else {
                        reasons.join(", ")
                    })
                    .unwrap_or_else(|| "N/A".into()),
                ui::value(gpu.metrics.pcie_generation, ""),
                ui::value(gpu.metrics.pcie_width, ""),
                ui::value(gpu.metrics.pcie_rx_kb_per_second, " KB/s"),
                ui::value(gpu.metrics.pcie_tx_kb_per_second, " KB/s")
            );
        }
        for issue in &gpu.issues {
            let _ = writeln!(
                text,
                "  Unavailable {}: {}",
                issue.metric,
                safe_text(&issue.error.message)
            );
        }
        let _ = writeln!(
            text,
            "  {:>8}  {:<16} {:<12} {:<30} {:>12}  Type",
            "PID", "User", "Elapsed", "Name", "GPU memory"
        );
        for process in &gpu.processes {
            let _ = writeln!(
                text,
                "  {:>8}  {:<16} {:<12} {:<30} {:>12}  {}",
                process.pid,
                truncate_str(&process_owner(process), 16),
                elapsed(process.elapsed_seconds),
                truncate_str(&safe_text(&process.name), 30),
                ui::value(process.gpu_memory_mib(), " MiB"),
                process.process_type.short_label()
            );
            if let Some(command) = &process.command {
                let _ = writeln!(
                    text,
                    "    Command: {}",
                    serde_json::to_string(command).unwrap_or_default()
                );
            }
        }
        if gpu.processes.is_empty() {
            text.push_str(
                if gpu
                    .issues
                    .iter()
                    .any(|issue| issue.metric.starts_with("processes"))
                {
                    "  Process list unavailable or incomplete.\n"
                } else {
                    "  No processes in this view.\n"
                },
            );
        }
    }
    safe_lines(&text)
}
pub fn diagnostics_text(snapshot: &MonitorSnapshot) -> String {
    let mut text = format!(
        "Monitor diagnostics · schema {} · sample {} ms\n",
        snapshot.schema_version, snapshot.sampled_at_ms
    );
    text.push_str("Backend: NVIDIA NVML on Linux; unavailable fields retain their reason below.\n");
    for gpu in &snapshot.gpus {
        let _ = writeln!(
            text,
            "GPU {}: {}\n  UUID: {}\n  PCI: {}\n  Driver: {}\n  CUDA: {}\n  Last sample: {} ms",
            gpu.device.index,
            safe_text(&gpu.device.name),
            gpu.device.uuid,
            gpu.device.pci_bus_id,
            gpu.device.driver_version,
            gpu.device.cuda_version.as_deref().unwrap_or("N/A"),
            gpu.sampled_at_ms
        );
        let _ = writeln!(
            text,
            "  Performance state: {}\n  Throttle reasons: {}\n  PCIe: Gen {} ×{}; RX {}; TX {}",
            gpu.metrics.performance_state.as_deref().unwrap_or("N/A"),
            gpu.metrics
                .throttle_reasons
                .as_ref()
                .map(|reasons| if reasons.is_empty() {
                    "none".into()
                } else {
                    reasons.join(", ")
                })
                .unwrap_or_else(|| "N/A".into()),
            ui::value(gpu.metrics.pcie_generation, ""),
            ui::value(gpu.metrics.pcie_width, ""),
            ui::value(gpu.metrics.pcie_rx_kb_per_second, " KB/s"),
            ui::value(gpu.metrics.pcie_tx_kb_per_second, " KB/s")
        );
        for issue in &gpu.issues {
            let _ = writeln!(
                text,
                "  {}: {:?}: {}",
                issue.metric,
                issue.error.kind,
                safe_text(&issue.error.message)
            );
        }
    }
    if let Some(error) = &snapshot.error {
        let _ = writeln!(
            text,
            "Global error: {:?}: {}",
            error.kind,
            safe_text(&error.message)
        );
    }
    for failure in &snapshot.failures {
        let _ = writeln!(
            text,
            "GPU {} error: {:?}: {}",
            failure.index,
            failure.error.kind,
            safe_text(&failure.error.message)
        );
    }
    safe_lines(&text)
}
pub fn print_snapshot(
    snapshot: &MonitorSnapshot,
    json: bool,
    processes_only: bool,
    filtered: bool,
) -> anyhow::Result<()> {
    if json {
        let value = if processes_only {
            serde_json::to_value(process_snapshot_json(snapshot))?
        } else {
            serde_json::to_value(snapshot)?
        };
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        print!("{}", snapshot_text(snapshot, processes_only, filtered));
    }
    Ok(())
}
pub fn snapshot_result(snapshot: &MonitorSnapshot) -> anyhow::Result<()> {
    if let Some(error) = &snapshot.error {
        anyhow::bail!("GPU sampling failed: {}", safe_text(&error.message));
    }
    if snapshot.gpus.is_empty() && !snapshot.failures.is_empty() {
        anyhow::bail!("All selected GPU devices failed to report data");
    }
    Ok(())
}
pub fn selection_result(
    original: &MonitorSnapshot,
    selected: &MonitorSnapshot,
    filter: &Selection,
) -> anyhow::Result<()> {
    if original.error.is_some() {
        return snapshot_result(original);
    }
    snapshot_result(selected)?;
    if filter.has_device_filter() && selected.gpus.is_empty() {
        anyhow::bail!("No GPUs match the requested filters");
    }
    Ok(())
}
pub fn safe_lines(text: &str) -> String {
    text.chars()
        .map(|character| {
            if character.is_control() && character != '\n' {
                ' '
            } else {
                character
            }
        })
        .collect()
}
pub fn safe_text(text: &str) -> String {
    text.chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect()
}
pub fn process_owner(process: &GpuProcess) -> String {
    process
        .user
        .as_ref()
        .map(|name| safe_text(name))
        .or_else(|| process.uid.map(|uid| uid.to_string()))
        .unwrap_or_else(|| "N/A".into())
}
pub fn elapsed(seconds: Option<u64>) -> String {
    seconds.map_or_else(
        || "N/A".into(),
        |seconds| {
            let days = seconds / 86400;
            let hours = seconds / 3600 % 24;
            let minutes = seconds / 60 % 60;
            let seconds = seconds % 60;
            if days > 0 {
                format!("{days}d {hours:02}:{minutes:02}:{seconds:02}")
            } else {
                format!("{hours:02}:{minutes:02}:{seconds:02}")
            }
        },
    )
}
pub fn truncate_str(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.into()
    } else {
        format!(
            "{}…",
            text.chars()
                .take(max_chars.saturating_sub(1))
                .collect::<String>()
        )
    }
}
