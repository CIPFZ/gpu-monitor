//! Terminal monitoring, reporting, recording and replay over the shared runtime.
//!
//! `main` dispatches on the `Plan` produced during parsing, so what runs is
//! decided in one place instead of by a chain of flag combinations.

mod action;
mod app;
mod args;
mod device;
mod feed;
mod format;
mod keymap;
mod layout;
mod output;
mod render;
mod selection;
mod terminal;
#[cfg(test)]
mod tests;
mod theme;
mod view;
mod widgets;

use args::{Cli, OutputFormat, Plan, Scope};
use clap::Parser;
use feed::{Feed, LiveFeed, Playback};
use gpu_monitor_core::MonitorSnapshot;
use gpu_monitor_runtime::{load_recording, AlertConfig, MonitorRuntime};
use output::{print_snapshot, selection_result};
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
    for notice in cli.deprecations() {
        eprintln!("warning: {notice}");
    }
    let plan = cli.plan().map_err(anyhow::Error::msg)?;
    let selection = Selection::from(&cli);

    // Replaying a file must never require a driver, so no runtime is created.
    if let Plan::Replay {
        path,
        speed,
        first_frame_only,
    } = &plan
    {
        return replay(path, *speed, *first_frame_only, &cli, &selection);
    }

    let runtime = MonitorRuntime::new(Duration::from_millis(cli.interval));
    runtime
        .configure_alerts(AlertConfig {
            enabled: cli.alerts,
            ..AlertConfig::default()
        })
        .map_err(anyhow::Error::msg)?;
    match plan {
        Plan::Watch => watch(&runtime, &cli, selection),
        Plan::Snapshot(scope) => report(&runtime, &cli, &selection, scope),
        Plan::Stream(scope) => stream(&runtime, &cli, &selection, scope),
        Plan::Record { path, duration } => record(&runtime, &path, duration, cli.include_command),
        Plan::Alerts(config) => {
            runtime
                .configure_alerts(config)
                .map_err(anyhow::Error::msg)?;
            alerts(&runtime, cli.wants_json(), &selection)
        }
        Plan::Replay { .. } => unreachable!("replay is handled without a runtime"),
    }
}

fn watch(runtime: &MonitorRuntime, cli: &Cli, selection: Selection) -> anyhow::Result<()> {
    let mut session = terminal::init()?;
    let mut feed = LiveFeed::new(runtime);
    app::App::new(cli.interval)
        .with_options(selection, cli.history_seconds * 1000)
        .run(&mut session.terminal, &mut feed)
}

fn report(
    runtime: &MonitorRuntime,
    cli: &Cli,
    selection: &Selection,
    scope: Scope,
) -> anyhow::Result<()> {
    let original = wait_for_sample(runtime)?;
    let selected = selection.apply(&original);
    let filtered = selection.has_device_filter();
    let emitted = match scope {
        Scope::Diagnostics if !cli.wants_json() => {
            output::emit_text(&output::diagnostics_text(&selected))?
        }
        Scope::Diagnostics => print_snapshot(&selected, OutputFormat::Json, false, filtered)?,
        scope => print_snapshot(
            &selected,
            cli.output_format(),
            scope == Scope::Processes,
            filtered,
        )?,
    };
    if emitted.reader_closed() {
        // Nobody is left to receive a selection diagnosis.
        return Ok(());
    }
    selection_result(&original, &selected, selection)
}

fn stream(
    runtime: &MonitorRuntime,
    cli: &Cli,
    selection: &Selection,
    scope: Scope,
) -> anyhow::Result<()> {
    let stopped = interrupt_flag()?;
    let mut feed = LiveFeed::new(runtime);
    while !stopped.load(Ordering::Relaxed) {
        // The first completed snapshot carries any initialization error, so an
        // empty cache is simply "not ready yet".
        if let Ok(snapshots) = feed.poll() {
            for snapshot in snapshots {
                let selected = selection.apply(&snapshot);
                if print_frame(&selected, cli, selection, scope)?.reader_closed() {
                    return Ok(());
                }
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(())
}

fn print_frame(
    snapshot: &MonitorSnapshot,
    cli: &Cli,
    selection: &Selection,
    scope: Scope,
) -> anyhow::Result<output::Emit> {
    if cli.wants_json() {
        let line = if scope == Scope::Processes {
            serde_json::to_string(&output::process_snapshot_json(snapshot))?
        } else {
            serde_json::to_string(snapshot)?
        };
        return output::emit_line(&line);
    }
    let report = output::snapshot_text(
        snapshot,
        scope == Scope::Processes,
        selection.has_device_filter(),
    );
    output::emit_text(&format!("{report}{}\n", "─".repeat(40)))
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

fn replay(
    path: &Path,
    speed: f64,
    first_frame_only: bool,
    cli: &Cli,
    selection: &Selection,
) -> anyhow::Result<()> {
    // The whole file is validated before anything is reported.
    let frames = load_recording(path).map_err(anyhow::Error::msg)?;
    if first_frame_only {
        let frame = frames
            .first()
            .ok_or_else(|| anyhow::anyhow!("Recording contains no snapshots"))?;
        let selected = selection.apply(frame);
        if print_snapshot(
            &selected,
            cli.output_format(),
            false,
            selection.has_device_filter(),
        )?
        .reader_closed()
        {
            return Ok(());
        }
        return selection_result(frame, &selected, selection);
    }
    let mut playback = Playback::new(frames, speed).map_err(anyhow::Error::msg)?;
    if cli.wants_json() {
        let stopped = interrupt_flag()?;
        while !playback.is_finished() && !stopped.load(Ordering::Relaxed) {
            for snapshot in playback.poll().map_err(anyhow::Error::msg)? {
                let line = serde_json::to_string(&selection.apply(&snapshot))?;
                if output::emit_line(&line)?.reader_closed() {
                    return Ok(());
                }
            }
            if !playback.is_finished() {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        return Ok(());
    }
    let mut session = terminal::init()?;
    app::App::new(cli.interval)
        .with_options(selection.clone(), cli.history_seconds * 1000)
        .run(&mut session.terminal, &mut playback)
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
            let line = if json {
                serde_json::to_string(&event)?
            } else {
                output::alert_report_line(&event)
            };
            if output::emit_line(&line)?.reader_closed() {
                return Ok(());
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}
