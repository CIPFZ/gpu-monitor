use crate::{app::App, ui};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use gpu_monitor_core::{GpuInfo, MonitorSnapshot};
use ratatui::{backend::TestBackend, Terminal};
use serde_json::json;

fn gpu(index: u32, count: u32, timestamp: u64) -> GpuInfo {
    serde_json::from_value(json!({
        "device": {"index": index, "name": format!("Test GPU {index}"), "uuid": format!("uuid-{index}"), "pci_bus_id": "0", "driver_version": "test", "cuda_version": null, "power_limit": 250, "power_limit_max": 300},
        "metrics": {"gpu_utilization": 42, "memory_utilization": 15, "encoder_utilization": 0, "decoder_utilization": 0, "temperature": 65, "power_usage": 80000, "fan_speed": 30, "clock_graphics": 1500, "clock_memory": 7000, "clock_sm": 1500},
        "memory": {"total": 17179869184u64, "used": 8589934592u64, "free": 8589934592u64},
        "processes": (0..count).map(|process| json!({"pid": index * 1000 + process, "name": format!("gpu{index}-process-{process:03}"), "gpu_memory": 1048576, "process_type": "Compute"})).collect::<Vec<_>>(),
        "sampled_at_ms": timestamp, "issues": []
    })).unwrap()
}

fn snapshot(gpus: Vec<GpuInfo>, timestamp: u64) -> MonitorSnapshot {
    MonitorSnapshot {
        schema_version: gpu_monitor_core::SCHEMA_VERSION,
        sampled_at_ms: timestamp,
        gpus,
        failures: vec![],
        error: None,
    }
}

fn key(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}
fn render(app: &mut App, terminal: &mut Terminal<TestBackend>) -> String {
    terminal.draw(|frame| ui::draw(frame, app)).unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>()
}

#[test]
fn issue4_two_cards_eight_processes_are_independently_scrollable() {
    let mut app = App::new(1000);
    app.apply_snapshot(snapshot(vec![gpu(0, 8, 1), gpu(1, 8, 1)], 1));
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    for index in 0..2 {
        let first = render(&mut app, &mut terminal);
        assert!(first.contains(&format!("GPU {index}: Test GPU {index}")));
        assert!(first.contains("Temperature") || first.contains("Temp: 65°C"));
        assert!(first.contains("Clocks: graphics 1500MHz"));
        assert!(first.contains("Memory capacity: 8.0/16.0 GiB"));
        assert!(!first.contains(&format!("gpu{index}-process-007")));
        key(&mut app, KeyCode::End);
        let last = render(&mut app, &mut terminal);
        assert!(last.contains(&format!("gpu{index}-process-007")));
        assert!(app.selected_view().unwrap().process_scroll > 0);
        key(&mut app, KeyCode::Tab);
    }
    assert!(app.selected_view().unwrap().process_scroll > 0);
}

#[test]
fn issue4_eight_gpus_small_screen_each_has_full_detail() {
    let mut app = App::new(1000);
    app.apply_snapshot(snapshot((0..8).map(|index| gpu(index, 8, 1)).collect(), 1));
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    for index in 0..8 {
        let output = render(&mut app, &mut terminal);
        assert!(output.contains(&format!("Device {}/8", index + 1)));
        assert!(output.contains(&format!("GPU {index}: Test GPU {index}")));
        assert!(output.contains("Temp: 65°C"));
        assert!(output.contains("graphics 1500MHz"));
        assert!(output.contains("Memory capacity: 8.0/16.0 GiB"));
        key(&mut app, KeyCode::End);
        assert!(render(&mut app, &mut terminal).contains(&format!("gpu{index}-process-007")));
        key(&mut app, KeyCode::Right);
    }
    assert_eq!(app.selected_position(), 0);
}

