//! GPU Monitor CLI
//!
//! Terminal-based GPU monitoring tool with multiple output modes.

mod app;
mod tui;
mod ui;

use clap::{Parser, Subcommand};
use gpu_monitor_core::GpuMonitor;

/// GPU Monitor - Real-time NVIDIA GPU monitoring
#[derive(Parser)]
#[command(name = "gpu-monitor")]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Print GPU info once and exit (similar to nvidia-smi)
    #[arg(short, long)]
    once: bool,

    /// Continuous output mode (TUI with charts)
    #[arg(short, long)]
    watch: bool,

    /// Output as JSON
    #[arg(short, long)]
    json: bool,

    /// Refresh interval in milliseconds (default: 1000)
    #[arg(short, long, default_value = "1000")]
    interval: u64,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Show GPU processes only
    Processes,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize tracing for debug logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::WARN.into()),
        )
        .init();

    // Initialize monitor
    let monitor = match GpuMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error: Failed to initialize GPU monitor");
            eprintln!("Make sure NVIDIA drivers are installed and you have an NVIDIA GPU.");
            eprintln!("Details: {}", e);
            std::process::exit(1);
        }
    };

    // Handle subcommands
    if let Some(cmd) = &cli.command {
        match cmd {
            Commands::Processes => {
                return print_processes(&monitor, cli.json);
            }
        }
    }

    // Handle output modes
    if cli.once {
        print_gpu_info(&monitor, cli.json)?;
    } else if cli.json {
        // Continuous JSON stream if watch is set, otherwise once
        if cli.watch {
            run_json_watch(&monitor, cli.interval)?;
        } else {
            print_gpu_info(&monitor, true)?;
        }
    } else {
        // Default or --watch: launch TUI
        run_tui(&monitor, cli.interval)?;
    }

    Ok(())
}

/// Print GPU info once
fn print_gpu_info(monitor: &GpuMonitor, json: bool) -> anyhow::Result<()> {
    let gpus = monitor.get_all_gpu_info()?;

    if json {
        println!("{}", serde_json::to_string_pretty(&gpus)?);
    } else {
        for gpu in &gpus {
            println!("╭─────────────────────────────────────────────────────────────╮");
            println!("│ GPU {}: {:<48} │", gpu.device.index, gpu.device.name);
            println!("├─────────────────────────────────────────────────────────────┤");
            println!(
                "│ GPU Usage:    {:>3}%    Memory: {:>5.1}/{:.1} GiB ({:>3.0}%)        │",
                gpu.metrics.gpu_utilization,
                gpu.memory.used_gib(),
                gpu.memory.total_gib(),
                gpu.memory.usage_percent()
            );
            println!(
                "│ Temperature:  {:>3}°C   Power:  {:>5.1}/{} W                    │",
                gpu.metrics.temperature,
                gpu.metrics.power_watts(),
                gpu.device.power_limit
            );
            if let Some(fan) = gpu.metrics.fan_speed {
                println!("│ Fan Speed:    {:>3}%                                          │", fan);
            }
            println!(
                "│ Clocks:       Graphics {:>4} MHz  Memory {:>4} MHz          │",
                gpu.metrics.clock_graphics, gpu.metrics.clock_memory
            );

            if !gpu.processes.is_empty() {
                println!("├─────────────────────────────────────────────────────────────┤");
                println!("│ Processes:                                                  │");
                for proc in &gpu.processes {
                    println!(
                        "│   {:>6}  {:<30} {:>6} MiB  {:>5} │",
                        proc.pid,
                        truncate_str(&proc.name, 30),
                        proc.gpu_memory_mib(),
                        proc.process_type.short_label()
                    );
                }
            }
            println!("╰─────────────────────────────────────────────────────────────╯");
        }
    }

    Ok(())
}

/// Print GPU processes only
fn print_processes(monitor: &GpuMonitor, json: bool) -> anyhow::Result<()> {
    let gpus = monitor.get_all_gpu_info()?;

    if json {
        let all_processes: Vec<_> = gpus
            .iter()
            .flat_map(|g| {
                g.processes.iter().map(|p| {
                    serde_json::json!({
                        "gpu_index": g.device.index,
                        "pid": p.pid,
                        "name": p.name,
                        "gpu_memory_mib": p.gpu_memory_mib(),
                        "type": p.process_type
                    })
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&all_processes)?);
    } else {
        println!("╭─────────────────────────────────────────────────────────────╮");
        println!("│ GPU Processes                                               │");
        println!("├───────┬────────┬────────────────────────────┬────────┬──────┤");
        println!("│  GPU  │   PID  │ Name                       │ Memory │ Type │");
        println!("├───────┼────────┼────────────────────────────┼────────┼──────┤");

        for gpu in &gpus {
            for proc in &gpu.processes {
                println!(
                    "│  {:>3}  │ {:>6} │ {:<26} │ {:>4} MB│ {:>4} │",
                    gpu.device.index,
                    proc.pid,
                    truncate_str(&proc.name, 26),
                    proc.gpu_memory_mib(),
                    proc.process_type.short_label()
                );
            }
        }
        println!("╰───────┴────────┴────────────────────────────┴────────┴──────╯");
    }

    Ok(())
}

/// Run continuous JSON output
fn run_json_watch(monitor: &GpuMonitor, interval: u64) -> anyhow::Result<()> {
    use std::time::Duration;
    loop {
        let gpus = monitor.get_all_gpu_info()?;
        println!("{}", serde_json::to_string(&gpus)?);
        std::thread::sleep(Duration::from_millis(interval));
    }
}

/// Run interactive TUI
fn run_tui(monitor: &GpuMonitor, interval: u64) -> anyhow::Result<()> {
    let mut terminal = tui::init()?;
    let result = app::App::new(interval).run(&mut terminal, monitor);
    tui::restore()?;
    result
}

/// Truncate string to max length
fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}
