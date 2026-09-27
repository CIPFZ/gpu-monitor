//! Non-interactive reports and the shared JSON projections.
//!
//! The JSON contract is unchanged: consumers still receive the snapshot exactly
//! as the core defines it. Only the human-readable layout was reorganised, into
//! labelled groups with aligned columns instead of one long line per concern.

use gpu_monitor_core::{GpuProcess, MonitorSnapshot};
use gpu_monitor_runtime::{AlertEvent, AlertKind, AlertState};
use std::{
    fmt::Write as _,
    io::{self, Write as _},
};

use crate::{
    args::OutputFormat,
    format::{self, UNAVAILABLE},
    selection::Selection,
};

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
        "GPU sample · schema {} · {} ms since the Unix epoch\n",
        snapshot.schema_version, snapshot.sampled_at_ms
    );
    if let Some(error) = &snapshot.error {
        let _ = writeln!(
            text,
            "Monitor unavailable: {}",
            format::safe_text(&error.message)
        );
    }
    for failure in &snapshot.failures {
        let _ = writeln!(
            text,
            "GPU {} unavailable: {}",
            failure.index,
            format::safe_text(&failure.error.message)
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
        let metrics = &gpu.metrics;
        let memory = gpu.memory.as_ref();
        let _ = writeln!(
            text,
            "\nGPU {}  {}",
            gpu.device.index,
            format::safe_text(&gpu.device.name)
        );
        let _ = writeln!(
            text,
            "  {} · {} · driver {} · CUDA {}",
            format::safe_text(&gpu.device.uuid),
            format::safe_text(&gpu.device.pci_bus_id),
            format::safe_text(&gpu.device.driver_version),
            gpu.device
                .cuda_version
                .as_deref()
                .map(format::safe_text)
                .unwrap_or_else(|| UNAVAILABLE.to_owned())
        );
        if !processes_only {
            let usage = format::memory_ratio(memory)
                .map(|percent| format!(" ({percent:.0}%)"))
                .unwrap_or_default();
            let _ = writeln!(
                text,
                "  Load    {}   Memory {}{}   Temp {}   Fan {}",
                format::value(metrics.gpu_utilization, "%"),
                format::memory_capacity(memory),
                usage,
                format::value(metrics.temperature, "°C"),
                format::value(metrics.fan_speed, "%")
            );
            let _ = writeln!(
                text,
                "  Power   {}   State {}   Throttle {}",
                format::power_capacity(metrics.power_watts(), gpu.device.power_limit),
                metrics
                    .performance_state
                    .as_deref()
                    .map(format::safe_text)
                    .unwrap_or_else(|| UNAVAILABLE.to_owned()),
                format::throttle(metrics.throttle_reasons.as_ref())
            );
            let _ = writeln!(
                text,
                "  Clocks  graphics {} · SM {} · memory {}",
                format::value(metrics.clock_graphics, " MHz"),
                format::value(metrics.clock_sm, " MHz"),
                format::value(metrics.clock_memory, " MHz")
            );
            let _ = writeln!(
                text,
                "  Video   memory I/O {} · encoder {} · decoder {}",
                format::value(metrics.memory_utilization, "%"),
                format::value(metrics.encoder_utilization, "%"),
                format::value(metrics.decoder_utilization, "%")
            );
            let _ = writeln!(
                text,
                "  Link    PCIe {} · RX {} · TX {}",
                format::pcie_link(metrics.pcie_generation, metrics.pcie_width),
                format::value(metrics.pcie_rx_kb_per_second, " KB/s"),
                format::value(metrics.pcie_tx_kb_per_second, " KB/s")
            );
        }
        for issue in &gpu.issues {
            let _ = writeln!(
                text,
                "  Unavailable {}: {}",
                format::safe_text(&issue.metric),
                format::safe_text(&issue.error.message)
            );
        }
        text.push_str(&process_table(gpu));
    }
    format::safe_lines(&text)
}