#[test]
fn issue4_empty_first_gpu_does_not_limit_second_gpu_and_resize_clamps() {
    let mut app = App::new(1000);
    app.apply_snapshot(snapshot(vec![gpu(0, 0, 1), gpu(1, 30, 1)], 1));
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    render(&mut app, &mut terminal);
    key(&mut app, KeyCode::Right);
    render(&mut app, &mut terminal);
    key(&mut app, KeyCode::End);
    assert!(render(&mut app, &mut terminal).contains("gpu1-process-029"));
    let original = app.selected_view().unwrap().process_scroll;
    terminal.backend_mut().resize(80, 19);
    render(&mut app, &mut terminal);
    key(&mut app, KeyCode::End);
    assert!(app.selected_view().unwrap().process_scroll > original);
    assert!(render(&mut app, &mut terminal).contains("gpu1-process-029"));
    terminal.backend_mut().resize(80, 50);
    assert!(render(&mut app, &mut terminal).contains("gpu1-process-029"));
    assert_eq!(app.selected_view().unwrap().process_scroll, 0);
}

#[test]
fn issue4_keyboard_scrolling_obeys_actual_page_size() {
    let mut app = App::new(1000);
    app.apply_snapshot(snapshot(vec![gpu(0, 40, 1)], 1));
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    render(&mut app, &mut terminal);
    key(&mut app, KeyCode::PageDown);
    let page = app.selected_view().unwrap().process_scroll;
    assert!(page > 0 && page < 40);
    key(&mut app, KeyCode::Down);
    assert_eq!(app.selected_view().unwrap().process_scroll, page + 1);
    key(&mut app, KeyCode::PageUp);
    assert_eq!(app.selected_view().unwrap().process_scroll, 1);
    key(&mut app, KeyCode::Home);
    assert_eq!(app.selected_view().unwrap().process_scroll, 0);
}

#[test]
fn sampling_history_survives_reordering_failure_and_recovery_by_uuid() {
    let mut app = App::new(1000);
    app.apply_snapshot(snapshot(vec![gpu(0, 15, 1), gpu(1, 15, 1)], 1));
    app.set_process_viewport(5);
    key(&mut app, KeyCode::Right);
    key(&mut app, KeyCode::End);
    let mut selected = gpu(1, 15, 2);
    selected.device.index = 0;
    let mut other = gpu(0, 15, 2);
    other.device.index = 1;
    app.apply_snapshot(snapshot(vec![selected, other], 2));
    assert_eq!(
        app.selected_view()
            .unwrap()
            .gpu
            .as_ref()
            .unwrap()
            .device
            .uuid,
        "uuid-1"
    );
    assert_eq!(app.selected_view().unwrap().process_scroll, 10);
    let failure: MonitorSnapshot = serde_json::from_value(json!({"sampled_at_ms": 3, "gpus": [], "failures": [{"index": 0, "uuid": "uuid-1", "error": {"kind": "device_lost", "message": "device lost"}}], "error": null})).unwrap();
    app.apply_snapshot(failure);
    assert_eq!(app.selected_view().unwrap().history.len(), 3);
    assert_eq!(
        app.selected_view().unwrap().history.back().unwrap().gpu,
        None
    );
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    assert!(render(&mut app, &mut terminal).contains("STALE / unavailable: device lost"));
    app.apply_snapshot(snapshot(vec![gpu(1, 15, 4)], 4));
    assert_eq!(app.selected_view().unwrap().history.len(), 4);
    assert!(app.selected_view().unwrap().error.is_none());
    assert_eq!(
        app.selected_view().unwrap().history.back().unwrap().gpu,
        Some(42)
    );
}

