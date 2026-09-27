//! Rendering is a pure projection of state that was already measured.
//!
//! Nothing here mutates the application, so a pane cannot silently change the
//! selection or a scroll offset while it draws. Every viewport it relies on was
//! resolved by `App::measure` against the same frame.

mod dashboard;
mod help;
mod history;
mod panel;
mod processes;
mod sidebar;

use ratatui::{
    layout::{Alignment, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
    Frame,
};

use crate::{
    app::App,
    format, keymap,
    layout::{self as frames},
    theme,
    view::View,
};

pub fn draw(frame: &mut Frame, app: &App) {
    let Some(chrome) = app.chrome() else {
        // A partial interface is more misleading than an explicit request.
        frame.render_widget(
            Paragraph::new(frames::resize_request(frame.area()))
                .wrap(Wrap { trim: true })
                .alignment(Alignment::Center)
                .style(Style::default().fg(theme::STALE)),
            frame.area(),
        );
        return;
    };
    header(frame, chrome.header, app);
    if let Some(area) = chrome.sidebar {
        sidebar::draw(frame, area, app);
    }
    tabs(frame, chrome.tabs, app);
    match app.view {
        View::Dashboard => dashboard::draw(frame, chrome.body, app),
        View::Processes => processes::draw(frame, chrome.body, app),
        View::History => history::draw(frame, chrome.body, app),
        View::Diagnostics | View::Alerts => panel::draw(frame, chrome.body, app),
    }
    status(frame, chrome.status, app);
    footer(frame, chrome.footer, app);
    if app.help_open {
        help::draw(frame, chrome.body);
    }
}

fn header(frame: &mut Frame, area: Rect, app: &App) {
    let devices = app.device_count();
    let mut parts = vec![
        "GPU Monitor".to_owned(),
        format::safe_text(&app.source_label),
        format!("{devices} device{}", if devices == 1 { "" } else { "s" }),
    ];
    if app.failure_count > 0 {
        parts.push(format!("{} failed", app.failure_count));
    }
    parts.push(format!("{:.1}s", app.sample_interval_ms as f64 / 1000.0));
    parts.push(format!(
        "{} window",
        format::window_label(app.history_window_ms)
    ));
    frame.render_widget(
        Paragraph::new(format!(" {} ", parts.join(" │ "))).style(theme::header()),
        area,
    );
}

fn tabs(frame: &mut Frame, area: Rect, app: &App) {
    // On a narrow pane only the active view is named, so the label never clips
    // to something ambiguous.
    let full: usize = View::ALL.iter().map(|view| view.title().len() + 5).sum();
    if (area.width as usize) < full {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!(" {} ", app.view.title()), theme::tab_active()),
                Span::styled(
                    format!(" {}/{} ", app.view.position() + 1, View::ALL.len()),
                    theme::footer(),
                ),
            ])),
            area,
        );
        return;
    }
    let mut spans = Vec::new();
    for view in View::ALL {
        let style = if view == app.view {
            theme::tab_active()
        } else {
            theme::tab_inactive()
        };
        spans.push(Span::styled(
            format!(" {} {} ", view.hotkey(), view.title()),
            style,
        ));
        spans.push(Span::styled("│", theme::footer()));
    }
    spans.pop();
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// One line that always explains the freshness of what is on screen.
fn status(frame: &mut Frame, area: Rect, app: &App) {
    if app.is_editing_search() {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" Filter ", theme::header()),
                Span::raw(format!(" {}", format::safe_text(app.search_text()))),
                Span::styled("▏", Style::default().fg(theme::ACCENT)),
                Span::styled("  Enter apply · Esc cancel", theme::footer()),
            ])),
            area,
        );
        return;
    }
    let (text, style) = status_text(app);
    frame.render_widget(Paragraph::new(format::safe_text(&text)).style(style), area);
}

fn status_text(app: &App) -> (String, Style) {
    let stale = Style::default().fg(theme::STALE);
    let Some(view) = app.selected_view() else {
        let message = app.error.clone().unwrap_or_else(|| {
            if app.selection.has_device_filter() {
                "No device matches the current filters. Monitoring continues.".into()
            } else {
                "No device detected. Retrying automatically; r retries now.".into()
            }
        });
        return (format!(" {message}"), stale);
    };
    if let Some(error) = &view.error {
        return (
            format!(" Device unavailable: {error}. Retrying automatically; r retries now."),
            Style::default().fg(theme::FAILED),
        );
    }
    if app.is_stale(view) {
        let detail = app
            .error
            .clone()
            .unwrap_or_else(|| "waiting for the sampler; r retries now".into());
        return (format!(" Stale: {detail}."), stale);
    }
    if let Some(gpu) = &view.gpu {
        if let Some(issue) = gpu.issues.first() {
            return (
                format!(
                    " {} unavailable metric(s) · {}: {}",
                    gpu.issues.len(),
                    issue.metric,
                    issue.error.message
                ),
                stale,
            );
        }
    }
    (format!(" {}", reading_summary(app)), theme::footer())
}

/// Describes the display choices in effect, so a filtered list is never mistaken
/// for an empty one.
fn reading_summary(app: &App) -> String {
    let mut parts = vec!["Live".to_owned()];
    match app.view {
        View::Processes => {
            parts.push(format!("sorted by {}", app.filter.sort.label()));
            if app.filter.is_active() {
                parts.push(format!("filter \"{}\"", app.filter.query.trim()));
            }
            if app.filter.include_command {
                parts.push("full command lines".into());
            }
        }
        View::History => {
            parts.push(format!(
                "{} window · buckets keep peaks, × marks missing data",
                format::window_label(app.history_window_ms)
            ));
        }
        View::Alerts if app.events.is_empty() => parts.push("no events yet".into()),
        View::Alerts => parts.push(format!("{} event(s) retained", app.events.len())),
        View::Diagnostics => parts.push("driver and per-metric availability".into()),
        View::Dashboard => {
            if !app.sidebar_visible() && app.device_count() > 1 {
                parts.push("b shows the device sidebar".into());
            }
        }
    }
    parts.join(" · ")
}

fn footer(frame: &mut Frame, area: Rect, app: &App) {
    let hint = if app.is_editing_search() {
        "Type to filter · Enter apply · Esc cancel".to_owned()
    } else {
        keymap::footer_hint()
    };
    frame.render_widget(
        Paragraph::new(format!(" {hint}")).style(theme::footer()),
        area,
    );
}

/// Centres a fixed-size box inside `area`, never exceeding it.
pub(crate) fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}
