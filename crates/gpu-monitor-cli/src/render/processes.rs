//! Full process table for the selected device.
//!
//! The header marks the active sort column, and the summary distinguishes an
//! empty result caused by a filter from a device that genuinely has no work.

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Paragraph, Row, Table, Wrap},
    Frame,
};

use crate::{app::App, device::ProcessSort, format, theme};

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let Some(view) = app.selected_view() else {
        frame.render_widget(
            Paragraph::new("No device is selected.")
                .wrap(Wrap { trim: true })
                .style(Style::default().fg(theme::STALE)),
            area,
        );
        return;
    };
    let processes = view.processes(&app.filter);
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(area);
    frame.render_widget(
        Paragraph::new(summary(app, processes.len(), view.sampled_process_count())),
        rows[0],
    );
    let capacity = app.process_rows();
    let first = view.process_scroll.min(processes.len());
    let table = Table::new(
        processes
            .into_iter()
            .skip(first)
            .take(capacity)
            .map(|process| {
                Row::new([
                    process.pid.to_string(),
                    format::process_owner(process),
                    format::elapsed(process.elapsed_seconds),
                    format::process_command(process, app.filter.include_command),
                    format::value(process.gpu_memory_mib(), " MiB"),
                    process.process_type.short_label().to_owned(),
                ])
            })
            .collect::<Vec<_>>(),
        [
            Constraint::Length(8),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Min(12),
            Constraint::Length(12),
            Constraint::Length(5),
        ],
    )
    .header(header(app));
    frame.render_widget(table, rows[1]);
}

fn header(app: &App) -> Row<'static> {
    let name = if app.filter.include_command {
        "Command"
    } else {
        "Name"
    };
    let mark = |column: ProcessSort, label: &str| {
        if app.filter.sort == column {
            format!("{label} ▼")
        } else {
            label.to_owned()
        }
    };
    Row::new([
        mark(ProcessSort::Pid, "PID"),
        "User".to_owned(),
        mark(ProcessSort::Elapsed, "Elapsed"),
        mark(ProcessSort::Name, name),
        mark(ProcessSort::Memory, "GPU memory"),
        "Type".to_owned(),
    ])
    .style(theme::table_header())
}

fn summary(app: &App, visible: usize, sampled: usize) -> Line<'static> {
    let Some(view) = app.selected_view() else {
        return Line::raw("");
    };
    if visible == 0 {
        let (text, style) = if view.processes_incomplete() {
            (
                " Process list unavailable or incomplete for this device.".to_owned(),
                Style::default().fg(theme::STALE),
            )
        } else if app.filter.is_active() {
            (
                format!(
                    " No process matches \"{}\" · {sampled} sampled · / edits the filter",
                    app.filter.query.trim()
                ),
                theme::footer(),
            )
        } else {
            (
                " No process is using this device.".to_owned(),
                theme::footer(),
            )
        };
        return Line::from(Span::styled(text, style));
    }
    let first = view.process_scroll.min(visible.saturating_sub(1)) + 1;
    let last = (view.process_scroll + app.process_rows().max(1)).min(visible);
    let mut text = format!(" {first}-{last} of {visible}");
    if visible != sampled {
        text.push_str(&format!(" · filtered from {sampled}"));
    }
    Line::from(Span::styled(text, theme::footer()))
}
