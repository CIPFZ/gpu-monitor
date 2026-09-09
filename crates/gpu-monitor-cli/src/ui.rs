//! One selected GPU per page, with layout-derived process scrolling.

use gpu_monitor_core::GpuInfo;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph, Row, Sparkline, Table, Wrap},
    Frame,
};

use crate::app::{App, DeviceView, HistoryPoint};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(frame.area());
    let header = format!(
        " GPU Monitor | Device {}/{} | {} failed | Sample {} ms",
        if app.device_count() == 0 {
            0
        } else {
            app.selected_position() + 1
        },
        app.device_count(),
        app.failure_count,
        app.sampled_at_ms
    );
    frame.render_widget(
        Paragraph::new(header).style(Style::default().fg(Color::Cyan)),
        chunks[0],
    );
    frame.render_widget(
        Paragraph::new("←/→ Tab GPU | ↑/↓ PgUp/PgDn scroll | Home/End | r retry | q/Ctrl-C quit"),
        chunks[2],
    );

    let Some(view) = app.selected_view() else {
        let message = app
            .error
            .as_deref()
            .unwrap_or("No GPU devices detected. Retrying automatically; r retries now.");
        frame.render_widget(
            Paragraph::new(message)
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
        |gpu| format!(" GPU {}: {} ", gpu.device.index, gpu.device.name),
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
            "GPU load: {} | recent samples; × = missing",
            value(gpu_value, "%")
        ),
        &view.history,
        |point| point.gpu,
        Color::Green,
    );
    draw_history(
        frame,
        sections[2],
        memory_title,
        &view.history,
        |point| point.memory,
        Color::Cyan,
    );

    let status = if let Some(error) = &view.error {
        format!("STALE / unavailable: {error}. Retrying automatically; r retries now.")
    } else if let Some(gpu) = &view.gpu {
        if let Some(issue) = gpu.issues.first() {
            format!(
                "{} unavailable metric(s): {}: {}",
                gpu.issues.len(),
                issue.metric,
                issue.error.message
            )
        } else {
            "Live | Charts show the latest visible samples; capacity differs from memory I/O activity.".into()
        }
    } else {
        "Waiting for a successful sample.".into()
    };
    frame.render_widget(
        Paragraph::new(status)
            .wrap(Wrap { trim: true })
            .style(Style::default().fg(
                if view.error.is_some()
                    || view.gpu.as_ref().is_some_and(|gpu| !gpu.issues.is_empty())
                {
                    Color::Yellow
                } else {
                    Color::DarkGray
                },
            )),
        sections[3],
    );
    draw_processes(frame, sections[4], view, visible_rows);
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
    metric: fn(&HistoryPoint) -> Option<u64>,
    color: Color,
) {
    let block = Block::default().title(title);
    let width = block.inner(area).width as usize;
    // Sparkline reads from the start of its input; slice the *tail* to keep newest data visible.
    let samples: Vec<Option<u64>> = history
        .iter()
        .skip(history.len().saturating_sub(width))
        .map(metric)
        .collect();
    let chart = Sparkline::default()
        .block(block)
        .data(samples)
        .max(100)
        .absent_value_symbol("×")
        .absent_value_style(Style::default().fg(Color::DarkGray))
        .style(Style::default().fg(color));
    frame.render_widget(chart, area);
}

fn draw_processes(frame: &mut Frame, area: Rect, view: &DeviceView, visible_rows: usize) {
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
    let label = if view.error.is_some() {
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
                process.name.clone(),
                value(process.gpu_memory_mib(), " MiB"),
                process.process_type.short_label().into(),
            ])
        })
        .collect();
    let table = Table::new(
        rows,
        [
            Constraint::Length(8),
            Constraint::Min(8),
            Constraint::Length(12),
            Constraint::Length(5),
        ],
    )
    .header(
        Row::new(["PID", "Name", "GPU memory", "Type"]).style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    )
    .block(block);
    frame.render_widget(table, area);
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
                sampled_at_ms: 1,
                gpu: Some(0),
                memory: None,
            },
            HistoryPoint {
                sampled_at_ms: 2,
                gpu: None,
                memory: None,
            },
            HistoryPoint {
                sampled_at_ms: 3,
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