fn process_table(gpu: &gpu_monitor_core::GpuInfo) -> String {
    let mut text = format!("  Processes ({})\n", gpu.processes.len());
    if gpu.processes.is_empty() {
        text.push_str(
            if gpu
                .issues
                .iter()
                .any(|issue| issue.metric.starts_with("processes"))
            {
                "    Process list unavailable or incomplete.\n"
            } else {
                "    No processes in this view.\n"
            },
        );
        return text;
    }
    let _ = writeln!(
        text,
        "    {:>8}  {:<16} {:<12} {:>12}  {:<5} NAME",
        "PID", "USER", "ELAPSED", "GPU MEMORY", "TYPE"
    );
    for process in &gpu.processes {
        let _ = writeln!(
            text,
            "    {:>8}  {:<16} {:<12} {:>12}  {:<5} {}",
            process.pid,
            format::truncate_str(&format::process_owner(process), 16),
            format::elapsed(process.elapsed_seconds),
            format::value(process.gpu_memory_mib(), " MiB"),
            process.process_type.short_label(),
            format::safe_text(&process.name)
        );
        if let Some(command) = &process.command {
            let _ = writeln!(
                text,
                "      command: {}",
                serde_json::to_string(command).unwrap_or_default()
            );
        }
    }
    text
}

pub fn diagnostics_text(snapshot: &MonitorSnapshot) -> String {
    let mut text = format!(
        "Monitor diagnostics · schema {} · sample {} ms\n\
         Backend: NVIDIA NVML on Linux. An unavailable field keeps its reason below.\n",
        snapshot.schema_version, snapshot.sampled_at_ms
    );
    for gpu in &snapshot.gpus {
        let _ = writeln!(
            text,
            "\nGPU {}  {}\n  UUID          {}\n  PCI           {}\n  Driver        {}\n  \
             CUDA          {}\n  Last sample   {} ms",
            gpu.device.index,
            format::safe_text(&gpu.device.name),
            format::safe_text(&gpu.device.uuid),
            format::safe_text(&gpu.device.pci_bus_id),
            format::safe_text(&gpu.device.driver_version),
            gpu.device
                .cuda_version
                .as_deref()
                .map(format::safe_text)
                .unwrap_or_else(|| UNAVAILABLE.to_owned()),
            gpu.sampled_at_ms
        );
        let _ = writeln!(
            text,
            "  Power limit   {} (max {})\n  State         {}\n  Throttle      {}\n  \
             PCIe          {} · RX {} · TX {}",
            format::value(gpu.device.power_limit, " W"),
            format::value(gpu.device.power_limit_max, " W"),
            gpu.metrics
                .performance_state
                .as_deref()
                .map(format::safe_text)
                .unwrap_or_else(|| UNAVAILABLE.to_owned()),
            format::throttle(gpu.metrics.throttle_reasons.as_ref()),
            format::pcie_link(gpu.metrics.pcie_generation, gpu.metrics.pcie_width),
            format::value(gpu.metrics.pcie_rx_kb_per_second, " KB/s"),
            format::value(gpu.metrics.pcie_tx_kb_per_second, " KB/s")
        );
        if gpu.issues.is_empty() {
            text.push_str("  Every requested metric was available.\n");
        }
        for issue in &gpu.issues {
            let _ = writeln!(
                text,
                "  Unavailable   {} · {:?} · {}",
                format::safe_text(&issue.metric),
                issue.error.kind,
                format::safe_text(&issue.error.message)
            );
        }
    }
    if let Some(error) = &snapshot.error {
        let _ = writeln!(
            text,
            "\nMonitor error · {:?} · {}",
            error.kind,
            format::safe_text(&error.message)
        );
    }
    for failure in &snapshot.failures {
        let _ = writeln!(
            text,
            "GPU {} error · {:?} · {}",
            failure.index,
            failure.error.kind,
            format::safe_text(&failure.error.message)
        );
    }
    format::safe_lines(&text)
}

