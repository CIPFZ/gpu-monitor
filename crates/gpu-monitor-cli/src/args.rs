//! Command line parsing, and the reduction of those options to one explicit plan.
//!
//! The previous entry point decided what to run from a matrix of `--once`,
//! `--watch` and `--json`, which meant the behaviour of a command could only be
//! discovered by reading a chain of guards. Parsing now produces a `Plan`, so the
//! decision is stated once, is testable without a terminal, and cannot silently
//! change when a new command is added. The legacy flags remain accepted and are
//! translated into the same plans.

use clap::{Parser, Subcommand, ValueEnum};
use gpu_monitor_runtime::AlertConfig;
use std::path::PathBuf;

use crate::keymap::HISTORY_WINDOWS_MS;

#[derive(Parser, Debug)]
#[command(
    name = "gpu-monitor",
    author,
    version,
    about = "Real-time NVIDIA GPU monitoring",
    after_help = "Run without a command to open the terminal interface."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Output format for non-interactive commands
    #[arg(long, global = true, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,
    /// Shorthand for --format json
    #[arg(short, long, global = true, conflicts_with = "format")]
    pub json: bool,

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

    /// Sampling interval in milliseconds (minimum 100)
    #[arg(short, long, default_value = "1000", global = true, value_parser = clap::value_parser!(u64).range(100..))]
    pub interval: u64,
    /// Initial chart window in seconds
    #[arg(long, global = true, default_value = "60", value_parser = history_seconds)]
    pub history_seconds: u64,
    /// Enable local threshold and availability alerts while monitoring
    #[arg(long, global = true)]
    pub alerts: bool,

    /// Deprecated: use the snapshot command, or replay --first-frame
    #[arg(short, long, global = true, hide = true, conflicts_with = "watch")]
    pub once: bool,
    /// Deprecated: use the watch or stream command
    #[arg(short, long, global = true, hide = true)]
    pub watch: bool,
}

#[derive(Clone, Copy, Debug, Default, ValueEnum, PartialEq, Eq)]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
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
pub enum Command {
    /// Open the terminal interface (the default when no command is given)
    Watch,
    /// Print one sample and exit
    Snapshot,
    /// Print every completed sample until interrupted
    Stream,
    /// Report GPU processes, their owners and running times
    Processes {
        /// Keep printing process samples until interrupted
        #[arg(long)]
        stream: bool,
    },
    /// Report device identity, measurement support and driver diagnostics
    Diagnostics,
    /// Record all devices to a new JSON Lines file (view filters do not change it)
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
        /// Validate the whole recording, then report only its first frame
        #[arg(long)]
        first_frame: bool,
    },
    /// Report alert events with explicit thresholds and recovery hysteresis
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
}

/// Which part of a sample a report covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Full,
    Processes,
    Diagnostics,
}

/// Exactly what the process will do, decided once during parsing.
#[derive(Clone, Debug, PartialEq)]
pub enum Plan {
    /// Interactive interface over the live runtime.
    Watch,
    Snapshot(Scope),
    Stream(Scope),
    Record {
        path: PathBuf,
        duration: Option<u64>,
    },
    Replay {
        path: PathBuf,
        speed: f64,
        /// Validate the whole recording but report only its first frame.
        first_frame_only: bool,
    },
    Alerts(AlertConfig),
}

impl Cli {
    pub fn output_format(&self) -> OutputFormat {
        if self.json {
            OutputFormat::Json
        } else {
            self.format
        }
    }

    pub fn wants_json(&self) -> bool {
        self.output_format() == OutputFormat::Json
    }

