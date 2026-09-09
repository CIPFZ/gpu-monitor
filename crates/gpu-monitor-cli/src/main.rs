//! Terminal-based GPU monitoring with machine-readable failure information.

mod app;
#[cfg(test)]
mod tests;
mod tui;
mod ui;

use clap::{Parser, Subcommand};
use gpu_monitor_core::{MonitorService, MonitorSnapshot};

#[derive(Parser, Debug)]
#[command(
    name = "gpu-monitor",
    author,
    version,
    about = "Real-time NVIDIA GPU monitoring"
)]
struct Cli {
    /// Print GPU info once and exit
    #[arg(short, long)]
    once: bool,
    /// Continuous output (TUI, or JSON Lines with --json)
    #[arg(short, long)]
    watch: bool,
    /// Output a snapshot as JSON, including device and metric errors
    #[arg(short, long)]
    json: bool,
    /// Refresh interval in milliseconds (minimum 100)
    #[arg(short, long, default_value = "1000", value_parser = clap::value_parser!(u64).range(app::MIN_INTERVAL_MS..))]
    interval: u64,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Show GPU processes only (retains GPU identity and errors in JSON)
    Processes,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::WARN.into()),
        )
        .init();
    let mut monitor = MonitorService::new();
    if matches!(cli.command, Some(Commands::Processes)) {
        let snapshot = monitor.sample();
        print_snapshot(&snapshot, cli.json, true)?;
        return snapshot_result(&snapshot);
    }
    if cli.once || (cli.json && !cli.watch) {
        let snapshot = monitor.sample();
        print_snapshot(&snapshot, cli.json, false)?;
        snapshot_result(&snapshot)
    } else if cli.json {
        run_json_watch(&mut monitor, cli.interval)
    } else {
        let mut session = tui::init()?;
        app::App::new(cli.interval).run(&mut session.terminal, &mut monitor)
    }
}

#[derive(serde::Serialize)]
struct ProcessSnapshot<'a> {
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
    process: &'a gpu_monitor_core::GpuProcess,
    gpu_memory_mib: Option<u64>,
}

/// Machine-readable process output uses the same snapshot envelope and preserves errors.
fn process_snapshot_json(snapshot: &MonitorSnapshot) -> ProcessSnapshot<'_> {
    ProcessSnapshot {
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

fn print_snapshot(
    snapshot: &MonitorSnapshot,
    json: bool,
    processes_only: bool,
) -> anyhow::Result<()> {
    if json {
        let output = if processes_only {
            serde_json::to_value(process_snapshot_json(snapshot))?
        } else {
            serde_json::to_value(snapshot)?
        };
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }
    println!(
        "GPU sample at {} ms since Unix epoch",
        snapshot.sampled_at_ms
    );
    if let Some(error) = &snapshot.error {
        println!("Monitor unavailable: {}", error.message);
    }
    for failure in &snapshot.failures {
        println!(
            "GPU {} unavailable: {}",
            failure.index, failure.error.message
        );
    }
    if snapshot.gpus.is_empty() && snapshot.failures.is_empty() && snapshot.error.is_none() {
        println!("No GPU devices detected.");
    }
    for gpu in &snapshot.gpus {
        println!(
            "GPU {}: {} ({})",
            gpu.device.index, gpu.device.name, gpu.device.uuid
        );
        if !processes_only {
            println!(
                "  GPU load: {} | {}",
                ui::value(gpu.metrics.gpu_utilization, "%"),
                ui::memory_label(gpu)
            );
            println!(
                "  Temperature: {} | Power: {}/{} | Fan: {}",
                ui::value(gpu.metrics.temperature, "°C"),
                ui::value(
                    gpu.metrics.power_watts().map(|power| format!("{power:.1}")),
                    " W"
                ),
                ui::value(gpu.device.power_limit, " W"),
                ui::value(gpu.metrics.fan_speed, "%")
            );
            println!(
                "  Clocks: graphics {}, SM {}, memory {}",
                ui::value(gpu.metrics.clock_graphics, " MHz"),
                ui::value(gpu.metrics.clock_sm, " MHz"),
                ui::value(gpu.metrics.clock_memory, " MHz")
            );
            println!(
                "  Memory I/O: {} | Encoder: {} | Decoder: {}",
                ui::value(gpu.metrics.memory_utilization, "%"),
                ui::value(gpu.metrics.encoder_utilization, "%"),
                ui::value(gpu.metrics.decoder_utilization, "%")
            );
        }
        for issue in &gpu.issues {
            println!("  Unavailable {}: {}", issue.metric, issue.error.message);
        }
        println!("  {:>8}  {:<30} {:>12}  Type", "PID", "Name", "GPU memory");
        for process in &gpu.processes {
            println!(
                "  {:>8}  {:<30} {:>12}  {}",
                process.pid,
                truncate_str(&process.name, 30),
                ui::value(process.gpu_memory_mib(), " MiB"),
                process.process_type.short_label()
            );
        }
    }
    Ok(())
}

fn snapshot_result(snapshot: &MonitorSnapshot) -> anyhow::Result<()> {
    if let Some(error) = &snapshot.error {
        anyhow::bail!("GPU sampling failed: {}", error.message);
    }
    if snapshot.gpus.is_empty() && !snapshot.failures.is_empty() {
        anyhow::bail!("All GPU devices failed to report data");
    }
    Ok(())
}

fn run_json_watch(monitor: &mut MonitorService, interval: u64) -> anyhow::Result<()> {
    loop {
        println!("{}", serde_json::to_string(&monitor.sample())?);
        std::thread::sleep(std::time::Duration::from_millis(interval));
    }
}

fn truncate_str(text: &str, max_chars: usize) -> String {
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
