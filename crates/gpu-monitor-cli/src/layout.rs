//! Responsive frame solving, kept free of rendering so it can be tested directly.
//!
//! The minimum size is a constant that both the guard and its message use, so the
//! interface can no longer refuse to draw at one size while advising another.

use ratatui::layout::{Constraint, Layout, Rect};

/// Smallest usable terminal: header, tab row, five body rows, status and footer.
pub const MIN_WIDTH: u16 = 48;
pub const MIN_HEIGHT: u16 = 12;

const SIDEBAR_WIDTH: u16 = 28;
/// Narrower than this, a sidebar would starve the detail pane, so it collapses.
const SIDEBAR_MIN_TOTAL_WIDTH: u16 = 92;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chrome {
    pub header: Rect,
    /// Absent when the user hid it or the terminal is too narrow to afford it.
    pub sidebar: Option<Rect>,
    pub tabs: Rect,
    pub body: Rect,
    pub status: Rect,
    pub footer: Rect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Frames {
    Ready(Chrome),
    /// The caller renders a resize request instead of a partial interface.
    TooSmall,
}

pub fn solve(area: Rect, sidebar_requested: bool) -> Frames {
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        return Frames::TooSmall;
    }
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);
    let sidebar_visible = sidebar_requested && area.width >= SIDEBAR_MIN_TOTAL_WIDTH;
    let (sidebar, content) = if sidebar_visible {
        let columns = Layout::horizontal([Constraint::Length(SIDEBAR_WIDTH), Constraint::Min(0)])
            .split(rows[1]);
        (Some(columns[0]), columns[1])
    } else {
        (None, rows[1])
    };
    let panes = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(content);
    Frames::Ready(Chrome {
        header: rows[0],
        sidebar,
        tabs: panes[0],
        body: panes[1],
        status: rows[2],
        footer: rows[3],
    })
}

/// Generated from the same constants as the guard above.
pub fn resize_request(area: Rect) -> String {
    format!(
        "Terminal is {}×{}. GPU Monitor needs at least {MIN_WIDTH}×{MIN_HEIGHT}.",
        area.width, area.height
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chrome(width: u16, height: u16, sidebar: bool) -> Chrome {
        match solve(Rect::new(0, 0, width, height), sidebar) {
            Frames::Ready(chrome) => chrome,
            Frames::TooSmall => panic!("{width}×{height} should be renderable"),
        }
    }

    #[test]
    fn the_minimum_size_guard_and_its_message_agree() {
        assert_eq!(
            solve(Rect::new(0, 0, MIN_WIDTH, MIN_HEIGHT - 1), true),
            Frames::TooSmall
        );
        assert_eq!(
            solve(Rect::new(0, 0, MIN_WIDTH - 1, MIN_HEIGHT), true),
            Frames::TooSmall
        );
        assert!(matches!(
            solve(Rect::new(0, 0, MIN_WIDTH, MIN_HEIGHT), true),
            Frames::Ready(_)
        ));
        let message = resize_request(Rect::new(0, 0, 10, 4));
        assert!(message.contains(&format!("{MIN_WIDTH}×{MIN_HEIGHT}")));
        assert!(message.contains("10×4"));
    }

    #[test]
    fn a_narrow_terminal_spends_its_width_on_the_detail_pane() {
        let narrow = chrome(80, 24, true);
        assert_eq!(narrow.sidebar, None);
        assert_eq!(narrow.body.width, 80);
        let wide = chrome(120, 24, true);
        assert_eq!(
            wide.sidebar.map(|sidebar| sidebar.width),
            Some(SIDEBAR_WIDTH)
        );
        assert_eq!(wide.body.width, 120 - SIDEBAR_WIDTH);
    }

    #[test]
    fn hiding_the_sidebar_is_honoured_on_any_width() {
        assert_eq!(chrome(200, 40, false).sidebar, None);
        assert_eq!(chrome(200, 40, false).body.width, 200);
    }

    #[test]
    fn chrome_rows_tile_the_area_without_gaps_or_overlap() {
        let chrome = chrome(120, 24, true);
        assert_eq!(chrome.header.y, 0);
        assert_eq!(chrome.tabs.y, 1);
        assert_eq!(chrome.body.y, 2);
        assert_eq!(chrome.body.bottom(), chrome.status.y);
        assert_eq!(chrome.status.y, 22);
        assert_eq!(chrome.footer.y, 23);
        let sidebar = chrome.sidebar.unwrap();
        assert_eq!(sidebar.y, 1, "the sidebar spans the tab row as well");
        assert_eq!(sidebar.bottom(), chrome.status.y);
    }
}