#[test]
fn identical_values_are_samples_unknown_is_missing_and_history_is_bounded() {
    let mut app = App::new(1000);
    for timestamp in 1..=65 {
        let timestamp = timestamp * 1000;
        app.apply_snapshot(snapshot(vec![gpu(0, 0, timestamp)], timestamp));
    }
    assert_eq!(app.selected_view().unwrap().history.len(), 61);
    assert_eq!(
        app.selected_view()
            .unwrap()
            .history
            .front()
            .unwrap()
            .sampled_at_ms,
        5000
    );
    let mut missing = gpu(0, 0, 66000);
    missing.metrics.gpu_utilization = None;
    missing.metrics.temperature = None;
    missing.memory = None;
    app.apply_snapshot(snapshot(vec![missing.clone()], 66000));
    app.apply_snapshot(snapshot(vec![missing], 66000));
    let view = app.selected_view().unwrap();
    assert_eq!(view.history.len(), 61);
    assert_eq!(view.history.back().unwrap().gpu, None);
    assert_eq!(view.history.back().unwrap().memory, None);
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let output = render(&mut app, &mut terminal);
    assert!(output.contains("Temp: N/A"));
    assert!(output.contains("Memory capacity: N/A"));
    assert!(output.contains('×'));
}

#[test]
fn process_json_preserves_device_failure_unknown_memory_and_timestamp() {
    let mut gpu = gpu(1, 1, 10);
    gpu.processes[0].gpu_memory = None;
    let mut data = snapshot(vec![gpu], 11);
    data.failures = serde_json::from_value(
        json!([{"index": 0, "uuid": null, "error": {"kind": "device_lost", "message": "lost"}}]),
    )
    .unwrap();
    let output = serde_json::to_value(crate::process_snapshot_json(&data)).unwrap();
    assert_eq!(output["sampled_at_ms"], 11);
    assert_eq!(output["gpus"][0]["sampled_at_ms"], 10);
    assert_eq!(output["gpus"][0]["device"]["uuid"], "uuid-1");
    assert!(output["gpus"][0]["processes"][0]["gpu_memory_mib"].is_null());
    assert_eq!(output["failures"][0]["error"]["kind"], "device_lost");
    assert!(crate::snapshot_result(&data).is_ok());
    data.gpus.clear();
    assert!(crate::snapshot_result(&data).is_err());
}

#[test]
fn cli_rejects_busy_loop_intervals_and_truncates_unicode_without_panicking() {
    use clap::Parser;
    assert!(crate::Cli::try_parse_from(["gpu-monitor", "--interval", "0"]).is_err());
    assert!(crate::Cli::try_parse_from(["gpu-monitor", "--interval", "99"]).is_err());
    assert!(crate::Cli::try_parse_from(["gpu-monitor", "--interval", "100"]).is_ok());
    assert_eq!(crate::truncate_str("GPU训练进程", 6), "GPU训练…");
}

#[test]
fn issue4_short_terminal_keeps_processes_accessible_or_requests_resize() {
    let mut app = App::new(1000);
    app.apply_snapshot(snapshot(vec![gpu(0, 20, 1)], 1));
    let mut terminal = Terminal::new(TestBackend::new(80, 16)).unwrap();
    assert!(render(&mut app, &mut terminal).contains("gpu0-process-000"));
    key(&mut app, KeyCode::End);
    assert!(render(&mut app, &mut terminal).contains("gpu0-process-019"));
    terminal.backend_mut().resize(80, 10);
    assert!(render(&mut app, &mut terminal).contains("Terminal too small"));
    let previous_scroll = app.selected_view().unwrap().process_scroll;
    key(&mut app, KeyCode::Down);
    assert_eq!(app.selected_view().unwrap().process_scroll, previous_scroll);
    terminal.backend_mut().resize(80, 24);
    key(&mut app, KeyCode::Home);
    render(&mut app, &mut terminal);
    key(&mut app, KeyCode::End);
    assert!(render(&mut app, &mut terminal).contains("gpu0-process-019"));
}

