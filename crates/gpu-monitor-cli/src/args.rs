use clap::{Parser, Subcommand, ValueEnum};
use gpu_monitor_runtime::AlertConfig;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "gpu-monitor",
    author,
    version,
    about = "Real-time NVIDIA GPU monitoring"
)]
pub struct Cli {
    /// Print one snapshot and exit
    #[arg(short, long, global = true, conflicts_with = "watch")]
    pub once: bool,
    /// Stream snapshots or display the live terminal interface
    #[arg(short, long, global = true)]
    pub watch: bool,
    /// Print JSON (JSON Lines for streaming and replay)
    #[arg(short, long, global = true)]
    pub json: bool,
    /// Sampling interval in milliseconds (minimum 100)
    #[arg(short, long, default_value = "1000", global = true, value_parser = clap::value_parser!(u64).range(100..))]
    pub interval: u64,
    /// Select an exact GPU UUID; repeat to select several devices
    #[arg(long = "gpu", global = true, value_name = "UUID", value_parser = nonempty)]
    pub gpus: Vec<String>,
    /// Require at least this much known free GPU memory, in GiB
    #[arg(long, global = true, value_parser = nonnegative)]
    pub min_free_gib: Option<f64>,
    /// Filter processes by exact owner name or numeric UID
    #[arg(long, global = true, value_parser = nonempty)]
    pub user: Option<String>,
    /// GPU order; numeric metrics descend and unavailable values sort last
    #[arg(long, global = true, value_enum, default_value_t = SortBy::Index)]
    pub sort: SortBy,
    /// Include full process argument vectors in output and recordings
    #[arg(long, global = true)]
    pub include_command: bool,
    /// Chart time window in seconds: 60, 300 or 3600
    #[arg(long, global = true, default_value = "60", value_parser = history_seconds)]
    pub history_seconds: u64,
    /// Enable local threshold and availability alerts while monitoring
    #[arg(long, global = true)]
    pub alerts: bool,
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Clone, Copy, Debug, Default, ValueEnum, PartialEq, Eq)]
pub enum SortBy {
    #[default]
    Index,
    FreeMemory,
    Utilization,
    Temperature,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// List GPU processes, their owners and running times
    Processes,
    /// Record all devices to a new JSON Lines file (view filters do not change the recording)
    Record {
        path: PathBuf,
        /// Stop after this many seconds; otherwise press Ctrl-C to finish
        #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
        duration: Option<u64>,
    },
    /// Play a recorded session without requiring NVIDIA hardware
    Replay {
        path: PathBuf,
        /// Playback speed multiplier; JSON also follows the recorded timing
        #[arg(long, default_value = "1", value_parser = positive)]
        speed: f64,
    },
    /// Monitor alert events with explicit thresholds and recovery hysteresis
    Alerts {
        #[arg(long, default_value_t = AlertConfig::default().temperature_threshold, value_parser = nonnegative)]
        temperature: f64,
        #[arg(long, default_value_t = AlertConfig::default().temperature_recovery, value_parser = nonnegative)]
        temperature_recovery: f64,
        #[arg(long, default_value_t = AlertConfig::default().memory_threshold, value_parser = percentage)]
        memory: f64,
        #[arg(long, default_value_t = AlertConfig::default().memory_recovery, value_parser = percentage)]
        memory_recovery: f64,
        #[arg(long, default_value_t = AlertConfig::default().duration_ms as f64 / 1000.0, value_parser = nonnegative)]
        duration_seconds: f64,
        #[arg(long, default_value_t = AlertConfig::default().cooldown_ms as f64 / 1000.0, value_parser = nonnegative)]
        cooldown_seconds: f64,
    },
    /// Print the latest device identity, measurement support and driver diagnostics
    Diagnostics,
}

fn nonempty(input: &str) -> Result<String, String> {
    if input.trim().is_empty() {
        Err("value must not be empty".into())
    } else {
        Ok(input.into())
    }
}
fn nonnegative(input: &str) -> Result<f64, String> {
    let value = input
        .parse::<f64>()
        .map_err(|_| "expected a finite nonnegative number")?;
    if value.is_finite() && value >= 0.0 {
        Ok(value)
    } else {
        Err("expected a finite nonnegative number".into())
    }
}
fn positive(input: &str) -> Result<f64, String> {
    let value = nonnegative(input)?;
    if value > 0.0 && value <= 1_000_000.0 {
        Ok(value)
    } else {
        Err("speed must be greater than zero and at most 1000000".into())
    }
}
fn percentage(input: &str) -> Result<f64, String> {
    let value = nonnegative(input)?;
    if value <= 100.0 {
        Ok(value)
    } else {
        Err("percentage must be between 0 and 100".into())
    }
}
fn history_seconds(input: &str) -> Result<u64, String> {
    match input {
        "60" => Ok(60),
        "300" => Ok(300),
        "3600" => Ok(3600),
        _ => Err("history seconds must be 60, 300 or 3600".into()),
    }
}
