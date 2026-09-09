//! One selected GPU per page, with layout-derived process scrolling.

use gpu_monitor_core::GpuInfo;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph, Row, Sparkline, Table, TableState, Wrap},
    Frame,
};

use crate::app::{App, DeviceView, HistoryPoint};
use crate::output::{diagnostics_text, elapsed, process_owner, safe_lines, safe_text};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(frame.area());
    let header = format!(
        " GPU Monitor | {} | Device {}/{} | {} failed | {}s history",
        app.source_label,
        if app.device_count() == 0 {
            0
        } else {
            app.selected_position() + 1
        },
        app.device_count(),
        app.failure_count,
        app.history_window_ms / 1000
    );
    frame.render_widget(
        Paragraph::new(header).style(Style::default().fg(Color::Cyan)),
        chunks[0],
    );
    frame.render_widget(
        Paragraph::new(
            "←/→ GPU | ↑/↓ scroll | t overview | d diagnostics | a alerts | r retry | q quit",
        ),
        chunks[2],
    );

    if app.diagnostics || app.show_events {
        let text = if app.diagnostics {
            app.latest_snapshot
                .as_ref()
                .map(diagnostics_text)
                .unwrap_or_else(|| {
                    app.error
                        .clone()
                        .unwrap_or_else(|| "Waiting for the first sample".into())
                })
        } else if app.events.is_empty() {
            "No alert events. Enable monitoring with --alerts or the alerts command.
Alerts use sustained thresholds and separate recovery limits."
                .into()
        } else {
            app.events
                .iter()
                .rev()
                .map(|event| {
                    format!(
                        "{} {:?} {:?} {}: {}",
                        event.at_ms,
                        event.kind,
                        event.state,
                        event.gpu_uuid.as_deref().unwrap_or("monitor"),
                        safe_text(&event.message)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        let block = Block::bordered().title(if app.diagnostics {
            "Diagnostics · d to return"
        } else {
            "Alert events · a to return"
        });
        let inner = block.inner(chunks[1]);
        let lines = text
            .lines()
            .map(|line| {
                line.chars()
                    .count()
                    .max(1)
                    .div_ceil(inner.width.max(1) as usize)
            })
            .sum();
        app.set_panel_viewport(lines, inner.height);
        frame.render_widget(
            Paragraph::new(safe_lines(&text))
                .block(block)
                .wrap(Wrap { trim: false })
                .scroll((app.panel_scroll, 0)),
            chunks[1],
        );
        app.set_process_viewport(0);
        return;
    }
    if app.overview {
        draw_overview(frame, chunks[1], app);
        app.set_process_viewport(0);
        return;
    }

    let Some(view) = app.selected_view() else {
        let message = app
            .error
            .as_deref()
            .unwrap_or(if app.selection.has_device_filter() {
                "No GPUs match the current filters. Monitoring continues."
            } else {
                "No GPU devices detected. Retrying automatically; r retries now."
            });
        frame.render_widget(
            Paragraph::new(safe_text(message))
                .wrap(Wrap { trim: true })
                .style(Style::default().fg(Color::Yellow))
                .block(Block::bordered().title("GPU Monitor")),
            chunks[1],
        );
        app.set_process_viewport(0);
        return;
    };
    let title = view.gpu.as_ref().map_or_else(
        || format!(" GPU {} ", view.index),
        |gpu| {
            format!(
                " GPU {}: {} ",
                gpu.device.index,
                safe_text(&gpu.device.name)
            )
        },
    );
    let block = Block::bordered()
        .title(title)
        .border_style(Style::default().fg(Color::Blue));
    let inner = block.inner(chunks[1]);
    frame.render_widget(block, chunks[1]);
    if inner.height < 10 || inner.width < 40 {
        frame.render_widget(
            Paragraph::new("Terminal too small. Resize to at least 42 columns × 14 rows.")
                .wrap(Wrap { trim: true })
                .style(Style::default().fg(Color::Yellow)),
            inner,
        );
        app.set_process_viewport(0);
        return;
    }
    // Shrink history before sacrificing the process table on shorter terminals.
    let chart_height = (inner.height.saturating_sub(9) / 2).clamp(1, 3);
    let sections = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(chart_height),
        Constraint::Length(chart_height),
        Constraint::Length(2),
        Constraint::Min(0),
    ])
    .split(inner);
    let process_block = Block::default().borders(Borders::TOP);
    // The block's top border and the table's one-line header consume real rows.
    let visible_rows = process_block.inner(sections[4]).height.saturating_sub(1) as usize;
    app.set_process_viewport(visible_rows);
    let view = app
        .selected_view()
        .expect("selection is unchanged by viewport updates");

    if let Some(gpu) = &view.gpu {
        draw_metrics(frame, sections[0], gpu);
    } else {
        frame.render_widget(
            Paragraph::new("GPU metrics: N/A\nMemory: N/A\nNo successful sample yet."),
            sections[0],
        );
    }
    let gpu_value = view
        .gpu
        .as_ref()
        .and_then(|gpu| gpu.metrics.gpu_utilization);
    let memory_title = view
        .gpu
        .as_ref()
        .map_or_else(|| "Memory capacity: N/A".into(), memory_label);
    draw_history(
        frame,
        sections[1],
        format!(
            "GPU load: {} | {}s; × = missing",
            value(gpu_value, "%"),
            app.history_window_ms / 1000
        ),
        &view.history,
        app.now_ms,
        app.history_window_ms,
        app.sample_interval_ms,
        |point| point.gpu,
        Color::Green,
    );
    draw_history(
        frame,
        sections[2],
        memory_title,
        &view.history,
        app.now_ms,
        app.history_window_ms,
        app.sample_interval_ms,
        |point| point.memory,
        Color::Cyan,
    );

    let stale = app.is_stale(view);
    let status = if let Some(error) = &view.error {
        format!("STALE / unavailable: {error}. Retrying automatically; r retries now.")
    } else if stale {
        format!(
            "STALE: last successful sample {} ms. {}",
            view.gpu.as_ref().map_or(0, |gpu| gpu.sampled_at_ms),
            app.error
                .as_deref()
                .unwrap_or("Waiting for the sampler; r retries now.")
        )
    } else if let Some(gpu) = &view.gpu {
        if let Some(issue) = gpu.issues.first() {
            format!(
                "{} unavailable metric(s): {}: {}",
                gpu.issues.len(),
                issue.metric,
                issue.error.message
            )
        } else {
            "Live | Time buckets retain peaks; × marks missing data. d diagnostics · a alerts"
                .into()
        }
    } else {
        "Waiting for a successful sample.".into()
    };
    frame.render_widget(
        Paragraph::new(safe_lines(&status))
            .wrap(Wrap { trim: true })
            .style(Style::default().fg(
                if stale || view.gpu.as_ref().is_some_and(|gpu| !gpu.issues.is_empty()) {
                    Color::Yellow
                } else {
                    Color::DarkGray
                },
            )),
        sections[3],
    );
    draw_processes(
        frame,
        sections[4],
        view,
        visible_rows,
        stale,
        app.selection.include_command,
    );
}

pub fn value<T: std::fmt::Display>(value: Option<T>, suffix: &str) -> String {
    value.map_or_else(|| "N/A".into(), |value| format!("{value}{suffix}"))
}

pub fn memory_label(gpu: &GpuInfo) -> String {
    gpu.memory.as_ref().map_or_else(
        || "Memory capacity: N/A".into(),
        |memory| {
            format!(
                "Memory capacity: {:.1}/{:.1} GiB ({:.0}%)",
                memory.used_gib(),
                memory.total_gib(),
                memory.usage_percent()
            )
        },
    )
}

fn draw_metrics(frame: &mut Frame, area: Rect, gpu: &GpuInfo) {
    let metrics = &gpu.metrics;
    let power = metrics.power_watts().map(|power| format!("{power:.1}"));
    let lines = vec![
        Line::from(format!(
            "Temp: {}  Power: {}/{}  Fan: {}",
            value(metrics.temperature, "°C"),
            value(power, "W"),
            value(gpu.device.power_limit, "W"),
            value(metrics.fan_speed, "%")
        )),
        Line::from(format!(
            "Clocks: graphics {}  SM {}  memory {}",
            value(metrics.clock_graphics, "MHz"),
            value(metrics.clock_sm, "MHz"),
            value(metrics.clock_memory, "MHz")
        )),
        Line::from(format!(
            "Memory I/O: {}  Encoder: {}  Decoder: {}",
            value(metrics.memory_utilization, "%"),
            value(metrics.encoder_utilization, "%"),
            value(metrics.decoder_utilization, "%")
        )),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_history(
    frame: &mut Frame,
    area: Rect,
    title: String,
    history: &std::collections::VecDeque<HistoryPoint>,
    now_ms: u64,
    window_ms: u64,
    sample_interval_ms: u64,
    metric: fn(&HistoryPoint) -> Option<u64>,
    color: Color,
) {
    let block = Block::default().title(title);
    let width = block.inner(area).width as usize;
    let samples = time_buckets(
        history,
        now_ms,
        window_ms,
        sample_interval_ms,
        width,
        metric,
    );
    let chart = Sparkline::default()
        .block(block)
        .data(samples)
        .max(100)
        .absent_value_symbol("×")
        .absent_value_style(Style::default().fg(Color::DarkGray))
        .style(Style::default().fg(color));
    frame.render_widget(chart, area);
}

fn draw_processes(
    frame: &mut Frame,
    area: Rect,
    view: &DeviceView,
    visible_rows: usize,
    stale: bool,
    include_command: bool,
) {
    let count = view.process_count();
    let first = if count == 0 || visible_rows == 0 {
        0
    } else {
        view.process_scroll + 1
    };
    let last = (view.process_scroll + visible_rows).min(count);
    let unavailable = view.gpu.as_ref().is_some_and(|gpu| {
        gpu.issues
            .iter()
            .any(|issue| issue.metric.contains("process"))
    });
    let label = if stale {
        " STALE"
    } else if unavailable {
        " incomplete/N/A"
    } else {
        ""
    };
    let block = Block::default()
        .borders(Borders::TOP)
        .title(format!("Processes{label} ({first}-{last}/{count})"));
    let rows: Vec<Row> = view
        .gpu
        .as_ref()
        .into_iter()
        .flat_map(|gpu| &gpu.processes)
        .skip(view.process_scroll)
        .take(visible_rows)
        .map(|process| {
            Row::new(vec![
                process.pid.to_string(),
                process_owner(process),
                elapsed(process.elapsed_seconds),
                if include_command {
                    process
                        .command
                        .as_ref()
                        .map(|args| {
                            args.iter()
                                .map(|arg| safe_text(arg))
                                .collect::<Vec<_>>()
                                .join(" ")
                        })
                        .unwrap_or_else(|| safe_text(&process.name))
                } else {
                    safe_text(&process.name)
                },
                value(process.gpu_memory_mib(), " MiB"),
                process.process_type.short_label().into(),
            ])
        })
        .collect();
    let table = Table::new(
        rows,
        [
            Constraint::Length(7),
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Min(8),
            Constraint::Length(10),
            Constraint::Length(5),
        ],
    )
    .header(
        Row::new([
            "PID",
            "User",
            "Elapsed",
            if include_command {
                "Command / Name"
            } else {
                "Name"
            },
            "GPU memory",
            "Type",
        ])
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    )
    .block(block);
    frame.render_widget(table, area);
}

/// Each column represents equal elapsed time. Any explicit missing sample leaves a
/// gap in its bucket; populated buckets retain peaks so brief load is not erased.
pub fn time_buckets(
    history: &std::collections::VecDeque<HistoryPoint>,
    now_ms: u64,
    window_ms: u64,
    sample_interval_ms: u64,
    width: usize,
    metric: fn(&HistoryPoint) -> Option<u64>,
) -> Vec<Option<u64>> {
    if width == 0 || window_ms == 0 {
        return vec![];
    }
    let start = now_ms.saturating_sub(window_ms);
    let mut buckets = vec![None; width];
    let mut missing = vec![false; width];
    for point in history {
        if point.sampled_at_ms < start || point.sampled_at_ms > now_ms {
            continue;
        }
        let first = (((point.sampled_at_ms - start) as u128 * width as u128 / window_ms as u128)
            as usize)
            .min(width - 1);
        // A sampled value covers its acquisition interval, without extending over
        // missing acquisitions. This avoids fictitious gaps on wider terminals.
        let covered_until = point
            .sampled_at_ms
            .saturating_add(sample_interval_ms.saturating_sub(1))
            .min(now_ms);
        let last = (((covered_until - start) as u128 * width as u128 / window_ms as u128) as usize)
            .min(width - 1);
        for bucket in first..=last {
            match metric(point) {
                Some(value) if !missing[bucket] => {
                    buckets[bucket] = Some(buckets[bucket].unwrap_or(0).max(value))
                }
                None => {
                    missing[bucket] = true;
                    buckets[bucket] = None;
                }
                _ => {}
            }
        }
    }
    buckets
}

fn draw_overview(frame: &mut Frame, area: Rect, app: &App) {
    let rows = app
        .ordered_views()
        .map(|view| {
            if let Some(gpu) = &view.gpu {
                Row::new(vec![
                    gpu.device.index.to_string(),
                    safe_text(&gpu.device.name),
                    value(gpu.metrics.gpu_utilization, "%"),
                    gpu.memory.as_ref().map_or_else(
                        || "N/A".into(),
                        |memory| format!("{:.1}/{:.1}", memory.used_gib(), memory.total_gib()),
                    ),
                    gpu.memory.as_ref().map_or_else(
                        || "N/A".into(),
                        |memory| format!("{:.1}", memory.free as f64 / 1024_f64.powi(3)),
                    ),
                    value(gpu.metrics.temperature, "°C"),
                    gpu.processes.len().to_string(),
                    if app.is_stale(view) {
                        "STALE".into()
                    } else if gpu.issues.is_empty() {
                        "Live".into()
                    } else {
                        format!("{} issues", gpu.issues.len())
                    },
                ])
            } else {
                Row::new(vec![
                    view.index.to_string(),
                    "Unavailable".into(),
                    "N/A".into(),
                    "N/A".into(),
                    "N/A".into(),
                    "N/A".into(),
                    "N/A".into(),
                    "Failed".into(),
                ])
            }
        })
        .collect::<Vec<_>>();
    let table = Table::new(
        rows,
        [
            Constraint::Length(4),
            Constraint::Min(12),
            Constraint::Length(6),
            Constraint::Length(13),
            Constraint::Length(9),
            Constraint::Length(7),
            Constraint::Length(5),
            Constraint::Length(9),
        ],
    )
    .header(
        Row::new([
            "GPU",
            "Name",
            "Load",
            "Used/Total GiB",
            "Free GiB",
            "Temp",
            "Procs",
            "Status",
        ])
        .style(Style::default().fg(Color::Cyan)),
    )
    .block(Block::bordered().title("All selected GPUs · ↑/↓ select · Enter details · t toggle"))
    .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    let mut selected = TableState::default().with_selected(if app.device_count() > 0 {
        Some(app.selected_position())
    } else {
        None
    });
    frame.render_stateful_widget(table, area, &mut selected);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};
    use std::collections::VecDeque;

    #[test]
    fn issue7_narrow_history_renders_latest_sample_not_oldest() {
        let history = (0..60)
            .map(|i| HistoryPoint {
                sampled_at_ms: i,
                gpu: Some(if i == 59 { 100 } else { 0 }),
                memory: None,
            })
            .collect::<VecDeque<_>>();
        let mut terminal = Terminal::new(TestBackend::new(12, 3)).unwrap();
        terminal
            .draw(|frame| {
                draw_history(
                    frame,
                    frame.area(),
                    "Load".into(),
                    &history,
                    59,
                    60,
                    1,
                    |point| point.gpu,
                    Color::Green,
                )
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(11, 2)].symbol(), "█");
        assert_eq!(buffer[(0, 2)].symbol(), " ");
    }

    #[test]
    fn issue7_absent_sample_is_a_gap_distinct_from_zero() {
        let history = VecDeque::from([
            HistoryPoint {
                sampled_at_ms: 0,
                gpu: Some(0),
                memory: None,
            },
            HistoryPoint {
                sampled_at_ms: 1,
                gpu: None,
                memory: None,
            },
            HistoryPoint {
                sampled_at_ms: 2,
                gpu: Some(100),
                memory: None,
            },
        ]);
        let mut terminal = Terminal::new(TestBackend::new(3, 3)).unwrap();
        terminal
            .draw(|frame| {
                draw_history(
                    frame,
                    frame.area(),
                    "GPU".into(),
                    &history,
                    3,
                    3,
                    1,
                    |point| point.gpu,
                    Color::Green,
                )
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(0, 2)].symbol(), " ");
        assert_eq!(buffer[(1, 2)].symbol(), "×");
        assert_eq!(buffer[(2, 2)].symbol(), "█");
    }
}