#[test]
fn selection_combines_uuid_free_capacity_owner_and_command_privacy() {
    use crate::selection::Selection;
    let mut first = gpu(0, 2, 1000);
    first.processes[0].user = Some("alice".into());
    first.processes[0].uid = Some(1001);
    first.processes[0].command = Some(vec!["python".into(), "--secret=token".into()]);
    first.processes[1].user = Some("bob".into());
    first.processes[1].uid = Some(1002);
    let original = snapshot(vec![first, gpu(1, 2, 1000)], 1000);
    let mut selection = Selection {
        uuids: vec!["uuid-0".into()],
        min_free_gib: Some(8.0),
        user: Some("alice".into()),
        ..Selection::default()
    };
    let selected = selection.apply(&original);
    assert_eq!(selected.gpus.len(), 1);
    assert_eq!(selected.gpus[0].processes.len(), 1);
    assert!(selected.gpus[0].processes[0].command.is_none());
    assert!(!serde_json::to_string(&selected).unwrap().contains("secret"));
    assert!(
        original.gpus[0].processes[0].command.is_some(),
        "redaction cannot mutate the source snapshot"
    );
    selection.user = Some("1001".into());
    selection.include_command = true;
    assert_eq!(
        selection.apply(&original).gpus[0].processes[0]
            .command
            .as_ref()
            .unwrap()[1],
        "--secret=token"
    );
    selection.min_free_gib = Some(9.0);
    let empty = selection.apply(&original);
    assert!(empty.gpus.is_empty());
    assert!(crate::output::selection_result(&original, &empty, &selection).is_err());
    assert!(crate::output::snapshot_text(&empty, false, true).contains("No GPUs match"));
}

#[test]
fn free_memory_filter_does_not_treat_unknown_capacity_as_zero_and_retains_failures() {
    use crate::selection::Selection;
    let mut missing = gpu(0, 0, 1000);
    missing.memory = None;
    let mut original = snapshot(vec![missing, gpu(1, 0, 1000)], 1000);
    original.failures = serde_json::from_value(json!([{"index":2,"uuid":null,"error":{"kind":"device_lost","message":"identity unavailable"}}])).unwrap();
    let selected = Selection {
        min_free_gib: Some(0.0),
        ..Selection::default()
    }
    .apply(&original);
    assert_eq!(selected.gpus.len(), 1);
    assert_eq!(selected.gpus[0].device.index, 1);
    assert_eq!(selected.failures.len(), 1);
    assert!(crate::output::selection_result(&original, &selected, &Selection::default()).is_ok());
    let no_uuid = Selection {
        uuids: vec!["missing".into()],
        ..Selection::default()
    };
    assert!(no_uuid.apply(&original).gpus.is_empty());
    assert!(
        crate::output::selection_result(&original, &no_uuid.apply(&original), &no_uuid).is_err()
    );
}