fn alert_kind_label(kind: AlertKind) -> &'static str {
    match kind {
        AlertKind::Temperature => "temperature",
        AlertKind::Memory => "memory",
        AlertKind::DeviceUnavailable => "device-unavailable",
        AlertKind::MonitorUnavailable => "monitor-unavailable",
    }
}

fn alert_state_label(state: AlertState) -> &'static str {
    match state {
        AlertState::Firing => "firing",
        AlertState::Recovered => "recovered",
    }
}

fn alert_scope(event: &AlertEvent) -> String {
    format::safe_text(event.gpu_uuid.as_deref().unwrap_or("monitor"))
}

/// Absolute time, for logs and scripts that consume the text form.
pub fn alert_report_line(event: &AlertEvent) -> String {
    format!(
        "{} {:<9} {:<19} {:<40} {}",
        event.at_ms,
        alert_state_label(event.state),
        alert_kind_label(event.kind),
        format::truncate_str(&alert_scope(event), 40),
        format::safe_text(&event.message)
    )
}

/// Relative time, which is what a watching operator actually needs.
pub fn alert_panel_line(event: &AlertEvent, now_ms: u64) -> String {
    format!(
        "{:>10}  {:<9} {:<19} {}",
        relative_time(event.at_ms, now_ms),
        alert_state_label(event.state),
        alert_kind_label(event.kind),
        format::safe_text(&event.message)
    )
}

fn relative_time(at_ms: u64, now_ms: u64) -> String {
    let Some(elapsed) = now_ms.checked_sub(at_ms) else {
        return "just now".into();
    };
    let seconds = elapsed / 1000;
    match seconds {
        0 => "just now".into(),
        seconds if seconds < 60 => format!("{seconds}s ago"),
        seconds if seconds < 3_600 => format!("{}m ago", seconds / 60),
        seconds => format!("{}h ago", seconds / 3_600),
    }
}

/// Whether the process reading our output is still listening.
///
/// A reader such as `head` closes the pipe once it has enough. That is how it
/// says "stop", not a failure, so it ends the output normally instead of
/// raising an I/O error the user would have to interpret.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Emit {
    Wrote,
    ReaderClosed,
}

impl Emit {
    pub fn reader_closed(self) -> bool {
        self == Emit::ReaderClosed
    }
}

fn write_out(text: &str) -> anyhow::Result<Emit> {
    let mut out = io::stdout().lock();
    // Flush per record so a consumer reading a stream sees each sample as it is
    // produced, rather than when a buffer happens to fill.
    match out.write_all(text.as_bytes()).and_then(|()| out.flush()) {
        Ok(()) => Ok(Emit::Wrote),
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(Emit::ReaderClosed),
        Err(error) => Err(error.into()),
    }
}

pub fn emit_line(line: &str) -> anyhow::Result<Emit> {
    write_out(&format!("{line}\n"))
}

pub fn emit_text(text: &str) -> anyhow::Result<Emit> {
    write_out(text)
}

pub fn print_snapshot(
    snapshot: &MonitorSnapshot,
    format: OutputFormat,
    processes_only: bool,
    filtered: bool,
) -> anyhow::Result<Emit> {
    if format == OutputFormat::Json {
        let value = if processes_only {
            serde_json::to_value(process_snapshot_json(snapshot))?
        } else {
            serde_json::to_value(snapshot)?
        };
        emit_line(&serde_json::to_string_pretty(&value)?)
    } else {
        emit_text(&snapshot_text(snapshot, processes_only, filtered))
    }
}

pub fn snapshot_result(snapshot: &MonitorSnapshot) -> anyhow::Result<()> {
    if let Some(error) = &snapshot.error {
        anyhow::bail!("GPU sampling failed: {}", format::safe_text(&error.message));
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
