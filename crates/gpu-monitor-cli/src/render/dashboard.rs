//! Everything about one device that fits on a single screen.
//!
//! Readings that have a natural limit are drawn as meters so that pressure is
//! visible before the number is read, and an unavailable reading keeps a distinct
//! fill rather than an empty bar that would look like zero.

use gpu_monitor_core::GpuInfo;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Paragraph, Row, Table, Wrap},
    Frame,
};

use crate::{app::App, format, theme, view::View, widgets};

/// Identity, meters and the fact block, before the process preview.
const SUMMARY_ROWS: u16 = 14;
/// A preview needs a title, a header and at least two entries to be worth space.
const MIN_PREVIEW_ROWS: u16 = 4;

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let Some(view) = app.selected_view() else {
        frame.render_widget(
            Paragraph::new("No device is selected. Monitoring continues in the background.")
                .wrap(Wrap { trim: true })
                .style(Style::default().fg(theme::STALE)),
            area,
        );
        return;
    };
    let Some(gpu) = &view.gpu else {
        let reason = view
            .error
            .clone()
            .or_else(|| app.error.clone())
            .unwrap_or_else(|| "Waiting for the first successful sample.".into());
        frame.render_widget(
            Paragraph::new(format!(
                "GPU {} reported no usable sample.\n\n{}",
                view.index,
                format::safe_lines(&reason)
            ))
            .wrap(Wrap { trim: true })
            .style(Style::default().fg(theme::STALE)),
            area,
        );
        return;
    };
    let rows = Layout::vertical([
        Constraint::Length(SUMMARY_ROWS.min(area.height)),
        Constraint::Min(0),
    ])
    .split(area);
    frame.render_widget(Paragraph::new(summary(gpu, area.width)), rows[0]);
    if rows[1].height >= MIN_PREVIEW_ROWS {
        preview(frame, rows[1], app);
    }
}

fn summary(gpu: &GpuInfo, width: u16) -> Vec<Line<'static>> {
    let metrics = &gpu.metrics;
    let memory = gpu.memory.as_ref();
    let power = metrics.power_watts();
    let identity = format!(
        "{} · {} · driver {} · CUDA {}",
        format::safe_text(&gpu.device.uuid),
        format::safe_text(&gpu.device.pci_bus_id),
        format::safe_text(&gpu.device.driver_version),
        gpu.device
            .cuda_version
            .as_deref()
            .map(format::safe_text)
            .unwrap_or_else(|| format::UNAVAILABLE.to_owned()),
    );
    vec![
        Line::from(Span::styled(
            format!(
                "GPU {} · {}",
                gpu.device.index,
                format::safe_text(&gpu.device.name)
            ),
            theme::table_header(),
        )),
        Line::from(Span::styled(
            format::truncate_str(&identity, width as usize),
            theme::footer(),
        )),
        Line::raw(""),
        widgets::meter(
            width,
            "Load",
            metrics.gpu_utilization.map(f64::from),
            &format::value(metrics.gpu_utilization, "%"),
            theme::from_load(metrics.gpu_utilization.map(f64::from)),
        ),
        widgets::meter(
            width,
            "Memory",
            format::memory_ratio(memory),
            &format::memory_capacity(memory),
            theme::from_memory(format::memory_ratio(memory)),
        ),
        widgets::meter(
            width,
            "Temp",
            metrics.temperature.map(f64::from),
            &format::value(metrics.temperature, "°C"),
            theme::from_temperature(metrics.temperature),
        ),
        widgets::meter(
            width,
            "Power",
            theme::power_ratio(power, gpu.device.power_limit),
            &format::power_capacity(power, gpu.device.power_limit),
            theme::from_power(power, gpu.device.power_limit),
        ),
        widgets::meter(
            width,
            "Fan",
            metrics.fan_speed.map(f64::from),
            &format::value(metrics.fan_speed, "%"),
            theme::Severity::Notice,
        ),
        Line::raw(""),
        fact_line(&[
            (
                "State",
                format::value(
                    metrics.performance_state.as_deref().map(format::safe_text),
                    "",
                ),
            ),
            (
                "Throttle",
                format::throttle(metrics.throttle_reasons.as_ref()),
            ),
        ]),
        fact_line(&[
            (
                "Graphics clock",
                format::value(metrics.clock_graphics, " MHz"),
            ),
            ("SM", format::value(metrics.clock_sm, " MHz")),
            ("Memory", format::value(metrics.clock_memory, " MHz")),
        ]),
        fact_line(&[
            ("Memory I/O", format::value(metrics.memory_utilization, "%")),
            ("Encoder", format::value(metrics.encoder_utilization, "%")),
            ("Decoder", format::value(metrics.decoder_utilization, "%")),
        ]),
        fact_line(&[
            (
                "PCIe",
                format::pcie_link(metrics.pcie_generation, metrics.pcie_width),
            ),
            ("RX", format::value(metrics.pcie_rx_kb_per_second, " KB/s")),
            ("TX", format::value(metrics.pcie_tx_kb_per_second, " KB/s")),
        ]),
    ]
}

fn fact_line(fields: &[(&str, String)]) -> Line<'static> {
    let mut spans = Vec::new();
    for (index, (label, value)) in fields.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled("  ", theme::footer()));
        }
        spans.extend(widgets::field(label, value.clone()));
    }
    Line::from(spans)
}

/// The highest-consuming processes, as an entry point to the full table.
fn preview(frame: &mut Frame, area: Rect, app: &App) {
    let Some(view) = app.selected_view() else {
        return;
    };
    let processes = view.processes(&app.filter);
    let capacity = area.height.saturating_sub(2) as usize;
    let shown = processes.len().min(capacity);
    let title = if processes.is_empty() {
        if view.processes_incomplete() {
            "Processes · list unavailable or incomplete".to_owned()
        } else if app.filter.is_active() {
            format!("Processes · none match \"{}\"", app.filter.query.trim())
        } else {
            "Processes · none using this device".to_owned()
        }
    } else {
        format!(
            "Processes · top {shown} of {} · {} for the full table",
            processes.len(),
            View::Processes.hotkey()
        )
    };
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(area);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(title, theme::table_header()))),
        rows[0],
    );
    if processes.is_empty() {
        // Column headings above nothing only add noise; the title already explains.
        return;
    }
    let table = Table::new(
        processes
            .into_iter()
            .take(capacity)
            .map(|process| {
                Row::new([
                    process.pid.to_string(),
                    format::truncate_str(&format::process_owner(process), 12),
                    format::truncate_str(
                        &format::process_command(process, app.filter.include_command),
                        48,
                    ),
                    format::value(process.gpu_memory_mib(), " MiB"),
                ])
            })
            .collect::<Vec<_>>(),
        [
            Constraint::Length(8),
            Constraint::Length(12),
            Constraint::Min(10),
            Constraint::Length(12),
        ],
    )
    .header(Row::new(["PID", "User", "Name", "GPU memory"]).style(theme::footer()));
    frame.render_widget(table, rows[1]);
}
