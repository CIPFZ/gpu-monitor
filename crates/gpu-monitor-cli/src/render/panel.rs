//! Scrollable text panes for diagnostics and alert events.
//!
//! The lines were wrapped during measurement, so the scrollbar reflects the real
//! rendered length instead of an estimate and the pane cannot scroll past its end.

use ratatui::{
    layout::Rect,
    text::Line,
    widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};

use crate::{app::App, theme};

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let lines = app.panel_lines();
    let visible = lines
        .iter()
        .skip(app.panel_scroll())
        .take(area.height as usize)
        .map(|line| Line::raw(line.clone()))
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(visible), area);
    if lines.len() > area.height as usize {
        let mut state = ScrollbarState::new(lines.len().saturating_sub(area.height as usize))
            .position(app.panel_scroll());
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None)
                .track_style(theme::footer())
                .thumb_style(theme::footer()),
            area,
            &mut state,
        );
    }
}
