//! Terminal monitoring, recording and replay over the shared background runtime.
mod app;
mod args;
mod feed;
mod output;
mod selection;
#[cfg(test)]
mod tests;
mod tui;
mod ui;

use args::{Cli, Commands};
use clap::Parser;
use feed::{Feed, LiveFeed, Playback};
use gpu_monitor_core::MonitorSnapshot;
use gpu_monitor_runtime::{load_recording, AlertConfig, MonitorRuntime};
use output::{print_snapshot, selection_result};
#[cfg(test)]
use output::{process_snapshot_json, snapshot_result, truncate_str};
use selection::Selection;
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::WARN.into()),
        )
        .init();
    let selection = Selection::from(&cli);
    if let Some(Commands::Replay { path, speed }) = &cli.command {
        return replay(path, *speed, &cli, &selection);
    }
    let runtime = MonitorRuntime::new(Duration::from_millis(cli.interval));
    runtime
        .configure_alerts(AlertConfig {
            enabled: cli.alerts,
            ..AlertConfig::default()
        })
        .map_err(anyhow::Error::msg)?;
    match &cli.command {
        Some(Commands::Record { path, duration }) => {
            record(&runtime, path, *duration, cli.include_command)
        }
        Some(Commands::Alerts {
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
            runtime
                .configure_alerts(config)
                .map_err(anyhow::Error::msg)?;
            alerts(&runtime, cli.json, &selection)
        }
        Some(Commands::Diagnostics) => {
            let original = wait_for_sample(&runtime)?;
            let selected = selection.apply(&original);
            if cli.json {
                print_snapshot(&selected, true, false, selection.has_device_filter())?;
            } else {
                print!("{}", output::diagnostics_text(&selected));
            }
            selection_result(&original, &selected, &selection)
        }
        Some(Commands::Processes) if cli.watch => {
            if cli.json {
                stream(&runtime, &selection, true)
            } else {
                let mut terminal = tui::init()?;
                app::App::new(cli.interval)
                    .with_options(selection, cli.history_seconds * 1000)
                    .run(&mut terminal.terminal, &mut LiveFeed::new(&runtime))
            }
        }
        Some(Commands::Processes) => {
            let original = wait_for_sample(&runtime)?;
            let selected = selection.apply(&original);
            print_snapshot(&selected, cli.json, true, selection.has_device_filter())?;
            selection_result(&original, &selected, &selection)
        }
        _ if cli.once || (cli.json && !cli.watch) => {
            let original = wait_for_sample(&runtime)?;
            let selected = selection.apply(&original);
            print_snapshot(&selected, cli.json, false, selection.has_device_filter())?;
            selection_result(&original, &selected, &selection)
        }
        _ if cli.json => stream(&runtime, &selection, false),
        _ => {
            let mut terminal = tui::init()?;
            let mut feed = LiveFeed::new(&runtime);
            app::App::new(cli.interval)
                .with_options(selection, cli.history_seconds * 1000)
                .run(&mut terminal.terminal, &mut feed)
        }
    }
}
fn seconds_to_ms(seconds: f64) -> anyhow::Result<u64> {
    if !seconds.is_finite() || seconds < 0.0 || seconds > u64::MAX as f64 / 1000.0 {
        anyhow::bail!("Duration is outside the supported range");
    }
    Ok((seconds * 1000.0) as u64)
}
fn interrupt_flag() -> anyhow::Result<Arc<AtomicBool>> {
    let stopped = Arc::new(AtomicBool::new(false));
    let handler = stopped.clone();
    ctrlc::set_handler(move || handler.store(true, Ordering::Relaxed))?;
    Ok(stopped)
}
fn wait_for_sample(runtime: &MonitorRuntime) -> anyhow::Result<MonitorSnapshot> {
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        match runtime.latest() {
            Ok(snapshot) => return Ok(snapshot),
            Err(error) if Instant::now() >= until => {
                anyhow::bail!("No completed sample within 30 seconds: {error}")
            }
            Err(_) => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}
fn stream(
    runtime: &MonitorRuntime,
    selection: &Selection,
    processes_only: bool,
) -> anyhow::Result<()> {
    let stopped = interrupt_flag()?;
    let mut feed = LiveFeed::new(runtime);
    while !stopped.load(Ordering::Relaxed) {
        match feed.poll() {
            Ok(snapshots) => {
                for snapshot in snapshots {
                    let selected = selection.apply(&snapshot);
                    if processes_only {
                        println!(
                            "{}",
                            serde_json::to_string(&output::process_snapshot_json(&selected))?
                        );
                    } else {
                        println!("{}", serde_json::to_string(&selected)?);
                    }
                }
            }
            Err(_) => {} // The first completed snapshot includes initialization errors.
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(())
}
fn record(
    runtime: &MonitorRuntime,
    path: &Path,
    duration: Option<u64>,
    include_commands: bool,
) -> anyhow::Result<()> {
    let stopped = interrupt_flag()?;
    runtime
        .start_recording(path.to_path_buf(), include_commands)
        .map_err(anyhow::Error::msg)?;
    eprintln!(
        "Recording all devices to {}. Ctrl-C finishes and flushes the file.",
        path.display()
    );
    let started = Instant::now();
    while !stopped.load(Ordering::Relaxed)
        && duration.is_none_or(|seconds| started.elapsed() < Duration::from_secs(seconds))
    {
        let status = runtime.recording_status();
        if let Some(error) = status.error {
            anyhow::bail!("Recording failed: {error}");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    runtime.stop_recording().map_err(anyhow::Error::msg)?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let status = runtime.recording_status();
        if let Some(error) = &status.error {
            anyhow::bail!("Recording failed: {error}");
        }
        if !status.finishing {
            eprintln!(
                "Recorded {} samples ({} bytes); {} samples dropped.",
                status.samples_written, status.bytes_written, status.dropped_samples
            );
            if status.samples_written == 0 {
                anyhow::bail!("No completed samples were recorded; retry with a longer duration or check the sampler");
            }
            if status.dropped_samples > 0 {
                anyhow::bail!(
                    "Recording is incomplete: {} samples dropped",
                    status.dropped_samples
                );
            }
            return Ok(());
        }
        if Instant::now() >= deadline {
            anyhow::bail!(
                "Timed out while flushing recording; check {} before using it",
                path.display()
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
fn replay(path: &Path, speed: f64, cli: &Cli, selection: &Selection) -> anyhow::Result<()> {
    let frames = load_recording(path).map_err(anyhow::Error::msg)?;
    if cli.once {
        let frame = frames
            .first()
            .ok_or_else(|| anyhow::anyhow!("Recording contains no snapshots"))?;
        let selected = selection.apply(frame);
        print_snapshot(&selected, cli.json, false, selection.has_device_filter())?;
        return selection_result(frame, &selected, selection);
    }
    let mut playback = Playback::new(frames, speed).map_err(anyhow::Error::msg)?;
    if cli.json {
        let stopped = interrupt_flag()?;
        while !playback.is_finished() && !stopped.load(Ordering::Relaxed) {
            for snapshot in playback.poll().map_err(anyhow::Error::msg)? {
                println!("{}", serde_json::to_string(&selection.apply(&snapshot))?);
            }
            if !playback.is_finished() {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        Ok(())
    } else {
        let mut terminal = tui::init()?;
        app::App::new(cli.interval)
            .with_options(selection.clone(), cli.history_seconds * 1000)
            .run(&mut terminal.terminal, &mut playback)
    }
}
fn alerts(runtime: &MonitorRuntime, json: bool, selection: &Selection) -> anyhow::Result<()> {
    let stopped = interrupt_flag()?;
    if !json {
        eprintln!(
            "Alert monitoring active. Ctrl-C exits. Threshold configuration: {}",
            serde_json::to_string(&runtime.alert_config())?
        );
    }
    let mut last_id = 0;
    while !stopped.load(Ordering::Relaxed) {
        for event in runtime
            .events()
            .into_iter()
            .filter(|event| event.id > last_id)
            .collect::<Vec<_>>()
        {
            last_id = last_id.max(event.id);
            if !selection.matches_event(&event) {
                continue;
            }
            if json {
                println!("{}", serde_json::to_string(&event)?);
            } else {
                println!(
                    "{} {:?} {:?} {}: {}",
                    event.at_ms,
                    event.kind,
                    event.state,
                    output::safe_text(event.gpu_uuid.as_deref().unwrap_or("monitor")),
                    output::safe_text(&event.message)
                );
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}
