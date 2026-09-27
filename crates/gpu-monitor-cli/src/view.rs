//! The visible pane is a single value, not a set of independent toggles.
//!
//! Earlier revisions tracked overview, diagnostics and alert panes as separate
//! booleans that were mutually exclusive only because every key handler cleared
//! the other two. One enumeration makes the exclusivity structural.

/// A selectable pane. The device sidebar is persistent and therefore not a view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum View {
    /// Gauges and identity for the selected device.
    #[default]
    Dashboard,
    /// Process table for the selected device.
    Processes,
    /// Time series for the selected device.
    History,
    /// Driver and per-metric availability report.
    Diagnostics,
    /// Threshold and availability events for the session.
    Alerts,
}

impl View {
    pub const ALL: [Self; 5] = [
        Self::Dashboard,
        Self::Processes,
        Self::History,
        Self::Diagnostics,
        Self::Alerts,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Dashboard => "Dashboard",
            Self::Processes => "Processes",
            Self::History => "History",
            Self::Diagnostics => "Diagnostics",
            Self::Alerts => "Alerts",
        }
    }

    /// Digit that selects this view directly.
    pub fn hotkey(self) -> char {
        match self {
            Self::Dashboard => '1',
            Self::Processes => '2',
            Self::History => '3',
            Self::Diagnostics => '4',
            Self::Alerts => '5',
        }
    }

    pub fn from_hotkey(key: char) -> Option<Self> {
        Self::ALL.into_iter().find(|view| view.hotkey() == key)
    }

    pub fn position(self) -> usize {
        Self::ALL
            .iter()
            .position(|view| *view == self)
            .unwrap_or_default()
    }

    pub fn cycle(self, forward: bool) -> Self {
        let count = Self::ALL.len();
        let position = self.position();
        let next = if forward {
            (position + 1) % count
        } else {
            (position + count - 1) % count
        };
        Self::ALL[next]
    }

    /// Wrapped-text panes scroll by line; tables scroll by row.
    pub fn is_text_pane(self) -> bool {
        matches!(self, Self::Diagnostics | Self::Alerts)
    }

    /// Only the process table consumes a search filter.
    pub fn accepts_search(self) -> bool {
        matches!(self, Self::Processes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_view_is_reachable_by_hotkey_and_by_cycling() {
        for view in View::ALL {
            assert_eq!(View::from_hotkey(view.hotkey()), Some(view));
        }
        assert_eq!(View::from_hotkey('9'), None);
        let mut visited = vec![View::default()];
        for _ in 1..View::ALL.len() {
            visited.push(visited.last().unwrap().cycle(true));
        }
        assert_eq!(visited, View::ALL.to_vec());
    }

    #[test]
    fn cycling_wraps_in_both_directions() {
        assert_eq!(View::Dashboard.cycle(false), View::Alerts);
        assert_eq!(View::Alerts.cycle(true), View::Dashboard);
    }

    #[test]
    fn hotkeys_are_unique() {
        let mut keys = View::ALL.map(View::hotkey).to_vec();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), View::ALL.len());
    }
}
