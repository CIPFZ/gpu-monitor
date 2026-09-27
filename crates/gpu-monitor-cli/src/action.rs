//! Intent produced by a key press, decided before any state is touched.
//!
//! Keeping intent separate from application state lets the key map be a pure
//! function and keeps one scrolling implementation for every scrollable pane.

use crate::view::View;

/// A movement request expressed in rows, resolved against the measured viewport.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scroll {
    LineUp,
    LineDown,
    PageUp,
    PageDown,
    Top,
    Bottom,
}

/// Editing step applied to the process search query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SearchEdit {
    /// Start editing, preserving any committed query.
    Begin,
    Push(char),
    Pop,
    /// Keep the typed query and leave editing.
    Commit,
    /// Leave editing and restore the query that was active before.
    /// Committing an empty query is how a filter is dropped.
    Cancel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Quit,
    /// Request immediate driver reinitialisation.
    Retry,
    ToggleHelp,
    ToggleSidebar,
    ToggleCommandColumn,
    NextDevice,
    PreviousDevice,
    GoToView(View),
    CycleView(bool),
    Scroll(Scroll),
    CycleProcessSort,
    /// Step through the supported chart windows without restarting the process.
    CycleHistoryWindow,
    Search(SearchEdit),
}
