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
        app.apply_snapshot(snapshot(vec![gpu(0, 0, timestamp)], timestamp));
    }
    assert_eq!(app.selected_view().unwrap().history.len(), 60);
    assert_eq!(
        app.selected_view()
            .unwrap()
            .history
            .front()
            .unwrap()
            .sampled_at_ms,
        6
    );
    let mut missing = gpu(0, 0, 66);
    missing.metrics.gpu_utilization = None;
    missing.metrics.temperature = None;
    missing.memory = None;
    app.apply_snapshot(snapshot(vec![missing.clone()], 66));
    app.apply_snapshot(snapshot(vec![missing], 66));
    let view = app.selected_view().unwrap();
    assert_eq!(view.history.len(), 60);
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