#[test]
fn numeric_sorts_descend_put_unknown_last_and_break_ties_by_device_identity() {
    use crate::{args::SortBy, selection::Selection};
    let mut low = gpu(0, 0, 1000);
    low.metrics.gpu_utilization = Some(10);
    low.metrics.temperature = Some(40);
    let mut high = gpu(1, 0, 1000);
    high.metrics.gpu_utilization = Some(90);
    high.metrics.temperature = Some(90);
    high.memory.as_mut().unwrap().free = 16 * 1024_u64.pow(3);
    let mut unknown = gpu(2, 0, 1000);
    unknown.metrics.gpu_utilization = None;
    unknown.metrics.temperature = None;
    unknown.memory = None;
    let original = snapshot(vec![unknown, low, high], 1000);
    for sort in [SortBy::FreeMemory, SortBy::Utilization, SortBy::Temperature] {
        let selected = Selection {
            sort,
            ..Selection::default()
        }
        .apply(&original);
        assert_eq!(
            selected
                .gpus
                .iter()
                .map(|gpu| gpu.device.index)
                .collect::<Vec<_>>(),
            [1, 0, 2]
        );
    }
    let selected = Selection::default().apply(&original);
    assert_eq!(
        selected
            .gpus
            .iter()
            .map(|gpu| gpu.device.index)
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
}

#[test]
fn parser_rejects_nonfinite_filters_playback_speeds_and_invalid_history_windows() {
    use clap::Parser;
    for value in ["NaN", "inf", "-inf", "-1"] {
        assert!(crate::Cli::try_parse_from(["gpu-monitor", "--min-free-gib", value]).is_err());
    }
    for value in ["0", "NaN", "inf", "-1", "1000001"] {
        assert!(crate::Cli::try_parse_from([
            "gpu-monitor",
            "replay",
            "example.jsonl",
            "--speed",
            value
        ])
        .is_err());
    }
    for value in ["60", "300", "3600"] {
        assert!(crate::Cli::try_parse_from(["gpu-monitor", "--history-seconds", value]).is_ok());
    }
    assert!(crate::Cli::try_parse_from(["gpu-monitor", "--history-seconds", "61"]).is_err());
    assert!(crate::Cli::try_parse_from([
        "gpu-monitor",
        "record",
        "example.jsonl",
        "--duration",
        "0"
    ])
    .is_err());
    assert!(crate::Cli::try_parse_from(["gpu-monitor", "alerts", "--memory", "101"]).is_err());
    let parsed = crate::Cli::try_parse_from([
        "gpu-monitor",
        "replay",
        "example.jsonl",
        "--json",
        "--gpu",
        "uuid-0",
        "--gpu",
        "uuid-1",
        "--include-command",
    ])
    .unwrap();
    assert!(parsed.json && parsed.include_command);
    assert_eq!(parsed.gpus, ["uuid-0", "uuid-1"]);
}

#[test]
fn playback_respects_irregular_timestamps_and_speed_without_sleeping_in_the_feed() {
    use crate::feed::{Feed, Playback};
    use std::time::Duration;
    let mut playback = Playback::new(
        vec![
            snapshot(vec![gpu(0, 0, 1000)], 1000),
            snapshot(vec![gpu(0, 0, 3000)], 3000),
            snapshot(vec![gpu(0, 0, 11000)], 11000),
        ],
        2.0,
    )
    .unwrap();
    assert_eq!(playback.drain_due(Duration::ZERO).len(), 1);
    assert!(playback.drain_due(Duration::from_millis(999)).is_empty());
    assert_eq!(
        playback.drain_due(Duration::from_secs(1))[0].sampled_at_ms,
        3000
    );
    assert!(playback.drain_due(Duration::from_secs(4)).is_empty());
    assert_eq!(
        playback.drain_due(Duration::from_secs(5))[0].sampled_at_ms,
        11000
    );
    assert!(playback.is_finished());
    assert!(playback.label().contains("complete"));
    assert!(Playback::new(vec![], 1.0).is_err());
    let mut rolled_back =
        Playback::new(vec![snapshot(vec![], 2), snapshot(vec![], 1)], 1.0).unwrap();
    assert_eq!(rolled_back.drain_due(Duration::ZERO).len(), 2);
}

#[test]
fn time_buckets_cover_real_windows_retain_peaks_and_do_not_bridge_missing_samples() {
    use crate::app::HistoryPoint;
    use std::collections::VecDeque;
    let history = VecDeque::from([
        HistoryPoint {
            sampled_at_ms: 10000,
            gpu: Some(10),
            memory: None,
        },
        HistoryPoint {
            sampled_at_ms: 11000,
            gpu: Some(90),
            memory: None,
        },
        HistoryPoint {
            sampled_at_ms: 12000,
            gpu: None,
            memory: None,
        },
        HistoryPoint {
            sampled_at_ms: 13000,
            gpu: Some(20),
            memory: None,
        },
    ]);
    let buckets = ui::time_buckets(&history, 14000, 4000, 1000, 4, |point| point.gpu);
    assert_eq!(buckets, [Some(10), Some(90), None, Some(20)]);
    let wide = ui::time_buckets(&history, 14000, 4000, 1000, 8, |point| point.gpu);
    assert_eq!(
        wide,
        [
            Some(10),
            Some(10),
            Some(90),
            Some(90),
            None,
            None,
            Some(20),
            Some(20)
        ]
    );
    let compressed = ui::time_buckets(&history, 14000, 4000, 1000, 2, |point| point.gpu);
    assert_eq!(compressed, [Some(90), None]);
    let aged = ui::time_buckets(&history, 74000, 60000, 1000, 6, |point| point.gpu);
    assert_eq!(aged, [None; 6]);
}

#[test]
fn overview_navigation_diagnostics_and_events_do_not_destroy_device_state() {
    let mut app = App::new(1000);
    app.apply_snapshot(snapshot(vec![gpu(0, 20, 1000), gpu(1, 20, 1000)], 1000));
    let mut terminal = Terminal::new(TestBackend::new(110, 24)).unwrap();
    render(&mut app, &mut terminal);
    key(&mut app, KeyCode::End);
    let scroll = app.selected_view().unwrap().process_scroll;
    key(&mut app, KeyCode::Char('t'));
    let text = render(&mut app, &mut terminal);
    assert!(
        text.contains("All selected GPUs")
            && text.contains("Test GPU 0")
            && text.contains("Test GPU 1")
    );
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.selected_position(), 1);
    assert!(!app.overview);
    key(&mut app, KeyCode::Left);
    assert_eq!(app.selected_view().unwrap().process_scroll, scroll);
    key(&mut app, KeyCode::Char('d'));
    assert!(render(&mut app, &mut terminal).contains("UUID: uuid-0"));
    key(&mut app, KeyCode::Char('a'));
    assert!(render(&mut app, &mut terminal).contains("No alert events"));
    key(&mut app, KeyCode::Char('a'));
    assert_eq!(app.selected_view().unwrap().history.len(), 1);
}

