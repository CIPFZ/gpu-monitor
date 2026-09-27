//! Behavioural tests for the interface.
//!
//! These drive the same path the binary does: a key becomes an action, the frame
//! is measured, and only then is it drawn. Assertions target behaviour that must
//! survive a redesign — independent scrolling, identity-stable history, filtered
//! results that cannot be mistaken for empty ones, and terminal-control safety.

use crate::{app::App, device::ProcessSort, keymap, render, view::View};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use gpu_monitor_core::{GpuInfo, MonitorSnapshot};
use ratatui::{backend::TestBackend, Terminal};
use serde_json::json;

/// Wide enough for the sidebar to be affordable.
const WIDE: (u16, u16) = (120, 30);
/// Realistic terminal where the sidebar must collapse.
const NARROW: (u16, u16) = (80, 24);

fn gpu(index: u32, processes: u32, timestamp: u64) -> GpuInfo {
    serde_json::from_value(json!({
        "device": {"index": index, "name": format!("Test GPU {index}"), "uuid": format!("uuid-{index}"),
                   "pci_bus_id": "0000:01:00.0", "driver_version": "test", "cuda_version": "12.4",
                   "power_limit": 250, "power_limit_max": 300},
        "metrics": {"gpu_utilization": 42, "memory_utilization": 15, "encoder_utilization": 0,
                    "decoder_utilization": 0, "temperature": 65, "power_usage": 80000,
                    "fan_speed": 30, "clock_graphics": 1500, "clock_memory": 7000, "clock_sm": 1500},
        "memory": {"total": 17179869184u64, "used": 8589934592u64, "free": 8589934592u64},
        "processes": (0..processes).map(|process| json!({
            "pid": index * 1000 + process,
            "name": format!("gpu{index}-process-{process:03}"),
            "gpu_memory": 1048576, "process_type": "Compute"
        })).collect::<Vec<_>>(),
        "sampled_at_ms": timestamp, "issues": []
    }))
    .unwrap()
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

fn terminal(size: (u16, u16)) -> Terminal<TestBackend> {
    Terminal::new(TestBackend::new(size.0, size.1)).unwrap()
}

/// Renders exactly as the binary does, and returns the screen with real rows.
fn screen(app: &mut App, terminal: &mut Terminal<TestBackend>) -> String {
    terminal
        .draw(|frame| {
            let area = frame.area();
            app.measure(area);
            render::draw(frame, app);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let width = buffer.area.width as usize;
    buffer
        .content()
        .chunks(width)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

fn press(app: &mut App, code: KeyCode) {
    let event = KeyEvent::new(code, KeyModifiers::NONE);
    if let Some(action) = keymap::resolve(event, app.is_editing_search()) {
        app.apply(action);
    }
}

fn type_text(app: &mut App, text: &str) {
    for character in text.chars() {
        press(app, KeyCode::Char(character));
    }
}

fn scroll_of(app: &App) -> usize {
    app.selected_view().unwrap().process_scroll
}

fn selected_gpu(app: &App) -> &GpuInfo {
    app.selected_view()
        .expect("a device is selected")
        .gpu
        .as_ref()
        .expect("the device has a sample")
}

#[test]
fn each_device_keeps_its_own_process_scroll_position() {
    let mut app = App::new(1000);
    app.apply_snapshot(snapshot(vec![gpu(0, 40, 1), gpu(1, 40, 1)], 1));
    let mut terminal = terminal(WIDE);
    press(&mut app, KeyCode::Char('2'));
    for index in 0..2 {
        let first = screen(&mut app, &mut terminal);
        assert!(first.contains(&format!("gpu{index}-process-000")));
        assert!(!first.contains(&format!("gpu{index}-process-039")));
        press(&mut app, KeyCode::End);
        assert!(screen(&mut app, &mut terminal).contains(&format!("gpu{index}-process-039")));
        assert!(scroll_of(&app) > 0);
        press(&mut app, KeyCode::Right);
    }
    // Returning to the first device restores the position it was left at.
    assert!(scroll_of(&app) > 0);
    press(&mut app, KeyCode::Right);
    assert!(scroll_of(&app) > 0);
}

#[test]
fn an_idle_device_does_not_limit_a_busy_one_and_resizing_clamps_the_offset() {
    let mut app = App::new(1000);
    app.apply_snapshot(snapshot(vec![gpu(0, 0, 1), gpu(1, 40, 1)], 1));
    let mut terminal = terminal(NARROW);
    press(&mut app, KeyCode::Char('2'));
    screen(&mut app, &mut terminal);
    press(&mut app, KeyCode::Right);
    screen(&mut app, &mut terminal);
    press(&mut app, KeyCode::End);
    assert!(screen(&mut app, &mut terminal).contains("gpu1-process-039"));
    let tall_offset = scroll_of(&app);
    terminal.backend_mut().resize(80, 18);
    screen(&mut app, &mut terminal);
    press(&mut app, KeyCode::End);
    assert!(
        scroll_of(&app) > tall_offset,
        "a shorter pane scrolls further"
    );
    assert!(screen(&mut app, &mut terminal).contains("gpu1-process-039"));
    terminal.backend_mut().resize(80, 60);
    assert!(screen(&mut app, &mut terminal).contains("gpu1-process-039"));
    assert_eq!(scroll_of(&app), 0, "a taller pane needs no offset at all");
}

#[test]
fn scrolling_uses_the_measured_page_height_not_a_fixed_guess() {
    let mut app = App::new(1000);
    app.apply_snapshot(snapshot(vec![gpu(0, 80, 1)], 1));
    let mut terminal = terminal(NARROW);
    press(&mut app, KeyCode::Char('2'));
    screen(&mut app, &mut terminal);
    press(&mut app, KeyCode::PageDown);
    let page = scroll_of(&app);
    assert_eq!(page, app.process_rows());
    assert!(page > 0 && page < 80);
    press(&mut app, KeyCode::Down);
    assert_eq!(scroll_of(&app), page + 1);
    press(&mut app, KeyCode::PageUp);
    assert_eq!(scroll_of(&app), 1);
    press(&mut app, KeyCode::Home);
    assert_eq!(scroll_of(&app), 0);
}

#[test]
fn the_sidebar_shows_every_device_at_once_and_collapses_when_unaffordable() {
    let mut app = App::new(1000);
    app.apply_snapshot(snapshot((0..4).map(|index| gpu(index, 2, 1)).collect(), 1));
    let mut wide = terminal(WIDE);
    let rendered = screen(&mut app, &mut wide);
    assert!(app.sidebar_visible());
    for index in 0..4 {
        assert!(
            rendered.contains(&format!("Test GPU {index}")),
            "device {index} must be visible without navigating"
        );
    }
    assert!(rendered.contains("4 devices"));

    let mut narrow = terminal(NARROW);
    let rendered = screen(&mut app, &mut narrow);
    assert!(
        !app.sidebar_visible(),
        "a narrow pane keeps its width for detail"
    );
    assert!(
        rendered.contains("Test GPU 0"),
        "the selected device still has a title"
    );
    assert!(rendered.contains("b shows the device sidebar"));

    press(&mut app, KeyCode::Char('b'));
    assert!(!app.sidebar_visible());
    assert!(!screen(&mut app, &mut wide).contains("Test GPU 3"));
}

#[test]
fn the_dashboard_reports_each_reading_with_its_unit() {
    let mut app = App::new(1000);
    app.apply_snapshot(snapshot(vec![gpu(0, 3, 1)], 1));
    let mut terminal = terminal(WIDE);
    let rendered = screen(&mut app, &mut terminal);
    assert!(rendered.contains("GPU 0 · Test GPU 0"));
    assert!(rendered.contains("driver test · CUDA 12.4"));
    assert!(rendered.contains("42%"), "load");
    assert!(rendered.contains("8.0/16.0 GiB"), "memory capacity");
    assert!(rendered.contains("65°C"), "temperature");
    assert!(rendered.contains("80/250 W"), "power against its limit");
    assert!(rendered.contains("1500 MHz"), "clocks");
    assert!(rendered.contains("Processes · top 3 of 3"));
}

#[test]
fn an_unavailable_reading_is_never_drawn_as_a_zero() {
    let mut app = App::new(1000);
    let mut blind = gpu(0, 0, 1);
    blind.metrics.gpu_utilization = None;
    blind.metrics.temperature = None;
    blind.metrics.fan_speed = None;
    blind.memory = None;
    app.apply_snapshot(snapshot(vec![blind], 1));
    let mut terminal = terminal(WIDE);
    let rendered = screen(&mut app, &mut terminal);
    assert!(rendered.contains("N/A"));
    assert!(
        rendered.contains('·'),
        "an unmeasured meter uses its own fill, not an empty bar"
    );
    press(&mut app, KeyCode::Char('3'));
    assert!(
        screen(&mut app, &mut terminal).contains('×'),
        "a missing acquisition is a gap in the chart"
    );
}

#[test]
fn history_follows_a_device_through_reordering_failure_and_recovery() {
    let mut app = App::new(1000);
    app.apply_snapshot(snapshot(vec![gpu(0, 40, 1), gpu(1, 40, 1)], 1));
    let mut terminal = terminal(WIDE);
    press(&mut app, KeyCode::Char('2'));
    screen(&mut app, &mut terminal);
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::End);
    let offset = scroll_of(&app);
    assert!(offset > 0);

    // The devices swap indices; identity is the UUID, not the position.
    let mut selected = gpu(1, 40, 2);
    selected.device.index = 0;
    let mut other = gpu(0, 40, 2);
    other.device.index = 1;
    app.apply_snapshot(snapshot(vec![selected, other], 2));
    assert_eq!(selected_gpu(&app).device.uuid, "uuid-1");
    assert_eq!(scroll_of(&app), offset);

    let failure: MonitorSnapshot = serde_json::from_value(json!({
        "sampled_at_ms": 3, "gpus": [],
        "failures": [{"index": 0, "uuid": "uuid-1",
                      "error": {"kind": "device_lost", "message": "device lost"}}],
        "error": null
    }))
    .unwrap();
    app.apply_snapshot(failure);
    let view = app.selected_view().unwrap();
    assert_eq!(
        view.history.len(),
        3,
        "the failure occupies its place in time"
    );
    assert_eq!(view.history.back().unwrap().gpu, None);
    assert!(screen(&mut app, &mut terminal).contains("Device unavailable: device lost"));

    app.apply_snapshot(snapshot(vec![gpu(1, 40, 4)], 4));
    let view = app.selected_view().unwrap();
    assert_eq!(view.history.len(), 4);
    assert!(view.error.is_none());
    assert_eq!(view.history.back().unwrap().gpu, Some(42));
}

#[test]
fn repeated_deliveries_are_not_new_samples_and_history_stays_bounded() {
    let mut app = App::new(1000);
    for second in 1..=65 {
        let timestamp = second * 1000;
        app.apply_snapshot(snapshot(vec![gpu(0, 0, timestamp)], timestamp));
    }
    assert_eq!(app.selected_view().unwrap().history.len(), 65);
    let mut missing = gpu(0, 0, 66_000);
    missing.metrics.gpu_utilization = None;
    missing.memory = None;
    app.apply_snapshot(snapshot(vec![missing.clone()], 66_000));
    app.apply_snapshot(snapshot(vec![missing], 66_000));
    let view = app.selected_view().unwrap();
    assert_eq!(
        view.history.len(),
        66,
        "the same acquisition is recorded once"
    );
    assert_eq!(view.history.back().unwrap().gpu, None);
    assert_eq!(view.history.back().unwrap().memory, None);
}

#[test]
fn switching_views_never_disturbs_device_state() {
    let mut app = App::new(1000);
    app.apply_snapshot(snapshot(vec![gpu(0, 40, 1000), gpu(1, 40, 1000)], 1000));
    let mut terminal = terminal(WIDE);
    press(&mut app, KeyCode::Char('2'));
    screen(&mut app, &mut terminal);
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::End);
    let offset = scroll_of(&app);
    let position = app.selected_position();

    for view in View::ALL {
        press(&mut app, KeyCode::Char(view.hotkey()));
        assert_eq!(app.view, view);
        let rendered = screen(&mut app, &mut terminal);
        assert!(rendered.contains(view.title()));
    }
    press(&mut app, KeyCode::Char('4'));
    assert!(screen(&mut app, &mut terminal).contains("uuid-1"));
    press(&mut app, KeyCode::Char('5'));
    assert!(screen(&mut app, &mut terminal).contains("No alert events"));

    press(&mut app, KeyCode::Char('2'));
    screen(&mut app, &mut terminal);
    assert_eq!(app.selected_position(), position);
    assert_eq!(scroll_of(&app), offset);
    assert_eq!(app.selected_view().unwrap().history.len(), 1);
}

#[test]
fn tab_cycles_views_and_a_text_pane_scrolls_by_line() {
    let mut app = App::new(1000);
    app.apply_snapshot(snapshot((0..6).map(|index| gpu(index, 1, 1)).collect(), 1));
    let mut terminal = terminal((100, 14));
    for expected in [View::Processes, View::History, View::Diagnostics] {
        press(&mut app, KeyCode::Tab);
        assert_eq!(app.view, expected);
    }
    screen(&mut app, &mut terminal);
    assert_eq!(app.panel_scroll(), 0);
    press(&mut app, KeyCode::Down);
    screen(&mut app, &mut terminal);
    assert_eq!(app.panel_scroll(), 1);
    press(&mut app, KeyCode::End);
    let bottom = app.panel_scroll();
    assert!(bottom > 1, "six devices produce a scrollable report");
    press(&mut app, KeyCode::Down);
    assert_eq!(
        app.panel_scroll(),
        bottom,
        "a pane cannot scroll past its end"
    );
    press(&mut app, KeyCode::Home);
    assert_eq!(app.panel_scroll(), 0);
}

#[test]
fn filtering_processes_is_reversible_and_never_looks_like_an_empty_device() {
    let mut app = App::new(1000);
    app.apply_snapshot(snapshot(vec![gpu(0, 40, 1)], 1));
    let mut terminal = terminal(WIDE);

    press(&mut app, KeyCode::Char('/'));
    assert!(app.is_editing_search());
    assert_eq!(
        app.view,
        View::Processes,
        "the filter applies to a visible list"
    );
    type_text(&mut app, "007");
    assert_eq!(app.search_text(), "007");
    assert!(screen(&mut app, &mut terminal).contains("Enter apply"));
    press(&mut app, KeyCode::Enter);
    assert!(!app.is_editing_search());
    let rendered = screen(&mut app, &mut terminal);
    assert!(rendered.contains("gpu0-process-007"));
    assert!(!rendered.contains("gpu0-process-000"));
    assert!(rendered.contains("filtered from 40"));

    // An abandoned edit restores the filter that was already applied.
    press(&mut app, KeyCode::Char('/'));
    type_text(&mut app, "zzz");
    press(&mut app, KeyCode::Esc);
    assert!(!app.is_editing_search());
    assert_eq!(app.filter.query, "007");

    press(&mut app, KeyCode::Char('/'));
    type_text(&mut app, "no-such-process");
    press(&mut app, KeyCode::Enter);
    let rendered = screen(&mut app, &mut terminal);
    assert!(
        rendered.contains("No process matches") && rendered.contains("40 sampled"),
        "a filtered result must not read as a device with no work"
    );
}

#[test]
fn the_process_sort_order_is_visible_and_cycles_through_every_column() {
    let mut app = App::new(1000);
    let mut device = gpu(0, 3, 1);
    device.processes[0].gpu_memory = Some(1 << 20);
    device.processes[1].gpu_memory = Some(9 << 20);
    device.processes[2].gpu_memory = None;
    app.apply_snapshot(snapshot(vec![device], 1));
    let mut terminal = terminal(WIDE);
    press(&mut app, KeyCode::Char('2'));

    assert_eq!(app.filter.sort, ProcessSort::Memory);
    let rendered = screen(&mut app, &mut terminal);
    assert!(rendered.contains("GPU memory ▼"));
    let ordered = app
        .selected_view()
        .unwrap()
        .processes(&app.filter)
        .iter()
        .map(|process| process.pid)
        .collect::<Vec<_>>();
    assert_eq!(ordered, [1, 0, 2], "unknown allocations sort last");

    press(&mut app, KeyCode::Char('s'));
    assert_eq!(app.filter.sort, ProcessSort::Pid);
    assert!(screen(&mut app, &mut terminal).contains("PID ▼"));
    for _ in 0..ProcessSort::ALL.len() - 1 {
        press(&mut app, KeyCode::Char('s'));
    }
    assert_eq!(app.filter.sort, ProcessSort::Memory, "the order cycles");
}

#[test]
fn the_chart_window_can_be_changed_without_restarting() {
    let mut app = App::new(1000);
    app.apply_snapshot(snapshot(vec![gpu(0, 0, 1000)], 1000));
    let mut terminal = terminal(WIDE);
    press(&mut app, KeyCode::Char('3'));
    assert_eq!(app.history_window_ms, keymap::HISTORY_WINDOWS_MS[0]);
    assert!(screen(&mut app, &mut terminal).contains("1m window"));
    for expected in [
        keymap::HISTORY_WINDOWS_MS[1],
        keymap::HISTORY_WINDOWS_MS[2],
        keymap::HISTORY_WINDOWS_MS[0],
    ] {
        press(&mut app, KeyCode::Char('w'));
        assert_eq!(app.history_window_ms, expected);
    }
    press(&mut app, KeyCode::Char('w'));
    assert!(screen(&mut app, &mut terminal).contains("5m window"));
}

#[test]
fn the_history_view_charts_every_metric_the_runtime_records() {
    let mut app = App::new(1000);
    app.apply_snapshot(snapshot(vec![gpu(0, 0, 1000)], 1000));
    let mut terminal = terminal(WIDE);
    press(&mut app, KeyCode::Char('3'));
    let rendered = screen(&mut app, &mut terminal);
    for series in ["GPU load", "Memory used", "Temperature", "Power"] {
        assert!(
            rendered.contains(series),
            "{series} is recorded but not charted"
        );
    }
    assert!(rendered.contains("now 42%"));
    assert!(rendered.contains("now 65°C"));
    assert!(rendered.contains("now 80 W"));
    assert!(
        rendered.contains("scale 250 W"),
        "power is charted against the device limit, not the window peak"
    );
    assert!(rendered.contains("scale 100%"));
}

#[test]
fn a_young_session_is_not_drawn_as_a_wall_of_missing_data() {
    let mut app = App::new(1000);
    // One sample inside an hour-long window leaves most of the window uncovered.
    app.apply_snapshot(snapshot(vec![gpu(0, 0, 3_600_000)], 3_600_000));
    let mut terminal = terminal(WIDE);
    press(&mut app, KeyCode::Char('3'));
    press(&mut app, KeyCode::Char('w'));
    press(&mut app, KeyCode::Char('w'));
    assert_eq!(app.history_window_ms, 3_600_000);
    let rendered = screen(&mut app, &mut terminal);
    let gaps = rendered
        .chars()
        .filter(|character| *character == '×')
        .count();
    assert!(
        gaps <= 8,
        "time before the first sample must stay blank, found {gaps} gap markers"
    );
    assert!(rendered.contains("GPU load"));
}

#[test]
fn help_lists_the_real_bindings_and_any_key_dismisses_it() {
    let mut app = App::new(1000);
    app.apply_snapshot(snapshot(vec![gpu(0, 2, 1)], 1));
    let mut terminal = terminal(WIDE);
    press(&mut app, KeyCode::Char('?'));
    assert!(app.help_open);
    let rendered = screen(&mut app, &mut terminal);
    assert!(rendered.contains("Previous device"));
    // The panel is sized from its own declarations, so no description is clipped.
    for binding in keymap::BINDINGS {
        assert!(
            rendered.contains(binding.description),
            "\"{}\" is clipped out of the help panel",
            binding.description
        );
    }
    press(&mut app, KeyCode::Down);
    assert!(!app.help_open, "help can never trap the interface");
    press(&mut app, KeyCode::Char('?'));
    press(&mut app, KeyCode::Char('?'));
    assert!(!app.help_open);
}

#[test]
fn a_terminal_that_is_too_small_asks_for_the_size_it_actually_needs() {
    let mut app = App::new(1000);
    app.apply_snapshot(snapshot(vec![gpu(0, 40, 1)], 1));
    let mut terminal = terminal(NARROW);
    press(&mut app, KeyCode::Char('2'));
    screen(&mut app, &mut terminal);
    press(&mut app, KeyCode::End);
    let offset = scroll_of(&app);
    assert!(offset > 0);

    terminal.backend_mut().resize(30, 8);
    let rendered = screen(&mut app, &mut terminal);
    assert!(rendered.contains(&format!(
        "{}×{}",
        crate::layout::MIN_WIDTH,
        crate::layout::MIN_HEIGHT
    )));
    press(&mut app, KeyCode::Down);
    assert_eq!(
        scroll_of(&app),
        offset,
        "an unrenderable pane does not scroll"
    );

    terminal.backend_mut().resize(NARROW.0, NARROW.1);
    press(&mut app, KeyCode::End);
    assert!(screen(&mut app, &mut terminal).contains("gpu0-process-039"));
}

#[test]
fn an_empty_selection_explains_itself_instead_of_showing_a_blank_pane() {
    use crate::selection::Selection;
    let mut app = App::new(1000).with_options(
        Selection {
            uuids: vec!["uuid-absent".into()],
            ..Selection::default()
        },
        60_000,
    );
    app.apply_snapshot(snapshot(vec![gpu(0, 2, 1)], 1));
    let mut terminal = terminal(WIDE);
    let rendered = screen(&mut app, &mut terminal);
    assert_eq!(app.device_count(), 0);
    assert!(rendered.contains("No device matches the current filters"));
    assert!(rendered.contains("none detected"));
}

#[test]
fn recorded_strings_cannot_inject_terminal_controls_anywhere() {
    let mut device = gpu(0, 1, 1000);
    device.device.name = "name\x1b[2J".into();
    device.device.uuid = "id\x1b[31m".into();
    device.device.driver_version = "driver\x1b[2J".into();
    device.device.pci_bus_id = "pci\x07".into();
    device.metrics.performance_state = Some("P2\x1b[2J".into());
    device.metrics.throttle_reasons = Some(vec!["cap\x1b[2J".into()]);
    device.processes[0].name = "job\x1b[2J".into();
    device.issues = serde_json::from_value(json!([
        {"metric": "field\u{001b}[2J", "error": {"kind": "unknown", "message": "unsupported\u{0007}"}}
    ]))
    .unwrap();
    let data = snapshot(vec![device], 1000);

    for report in [
        crate::output::snapshot_text(&data, false, false),
        crate::output::diagnostics_text(&data),
    ] {
        assert!(!report.contains('\x1b'));
        assert!(!report.contains('\x07'));
        assert!(report.contains('\n'));
    }

    let mut app = App::new(1000);
    app.apply_snapshot(data);
    let mut terminal = terminal(WIDE);
    for view in View::ALL {
        press(&mut app, KeyCode::Char(view.hotkey()));
        let rendered = screen(&mut app, &mut terminal);
        assert!(!rendered.contains('\x1b'), "{view:?} leaked an escape");
        assert!(!rendered.contains('\x07'), "{view:?} leaked a bell");
    }
}

#[test]
fn a_backward_clock_step_updates_state_without_joining_across_the_gap() {
    let mut app = App::new(1000);
    app.apply_snapshot(snapshot(vec![gpu(0, 0, 10_000)], 10_000));
    app.now_ms = 9_000;
    assert!(!app.is_stale(app.selected_view().unwrap()));
    app.now_ms = 6_000;
    assert!(app.is_stale(app.selected_view().unwrap()));
    let mut newer = gpu(0, 0, 9_000);
    newer.metrics.gpu_utilization = Some(99);
    app.apply_snapshot(snapshot(vec![newer], 9_000));
    assert_eq!(selected_gpu(&app).metrics.gpu_utilization, Some(99));
    assert_eq!(app.selected_view().unwrap().history.len(), 1);
    assert_eq!(app.now_ms, 9_000);
}

#[test]
fn playback_respects_irregular_timestamps_and_speed_without_sleeping() {
    use crate::feed::{Feed, Playback};
    use std::time::Duration;
    let mut playback = Playback::new(
        vec![
            snapshot(vec![gpu(0, 0, 1000)], 1000),
            snapshot(vec![gpu(0, 0, 3000)], 3000),
            snapshot(vec![gpu(0, 0, 11_000)], 11_000),
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
        11_000
    );
    assert!(playback.is_finished());
    assert!(playback.label().contains("complete"));
    assert!(Playback::new(vec![], 1.0).is_err());
    let mut rolled_back =
        Playback::new(vec![snapshot(vec![], 2), snapshot(vec![], 1)], 1.0).unwrap();
    assert_eq!(
        rolled_back.drain_due(Duration::ZERO).len(),
        2,
        "recorded order survives a backward clock step"
    );
}

#[test]
fn selection_combines_uuid_capacity_owner_and_command_privacy() {
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
fn an_unknown_capacity_is_not_treated_as_zero_free_memory() {
    use crate::selection::Selection;
    let mut missing = gpu(0, 0, 1000);
    missing.memory = None;
    let mut original = snapshot(vec![missing, gpu(1, 0, 1000)], 1000);
    original.failures = serde_json::from_value(json!([
        {"index": 2, "uuid": null, "error": {"kind": "device_lost", "message": "identity unavailable"}}
    ]))
    .unwrap();
    let selected = Selection {
        min_free_gib: Some(0.0),
        ..Selection::default()
    }
    .apply(&original);
    assert_eq!(selected.gpus.len(), 1);
    assert_eq!(selected.gpus[0].device.index, 1);
    assert_eq!(
        selected.failures.len(),
        1,
        "an unattributed failure stays visible"
    );
    assert!(crate::output::selection_result(&original, &selected, &Selection::default()).is_ok());
    let absent = Selection {
        uuids: vec!["missing".into()],
        ..Selection::default()
    };
    assert!(absent.apply(&original).gpus.is_empty());
    assert!(crate::output::selection_result(&original, &absent.apply(&original), &absent).is_err());
}

#[test]
fn numeric_sorts_descend_and_place_unknown_readings_last() {
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
            [1, 0, 2],
            "{sort:?}"
        );
    }
    assert_eq!(
        Selection::default()
            .apply(&original)
            .gpus
            .iter()
            .map(|gpu| gpu.device.index)
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
}

#[test]
fn the_process_json_contract_is_unchanged() {
    let mut device = gpu(1, 1, 10);
    device.processes[0].gpu_memory = None;
    let mut data = snapshot(vec![device], 11);
    data.failures = serde_json::from_value(json!([
        {"index": 0, "uuid": null, "error": {"kind": "device_lost", "message": "lost"}}
    ]))
    .unwrap();
    let output = serde_json::to_value(crate::output::process_snapshot_json(&data)).unwrap();
    assert_eq!(output["schema_version"], gpu_monitor_core::SCHEMA_VERSION);
    assert_eq!(output["sampled_at_ms"], 11);
    assert_eq!(output["gpus"][0]["sampled_at_ms"], 10);
    assert_eq!(output["gpus"][0]["device"]["uuid"], "uuid-1");
    assert!(output["gpus"][0]["processes"][0]["gpu_memory_mib"].is_null());
    assert_eq!(output["failures"][0]["error"]["kind"], "device_lost");
    assert!(crate::output::snapshot_result(&data).is_ok());
    data.gpus.clear();
    assert!(crate::output::snapshot_result(&data).is_err());
}

#[test]
fn reports_keep_owner_elapsed_time_and_the_extended_metrics() {
    let mut device = gpu(0, 1, 1000);
    device.processes[0].user = Some("alice".into());
    device.processes[0].elapsed_seconds = Some(90_061);
    device.metrics.performance_state = Some("P2".into());
    device.metrics.throttle_reasons = Some(vec!["sw_power_cap".into()]);
    device.metrics.pcie_rx_kb_per_second = Some(512);
    let report = crate::output::snapshot_text(&snapshot(vec![device], 1000), false, false);
    for expected in ["alice", "1d 01:01:01", "P2", "sw_power_cap", "512 KB/s"] {
        assert!(
            report.contains(expected),
            "{expected} missing from the report"
        );
    }
}

#[test]
fn alert_events_are_reported_in_a_readable_and_scriptable_form() {
    use gpu_monitor_runtime::{AlertEvent, AlertKind, AlertState};
    let event = AlertEvent {
        id: 1,
        at_ms: 1_000_000,
        gpu_uuid: Some("uuid-0".into()),
        kind: AlertKind::Temperature,
        state: AlertState::Firing,
        message: "88C for 10s".into(),
        value: Some(88.0),
    };
    let report = crate::output::alert_report_line(&event);
    assert!(report.starts_with("1000000 "));
    assert!(report.contains("firing") && report.contains("temperature"));
    assert!(report.contains("uuid-0") && report.contains("88C for 10s"));
    assert!(
        !report.contains("Firing"),
        "the text form uses stable lowercase names, not debug output"
    );
    let panel = crate::output::alert_panel_line(&event, 1_090_000);
    assert!(panel.contains("1m ago"));
    assert!(crate::output::alert_panel_line(&event, 1_000_500).contains("just now"));
    assert!(
        crate::output::alert_panel_line(&event, 500).contains("just now"),
        "a backward clock step must not underflow"
    );
}

#[test]
fn alert_scope_follows_the_device_selection() {
    use crate::{args::Cli, selection::Selection};
    use clap::Parser;
    use gpu_monitor_runtime::{AlertEvent, AlertKind, AlertState};
    assert!(
        !Cli::try_parse_from(["gpu-monitor", "alerts"])
            .unwrap()
            .alerts,
        "live alerts stay opt-in outside the alerts command"
    );
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
        assert_eq!(selection.matches_event(&event), visible, "{uuid}");
    }
}