    /// Warnings for superseded flags, reported once before work begins.
    pub fn deprecations(&self) -> Vec<&'static str> {
        let mut notices = Vec::new();
        if self.once {
            notices.push("--once is deprecated; use the snapshot command, or replay --first-frame");
        }
        if self.watch {
            notices.push("--watch is deprecated; use the watch or stream command");
        }
        notices
    }

    pub fn plan(&self) -> Result<Plan, String> {
        let json = self.wants_json();
        Ok(match &self.command {
            Some(Command::Watch) => Plan::Watch,
            Some(Command::Snapshot) => Plan::Snapshot(Scope::Full),
            Some(Command::Stream) => Plan::Stream(Scope::Full),
            Some(Command::Diagnostics) => Plan::Snapshot(Scope::Diagnostics),
            Some(Command::Processes { stream }) => {
                // The interface shows processes as one of its views, so an
                // interactive request stays interactive.
                if *stream || (self.watch && json) {
                    Plan::Stream(Scope::Processes)
                } else if self.watch {
                    Plan::Watch
                } else {
                    Plan::Snapshot(Scope::Processes)
                }
            }
            Some(Command::Record { path, duration }) => Plan::Record {
                path: path.clone(),
                duration: *duration,
            },
            Some(Command::Replay {
                path,
                speed,
                first_frame,
            }) => Plan::Replay {
                path: path.clone(),
                speed: *speed,
                first_frame_only: *first_frame || self.once,
            },
            Some(Command::Alerts {
                temperature,
                temperature_recovery,
                memory,
                memory_recovery,
                duration_seconds,
                cooldown_seconds,
            }) => {
                let config = AlertConfig {
                    enabled: true,
                    temperature_threshold: *temperature,
                    temperature_recovery: *temperature_recovery,
                    memory_threshold: *memory,
                    memory_recovery: *memory_recovery,
                    duration_ms: seconds_to_ms(*duration_seconds)?,
                    cooldown_ms: seconds_to_ms(*cooldown_seconds)?,
                };
                config.validate()?;
                Plan::Alerts(config)
            }
            None if self.once => Plan::Snapshot(Scope::Full),
            None if self.watch && json => Plan::Stream(Scope::Full),
            None if self.watch => Plan::Watch,
            // A bare --json request has no interactive meaning.
            None if json => Plan::Snapshot(Scope::Full),
            None => Plan::Watch,
        })
    }
}