#[test]
fn process_output_displays_owner_elapsed_and_retains_new_schema_fields() {
    let mut card = gpu(0, 1, 1000);
    card.processes[0].user = Some("alice".into());
    card.processes[0].elapsed_seconds = Some(90061);
    card.metrics.performance_state = Some("P2".into());
    card.metrics.throttle_reasons = Some(vec!["sw_power_cap".into()]);
    card.metrics.pcie_rx_kb_per_second = Some(512);
    let data = snapshot(vec![card], 1000);
    let text = crate::output::snapshot_text(&data, false, false);
    assert!(
        text.contains("alice")
            && text.contains("1d 01:01:01")
            && text.contains("P2")
            && text.contains("sw_power_cap")
            && text.contains("512 KB/s")
    );
    assert_eq!(
        serde_json::to_value(crate::process_snapshot_json(&data)).unwrap()["schema_version"],
        gpu_monitor_core::SCHEMA_VERSION
    );
    assert_eq!(
        crate::output::safe_text("worker\x1b[31m\nname"),
        "worker [31m name"
    );
}

#[test]
fn wall_clock_rollback_updates_live_state_and_replay_preserves_acquisition_order() {
    use crate::feed::Playback;
    use std::time::Duration;
    let mut app = App::new(1000);
    app.apply_snapshot(snapshot(vec![gpu(0, 0, 10000)], 10000));
    app.now_ms = 9000;
    assert!(!app.is_stale(app.selected_view().unwrap()));
    app.now_ms = 6000;
    assert!(app.is_stale(app.selected_view().unwrap()));
    let mut newer = gpu(0, 0, 9000);
    newer.metrics.gpu_utilization = Some(99);
    app.apply_snapshot(snapshot(vec![newer], 9000));
    assert_eq!(
        app.selected_view()
            .unwrap()
            .gpu
            .as_ref()
            .unwrap()
            .metrics
            .gpu_utilization,
        Some(99)
    );
    assert_eq!(app.selected_view().unwrap().history.len(), 1);
    assert_eq!(app.now_ms, 9000);
    let mut replay = Playback::new(
        vec![
            snapshot(vec![], 10000),
            snapshot(vec![], 9000),
            snapshot(vec![], 11000),
        ],
        2.0,
    )
    .unwrap();
    let initial = replay.drain_due(Duration::ZERO);
    assert_eq!(
        initial
            .iter()
            .map(|frame| frame.sampled_at_ms)
            .collect::<Vec<_>>(),
        [10000, 9000]
    );
    assert!(replay.drain_due(Duration::from_millis(999)).is_empty());
    assert_eq!(
        replay.drain_due(Duration::from_secs(1))[0].sampled_at_ms,
        11000
    );
}

