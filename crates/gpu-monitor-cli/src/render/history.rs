//! Time series for the selected device.
//!
//! The runtime already records temperature and power alongside load and memory,
//! so all four are charted here. Columns represent equal elapsed time, populated
//! buckets keep their peak, and a bucket whose acquisition failed is drawn as a
//! gap. Time before the first sample is left blank rather than filled with gap
//! markers, so a young session is not mistaken for a broken one.

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Paragraph, Sparkline, Wrap},
    Frame,
};

use crate::{
    app::App,
    device::HistoryPoint,
    format, theme,
    widgets::{self, Bucket},
};

/// How a series maps its readings onto an axis.
enum Scale {
    /// Always full scale, so two devices can be compared directly.
    Percent,
    Celsius,
    /// The device's own limit is the only meaningful ceiling for a draw.
    PowerLimit,
}

struct Series {
    title: &'static str,
    color: Color,
    unit: &'static str,
    metric: fn(&HistoryPoint) -> Option<u64>,
    scale: Scale,
}

const SERIES: [Series; 4] = [
    Series {
        title: "GPU load",
        color: theme::LOAD_SERIES,
        unit: "%",
        metric: |point| point.gpu,
        scale: Scale::Percent,
    },
    Series {
        title: "Memory used",
        color: theme::MEMORY_SERIES,
        unit: "%",
        metric: |point| point.memory,
        scale: Scale::Percent,
    },
    Series {
        title: "Temperature",
        color: theme::TEMPERATURE_SERIES,
        unit: "°C",
        metric: |point| point.temperature,
        scale: Scale::Celsius,
    },
    Series {
        title: "Power",
        color: theme::POWER_SERIES,
        unit: " W",
        metric: |point| point.power,
        scale: Scale::PowerLimit,
    },
];

/// Below this, charting four series would leave one row each.
const MIN_ROWS_FOR_ALL: u16 = 12;

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
    let power_limit = view
        .gpu
        .as_ref()
        .and_then(|gpu| gpu.device.power_limit)
        .map(u64::from);
    let shown = if area.height >= MIN_ROWS_FOR_ALL {
        SERIES.len()
    } else {
        2
    };
    let slots = Layout::vertical(vec![Constraint::Ratio(1, shown as u32); shown]).split(area);
    for (series, slot) in SERIES.iter().take(shown).zip(slots.iter()) {
        let rows = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(*slot);
        let buckets = widgets::time_buckets(
            &view.history,
            app.now_ms,
            app.history_window_ms,
            app.sample_interval_ms,
            rows[1].width as usize,
            series.metric,
        );
        let ceiling = match series.scale {
            Scale::Percent | Scale::Celsius => 100,
            Scale::PowerLimit => {
                power_limit.unwrap_or_else(|| widgets::series_ceiling(&buckets, 50))
            }
        };
        let latest = view.history.back().and_then(|point| (series.metric)(point));
        let covered = buckets
            .iter()
            .filter(|bucket| **bucket != Bucket::Unsampled)
            .count();
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(series.title, theme::table_header()),
                Span::styled(
                    format!(
                        "  now {}  ·  scale {}{}  ·  {} window",
                        format::value(latest, series.unit),
                        ceiling,
                        series.unit,
                        format::window_label(app.history_window_ms)
                    ),
                    theme::footer(),
                ),
            ])),
            rows[0],
        );
        if covered == 0 {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "collecting…",
                    Style::default().fg(theme::MUTED),
                ))),
                rows[1],
            );
            continue;
        }
        // Draw only the span that the session actually covers, at the right edge.
        let (leading, samples) = widgets::sampled_tail(&buckets);
        let chart = Rect {
            x: rows[1].x + leading as u16,
            width: rows[1].width.saturating_sub(leading as u16),
            ..rows[1]
        };
        frame.render_widget(
            Sparkline::default()
                .data(samples)
                .max(ceiling)
                .absent_value_symbol("×")
                .absent_value_style(Style::default().fg(theme::MUTED))
                .style(Style::default().fg(series.color)),
            chart,
        );
    }
}
