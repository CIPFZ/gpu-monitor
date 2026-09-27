//! Persistent device list.
//!
//! Multi-GPU hosts previously had to step through devices one at a time to learn
//! whether another card was busy or unavailable. Keeping the list on screen makes
//! that comparison continuous instead of a navigation task.

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::{app::App, device::DeviceView, format, theme, widgets};

/// Marker, index, gap, load column and the separating spaces.
const NON_NAME_COLUMNS: u16 = 11;

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::new()
        .borders(Borders::RIGHT)
        .border_style(theme::footer());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(inner);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(" Devices", theme::table_header()))),
        rows[0],
    );
    let name_width = inner.width.saturating_sub(NON_NAME_COLUMNS) as usize;
    let items = app
        .ordered_views()
        .map(|view| ListItem::new(entry(view, app, name_width)))
        .collect::<Vec<_>>();
    if items.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                " none detected",
                Style::default().fg(theme::STALE),
            ))),
            rows[1],
        );
    } else {
        let mut state = ListState::default().with_selected(Some(app.selected_position()));
        frame.render_stateful_widget(
            List::new(items).highlight_style(theme::selected_row()),
            rows[1],
            &mut state,
        );
    }
    frame.render_widget(Paragraph::new(summary(app)), rows[2]);
}

fn entry(view: &DeviceView, app: &App, name_width: usize) -> Line<'static> {
    let stale = app.is_stale(view);
    let load = view
        .gpu
        .as_ref()
        .and_then(|gpu| gpu.metrics.gpu_utilization);
    let severity = if view.error.is_some() {
        theme::Severity::Critical
    } else if stale {
        theme::Severity::Unknown
    } else {
        theme::from_load(load.map(f64::from))
    };
    let name = view.gpu.as_ref().map_or_else(
        || "unavailable".to_owned(),
        |gpu| format::device_model(&gpu.device.name),
    );
    let reading = if view.error.is_some() {
        "fail".to_owned()
    } else if stale {
        "stale".to_owned()
    } else {
        format::value(load, "%")
    };
    Line::from(vec![
        Span::raw(" "),
        widgets::dot(severity),
        Span::raw(format!(" {:<2} ", view.index)),
        Span::raw(format!(
            "{:<width$}",
            format::truncate_str(&name, name_width),
            width = name_width
        )),
        Span::styled(format!("{reading:>5}"), severity.style()),
    ])
}

fn summary(app: &App) -> Line<'static> {
    let mut parts = Vec::new();
    if app.failure_count > 0 {
        parts.push((
            format!("{} failed", app.failure_count),
            Style::default().fg(theme::FAILED),
        ));
    }
    if !app.events.is_empty() {
        parts.push((
            format!("{} alerts", app.events.len()),
            Style::default().fg(theme::STALE),
        ));
    }
    if parts.is_empty() {
        return Line::from(Span::styled(
            format!(" {} shown", app.device_count()),
            theme::footer(),
        ));
    }
    let mut spans = vec![Span::raw(" ")];
    for (index, (text, style)) in parts.into_iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(" · ", theme::footer()));
        }
        spans.push(Span::styled(text, style));
    }
    Line::from(spans)
}