#[test]
fn alert_defaults_and_scope_match_the_shared_runtime_contract() {
    use crate::{args::Commands, selection::Selection};
    use clap::Parser;
    use gpu_monitor_runtime::{AlertConfig, AlertEvent, AlertKind, AlertState};
    let parsed = crate::Cli::try_parse_from(["gpu-monitor", "alerts"]).unwrap();
    assert!(
        !parsed.alerts,
        "live alerts are opt-in outside the alerts subcommand"
    );
    let Commands::Alerts {
        temperature,
        temperature_recovery,
        memory,
        memory_recovery,
        duration_seconds,
        cooldown_seconds,
    } = parsed.command.unwrap()
    else {
        panic!("wrong command")
    };
    let default = AlertConfig::default();
    assert_eq!(
        (temperature, temperature_recovery, memory, memory_recovery),
        (
            default.temperature_threshold,
            default.temperature_recovery,
            default.memory_threshold,
            default.memory_recovery
        )
    );
    assert_eq!(duration_seconds * 1000.0, default.duration_ms as f64);
    assert_eq!(cooldown_seconds * 1000.0, default.cooldown_ms as f64);
    let selection = Selection {
        uuids: vec!["GPU-one".into()],
        ..Selection::default()
    };
    let mut event = AlertEvent {
        id: 1,
        at_ms: 1000,
        gpu_uuid: None,
        kind: AlertKind::DeviceUnavailable,
        state: AlertState::Firing,
        message: "missing".into(),
        value: None,
    };
    assert!(selection.matches_event(&event));
    for (uuid, visible) in [("GPU-one", true), ("GPU-two", false), ("index:2", true)] {
        event.gpu_uuid = Some(uuid.into());
        assert_eq!(selection.matches_event(&event), visible);
    }
}

#[test]
fn recording_strings_cannot_inject_terminal_controls_in_reports_or_tui() {
    let mut card = gpu(0, 0, 1000);
    card.device.name = "name\x1b[2J".into();
    card.device.uuid = "id\x1b[31m".into();
    card.device.driver_version = "driver\x1b[2J".into();
    card.device.pci_bus_id = "pci\x07".into();
    card.metrics.performance_state = Some("P2\x1b[2J".into());
    card.metrics.throttle_reasons = Some(vec!["cap\x1b[2J".into()]);
    card.issues=serde_json::from_value(json!([{"metric":"field\u{001b}[2J","error":{"kind":"unknown","message":"unsupported\u{0007}"}}])).unwrap();
    let snapshot = snapshot(vec![card], 1000);
    let text = crate::output::snapshot_text(&snapshot, false, false);
    let diagnostics = crate::output::diagnostics_text(&snapshot);
    for output in [&text, &diagnostics] {
        assert!(!output.contains('\x1b'));
        assert!(!output.contains('\x07'));
        assert!(output.contains('\n'));
    }
    let mut app = App::new(1000);
    app.apply_snapshot(snapshot);
    let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
    assert!(!render(&mut app, &mut terminal).contains('\x1b'));
    key(&mut app, KeyCode::Char('d'));
    let text = render(&mut app, &mut terminal);
    assert!(!text.contains('\x1b') && !text.contains('\x07'));
}