fn seconds_to_ms(seconds: f64) -> Result<u64, String> {
    if !seconds.is_finite() || seconds < 0.0 || seconds > u64::MAX as f64 / 1000.0 {
        return Err("Duration is outside the supported range".into());
    }
    Ok((seconds * 1000.0) as u64)
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

/// Accepts exactly the windows the interface can switch between at runtime, so
/// the command line and the `w` key always offer the same set.
fn history_seconds(input: &str) -> Result<u64, String> {
    let requested = input
        .parse::<u64>()
        .map_err(|_| "expected a whole number of seconds")?;
    let offered = HISTORY_WINDOWS_MS.map(|window| window / 1000);
    if offered.contains(&requested) {
        return Ok(requested);
    }
    Err(format!(
        "history seconds must be one of: {}",
        offered
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(arguments: &[&str]) -> Plan {
        let mut argv = vec!["gpu-monitor"];
        argv.extend_from_slice(arguments);
        Cli::try_parse_from(argv)
            .expect("arguments should parse")
            .plan()
            .expect("plan should resolve")
    }

    #[test]
    fn the_default_invocation_is_the_interactive_interface() {
        assert_eq!(plan(&[]), Plan::Watch);
        assert_eq!(plan(&["watch"]), Plan::Watch);
    }

    #[test]
    fn reports_choose_their_scope_explicitly() {
        assert_eq!(plan(&["snapshot"]), Plan::Snapshot(Scope::Full));
        assert_eq!(plan(&["stream"]), Plan::Stream(Scope::Full));
        assert_eq!(plan(&["processes"]), Plan::Snapshot(Scope::Processes));
        assert_eq!(
            plan(&["processes", "--stream"]),
            Plan::Stream(Scope::Processes)
        );
        assert_eq!(plan(&["diagnostics"]), Plan::Snapshot(Scope::Diagnostics));
    }

    #[test]
    fn the_superseded_flag_matrix_still_resolves_to_the_same_plans() {
        assert_eq!(plan(&["--once"]), Plan::Snapshot(Scope::Full));
        assert_eq!(plan(&["--json"]), Plan::Snapshot(Scope::Full));
        assert_eq!(plan(&["--watch"]), Plan::Watch);
        assert_eq!(plan(&["--watch", "--json"]), Plan::Stream(Scope::Full));
        assert_eq!(
            plan(&["--watch", "--json", "processes"]),
            Plan::Stream(Scope::Processes)
        );
        assert_eq!(
            plan(&["--watch", "processes"]),
            Plan::Watch,
            "an interactive process request stays interactive"
        );
        assert_eq!(
            plan(&["--once", "processes"]),
            Plan::Snapshot(Scope::Processes)
        );
    }

    #[test]
    fn superseded_flags_are_reported_once_and_only_when_used() {
        let quiet = Cli::try_parse_from(["gpu-monitor", "snapshot"]).unwrap();
        assert!(quiet.deprecations().is_empty());
        let noisy = Cli::try_parse_from(["gpu-monitor", "--watch"]).unwrap();
        assert_eq!(noisy.deprecations().len(), 1);
        assert!(noisy.deprecations()[0].contains("--watch"));
        assert!(Cli::try_parse_from(["gpu-monitor", "--once", "--watch"]).is_err());
    }

    #[test]
    fn the_format_flag_and_its_shorthand_agree_but_cannot_be_combined() {
        assert!(!Cli::try_parse_from(["gpu-monitor"]).unwrap().wants_json());
        assert!(Cli::try_parse_from(["gpu-monitor", "--json"])
            .unwrap()
            .wants_json());
        assert!(Cli::try_parse_from(["gpu-monitor", "--format", "json"])
            .unwrap()
            .wants_json());
        assert!(Cli::try_parse_from(["gpu-monitor", "--format", "json", "--json"]).is_err());
        assert!(Cli::try_parse_from(["gpu-monitor", "--format", "yaml"]).is_err());
    }

    #[test]
    fn replay_validates_the_file_before_reporting_a_single_frame() {
        assert_eq!(
            plan(&["replay", "session.jsonl", "--speed", "2"]),
            Plan::Replay {
                path: PathBuf::from("session.jsonl"),
                speed: 2.0,
                first_frame_only: false,
            }
        );
        for arguments in [
            vec!["replay", "session.jsonl", "--first-frame"],
            vec!["--once", "replay", "session.jsonl"],
        ] {
            assert_eq!(
                plan(&arguments),
                Plan::Replay {
                    path: PathBuf::from("session.jsonl"),
                    speed: 1.0,
                    first_frame_only: true,
                }
            );
        }
    }

    #[test]
    fn alert_thresholds_are_validated_while_the_plan_is_built() {
        let Plan::Alerts(config) = plan(&["alerts", "--temperature", "90"]) else {
            panic!("expected an alert plan");
        };
        assert!(config.enabled);
        assert_eq!(config.temperature_threshold, 90.0);
        assert_eq!(config.duration_ms, AlertConfig::default().duration_ms);
        let reversed = Cli::try_parse_from([
            "gpu-monitor",
            "alerts",
            "--temperature",
            "80",
            "--temperature-recovery",
            "90",
        ])
        .unwrap();
        assert!(
            reversed.plan().is_err(),
            "a recovery limit above the trigger cannot be applied later"
        );
        assert!(Cli::try_parse_from(["gpu-monitor", "alerts", "--memory", "101"]).is_err());
    }

    #[test]
    fn rejected_values_cannot_create_a_busy_loop_or_an_unreachable_window() {
        for interval in ["0", "99"] {
            assert!(Cli::try_parse_from(["gpu-monitor", "--interval", interval]).is_err());
        }
        assert!(Cli::try_parse_from(["gpu-monitor", "--interval", "100"]).is_ok());
        for window in HISTORY_WINDOWS_MS.map(|window| (window / 1000).to_string()) {
            assert!(Cli::try_parse_from(["gpu-monitor", "--history-seconds", &window]).is_ok());
        }
        let rejected = Cli::try_parse_from(["gpu-monitor", "--history-seconds", "61"]).unwrap_err();
        assert!(
            rejected.to_string().contains("60"),
            "the error lists the windows the interface actually offers"
        );
        for speed in ["0", "NaN", "inf", "-1", "1000001"] {
            assert!(
                Cli::try_parse_from(["gpu-monitor", "replay", "a.jsonl", "--speed", speed])
                    .is_err()
            );
        }
        for value in ["NaN", "inf", "-inf", "-1"] {
            assert!(Cli::try_parse_from(["gpu-monitor", "--min-free-gib", value]).is_err());
        }
        assert!(
            Cli::try_parse_from(["gpu-monitor", "record", "a.jsonl", "--duration", "0"]).is_err()
        );
    }

    #[test]
    fn device_and_privacy_selections_are_preserved_across_commands() {
        let parsed = Cli::try_parse_from([
            "gpu-monitor",
            "replay",
            "session.jsonl",
            "--json",
            "--gpu",
            "uuid-0",
            "--gpu",
            "uuid-1",
            "--include-command",
        ])
        .unwrap();
        assert!(parsed.wants_json() && parsed.include_command);
        assert_eq!(parsed.gpus, ["uuid-0", "uuid-1"]);
        assert!(Cli::try_parse_from(["gpu-monitor", "--gpu", "   "]).is_err());
    }
}
