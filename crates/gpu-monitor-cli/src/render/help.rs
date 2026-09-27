//! Key reference, generated from the binding declarations.
//!
//! Because the list is derived from the same table the handler uses, a key can
//! never be documented here without being implemented.

use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph},
    Frame,
};

use crate::{
    keymap::{Group, BINDINGS},
    render::centered,
    theme,
};

const KEY_COLUMN: usize = 18;

pub fn draw(frame: &mut Frame, area: Rect) {
    let lines = lines();
    // Wide enough for the longest key list and description declared below.
    let width = required_width().min(area.width);
    let height = (lines.len() as u16 + 2).min(area.height);
    let target = centered(area, width, height);
    let block = Block::bordered()
        .title(" Keys · ? or any key closes ")
        .border_style(theme::footer())
        .title_style(theme::table_header());
    let inner = block.inner(target);
    frame.render_widget(Clear, target);
    frame.render_widget(block, target);
    frame.render_widget(Paragraph::new(lines), inner);
}

/// Derived from the declarations, so adding a binding cannot clip the panel.
fn required_width() -> u16 {
    let widest = BINDINGS
        .iter()
        .map(|binding| {
            KEY_COLUMN.max(binding.keys_label().chars().count() + 1)
                + binding.description.chars().count()
        })
        .max()
        .unwrap_or(0);
    // Two leading spaces, plus the panel's own borders.
    (widest + 5) as u16
}

fn lines() -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for (index, group) in Group::ALL.into_iter().enumerate() {
        if index > 0 {
            lines.push(Line::raw(""));
        }
        lines.push(Line::from(Span::styled(
            group.title().to_owned(),
            theme::table_header(),
        )));
        for binding in BINDINGS.iter().filter(|binding| binding.group == group) {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {:<KEY_COLUMN$}", binding.keys_label()),
                    theme::footer(),
                ),
                Span::raw(binding.description.to_owned()),
            ]));
        }
    }
    lines
}
