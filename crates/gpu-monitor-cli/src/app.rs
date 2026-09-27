//! Interface state and the reducer that key intents act on.
//!
//! Three rules shape this module. The visible pane is one enumeration rather
//! than a set of booleans that have to be cleared in pairs. Every key press is
//! turned into an `Action` first, so one scrolling implementation serves every
//! pane. Measurement happens before rendering, so drawing itself needs no
//! mutable access and can be reasoned about as a pure projection of this state.

use crossterm::event::{self, Event};
use gpu_monitor_core::MonitorSnapshot;
use gpu_monitor_runtime::{AlertEvent, HistoryResponse};
use ratatui::layout::Rect;
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

use crate::{
    action::{Action, Scroll, SearchEdit},
    device::{DeviceView, HistoryPoint, ProcessFilter},
    feed::Feed,
    format,
    keymap::{self, HISTORY_WINDOWS_MS},
    layout::{self, Chrome, Frames},
    output, render,
    selection::Selection,
    terminal::Tui,
    view::View,
};

pub const MIN_INTERVAL_MS: u64 = 100;

/// Rows consumed by a pane's own header and summary line.
const PROCESS_CHROME_ROWS: usize = 2;

/// What the caller must do after a key press.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    None,
    /// The user asked for immediate driver reinitialisation.
    Retry,
}

pub struct App {
    exit: bool,
    interval: Duration,
    last_refresh: Option<Instant>,
    views: HashMap<String, DeviceView>,
    order: Vec<String>,
    selected: Option<String>,

    pub selection: Selection,
    pub filter: ProcessFilter,
    /// Present while the query is being typed; the committed query lives in `filter`.
    search_draft: Option<String>,

    pub view: View,
    pub help_open: bool,
    pub sidebar_requested: bool,
    pub history_window_ms: u64,
    /// Set when the full series must be re-read, not when a sample arrives.
    history_reload: bool,

    pub sample_interval_ms: u64,
    pub now_ms: u64,
    pub sampled_at_ms: u64,
    pub failure_count: usize,
    pub error: Option<String>,
    pub events: Vec<AlertEvent>,
    pub source_label: String,
    pub latest_snapshot: Option<MonitorSnapshot>,

    frames: Frames,
    panel_lines: Vec<String>,
    panel_scroll: usize,
    process_rows: usize,
    panel_rows: usize,
}

impl App {
    pub fn new(interval_ms: u64) -> Self {
        let interval_ms = interval_ms.max(MIN_INTERVAL_MS);
        Self {
            exit: false,
            interval: Duration::from_millis(interval_ms),
            last_refresh: None,
            views: HashMap::new(),
            order: Vec::new(),
            selected: None,
            selection: Selection::default(),
            filter: ProcessFilter::default(),
            search_draft: None,
            view: View::default(),
            help_open: false,
            sidebar_requested: true,
            history_window_ms: HISTORY_WINDOWS_MS[0],
            history_reload: true,
            sample_interval_ms: interval_ms,
            now_ms: 0,
            sampled_at_ms: 0,
            failure_count: 0,
            error: None,
            events: Vec::new(),
            source_label: "Live".into(),
            latest_snapshot: None,
            frames: Frames::TooSmall,
            panel_lines: Vec::new(),
            panel_scroll: 0,
            process_rows: 0,
            panel_rows: 0,
        }
    }

    pub fn with_options(mut self, selection: Selection, history_window_ms: u64) -> Self {
        self.filter.include_command = selection.include_command;
        self.selection = selection;
        self.history_window_ms = nearest_window(history_window_ms);
        self
    }

    pub fn run(&mut self, terminal: &mut Tui, feed: &mut impl Feed) -> anyhow::Result<()> {
        while !self.exit {
            // Only the background cache is read here; driver latency never blocks input.
            self.now_ms = feed.now_ms();
            self.sample_interval_ms = feed.interval_ms();
            self.source_label = feed.label();
            let due = self
                .last_refresh
                .is_none_or(|last| last.elapsed() >= self.interval.min(Duration::from_millis(100)));
            if due {
                match feed.poll() {
                    Ok(snapshots) => {
                        for snapshot in snapshots {
                            self.apply_snapshot(snapshot);
                        }
                    }
                    Err(error) => self.error = Some(error),
                }
                self.events = feed
                    .events()
                    .into_iter()
                    .filter(|event| self.selection.matches_event(event))
                    .collect();
                self.last_refresh = Some(Instant::now());
            }
            if self.history_reload {
                // Read the longest retained series once, then follow it incrementally.
                if let Some(history) = feed.history(retention_ms()) {
                    self.apply_history(history);
                }
                self.history_reload = false;
            }
            terminal.draw(|frame| {
                let area = frame.area();
                self.measure(area);
                render::draw(frame, self);
            })?;
            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    if let Some(action) = keymap::resolve(key, self.is_editing_search()) {
                        if self.apply(action) == Effect::Retry {
                            if let Err(error) = feed.retry() {
                                self.error = Some(error);
                            }
                            self.last_refresh = None;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    // -- measurement ------------------------------------------------------

    /// Resolves the frame and every viewport before a single cell is drawn.
    pub fn measure(&mut self, area: Rect) {
        self.frames = layout::solve(area, self.sidebar_requested);
        let Frames::Ready(chrome) = self.frames else {
            self.process_rows = 0;
            self.panel_rows = 0;
            self.panel_lines.clear();
            return;
        };
        self.process_rows = (chrome.body.height as usize).saturating_sub(PROCESS_CHROME_ROWS);
        self.panel_rows = chrome.body.height as usize;
        self.panel_lines = if self.view.is_text_pane() {
            format::wrap_lines(&self.pane_text(), chrome.body.width as usize)
        } else {
            Vec::new()
        };
        self.clamp_scroll();
    }

    pub fn chrome(&self) -> Option<Chrome> {
        match self.frames {
            Frames::Ready(chrome) => Some(chrome),
            Frames::TooSmall => None,
        }
    }

    pub fn panel_lines(&self) -> &[String] {
        &self.panel_lines
    }

    pub fn panel_scroll(&self) -> usize {
        self.panel_scroll
    }

    pub fn process_rows(&self) -> usize {
        self.process_rows
    }

    /// Whether the sidebar is both wanted and affordable at the current size.
    pub fn sidebar_visible(&self) -> bool {
        self.chrome().is_some_and(|chrome| chrome.sidebar.is_some())
    }

    fn pane_text(&self) -> String {
        match self.view {
            View::Diagnostics => self
                .latest_snapshot
                .as_ref()
                .map(output::diagnostics_text)
                .unwrap_or_else(|| {
                    self.error.clone().unwrap_or_else(|| {
                        "Waiting for the first sample. Press r to retry now.".into()
                    })
                }),
            View::Alerts => self.alerts_text(),
            _ => String::new(),
        }
    }

    fn alerts_text(&self) -> String {
        if self.events.is_empty() {
            return "No alert events in this session.\n\n\
                    Enable monitoring with --alerts, or use the alerts command to set \
                    explicit thresholds. Alerts require a reading to stay past its \
                    threshold for a sustained period, and clear only at a separate \
                    recovery limit."
                .into();
        }
        self.events
            .iter()
            .rev()
            .map(|event| output::alert_panel_line(event, self.now_ms))
            .collect::<Vec<_>>()
            .join("\n")
    }

    // -- intent -----------------------------------------------------------

    pub fn is_editing_search(&self) -> bool {
        self.search_draft.is_some()
    }

    /// The query as currently displayed, whether committed or being typed.
    pub fn search_text(&self) -> &str {
        self.search_draft.as_deref().unwrap_or(&self.filter.query)
    }

    pub fn apply(&mut self, action: Action) -> Effect {
        // Any key dismisses help, so it can never trap the interface.
        if self.help_open && action != Action::Quit {
            self.help_open = false;
            if action == Action::ToggleHelp {
                return Effect::None;
            }
        }
        match action {
            Action::Quit => self.exit = true,
            Action::Retry => return Effect::Retry,
            Action::ToggleHelp => self.help_open = !self.help_open,
            Action::ToggleSidebar => self.sidebar_requested = !self.sidebar_requested,
            Action::ToggleCommandColumn => {
                self.filter.include_command = !self.filter.include_command;
                self.clamp_scroll();
            }
            Action::NextDevice => self.select_relative(true),
            Action::PreviousDevice => self.select_relative(false),
            Action::GoToView(view) => self.set_view(view),
            Action::CycleView(forward) => self.set_view(self.view.cycle(forward)),
            Action::Scroll(scroll) => self.scroll(scroll),
            Action::CycleProcessSort => {
                self.filter.sort = self.filter.sort.next();
                self.set_view(View::Processes);
            }
            Action::CycleHistoryWindow => {
                let position = HISTORY_WINDOWS_MS
                    .iter()
                    .position(|window| *window == self.history_window_ms)
                    .unwrap_or_default();
                self.history_window_ms =
                    HISTORY_WINDOWS_MS[(position + 1) % HISTORY_WINDOWS_MS.len()];
            }
            Action::Search(edit) => self.edit_search(edit),
        }
        Effect::None
    }

    fn set_view(&mut self, view: View) {
        if self.view != view {
            self.view = view;
            self.panel_scroll = 0;
        }
    }

    fn edit_search(&mut self, edit: SearchEdit) {
        match edit {
            SearchEdit::Begin => {
                // A query is only meaningful beside the list it filters.
                if !self.view.accepts_search() {
                    self.set_view(View::Processes);
                }
                self.search_draft = Some(self.filter.query.clone());
            }
            SearchEdit::Push(character) => {
                if let Some(draft) = &mut self.search_draft {
                    draft.push(character);
                }
            }
            SearchEdit::Pop => {
                if let Some(draft) = &mut self.search_draft {
                    draft.pop();
                }
            }
            SearchEdit::Commit => {
                if let Some(draft) = self.search_draft.take() {
                    self.filter.query = draft;
                }
            }
            // Cancelling restores the filter that was active before editing began.
            SearchEdit::Cancel => self.search_draft = None,
        }
        self.reset_process_scroll();
    }

    fn scroll(&mut self, scroll: Scroll) {
        if self.view.is_text_pane() {
            let page = self.panel_rows.max(1);
            let max = self.panel_lines.len().saturating_sub(page);
            self.panel_scroll = match scroll {
                Scroll::LineUp => self.panel_scroll.saturating_sub(1),
                Scroll::LineDown => self.panel_scroll.saturating_add(1).min(max),
                Scroll::PageUp => self.panel_scroll.saturating_sub(page),
                Scroll::PageDown => self.panel_scroll.saturating_add(page).min(max),
                Scroll::Top => 0,
                Scroll::Bottom => max,
            };
            return;
        }
        if self.view != View::Processes {
            // Nothing scrolls here, so vertical keys keep their device meaning.
            match scroll {
                Scroll::LineDown | Scroll::PageDown => self.select_relative(true),
                Scroll::LineUp | Scroll::PageUp => self.select_relative(false),
                Scroll::Top => self.select_edge(false),
                Scroll::Bottom => self.select_edge(true),
            }
            return;
        }
        let page = self.process_rows.max(1);
        let max = self.visible_process_count().saturating_sub(page);
        let rows = self.process_rows;
        if let Some(view) = self.selected_view_mut() {
            if rows == 0 {
                return;
            }
            view.process_scroll = match scroll {
                Scroll::LineUp => view.process_scroll.saturating_sub(1),
                Scroll::LineDown => view.process_scroll.saturating_add(1).min(max),
                Scroll::PageUp => view.process_scroll.saturating_sub(page),
                Scroll::PageDown => view.process_scroll.saturating_add(page).min(max),
                Scroll::Top => 0,
                Scroll::Bottom => max,
            };
        }
    }

    fn select_relative(&mut self, forward: bool) {
        if self.order.is_empty() {
            return;
        }
        let position = self.selected_position();
        let next = if forward {
            (position + 1) % self.order.len()
        } else {
            (position + self.order.len() - 1) % self.order.len()
        };
        self.selected = Some(self.order[next].clone());
        self.clamp_scroll();
    }

    fn select_edge(&mut self, last: bool) {
        let edge = if last {
            self.order.last()
        } else {
            self.order.first()
        };
        if let Some(key) = edge.cloned() {
            self.selected = Some(key);
            self.clamp_scroll();
        }
    }

    // -- data -------------------------------------------------------------

    fn apply_history(&mut self, history: HistoryResponse) {
        // A backward clock step starts a new series rather than joining across it.
        let start = history
            .frames
            .windows(2)
            .enumerate()
            .filter(|(_, pair)| pair[1].sampled_at_ms < pair[0].sampled_at_ms)
            .map(|(index, _)| index + 1)
            .next_back()
            .unwrap_or(0);
        self.sample_interval_ms = history.interval_ms.max(MIN_INTERVAL_MS);
        for (uuid, view) in &mut self.views {
            view.replace_history(history.frames.iter().skip(start).map(|frame| {
                let gpu = frame.gpus.iter().find(|gpu| gpu.uuid == *uuid);
                HistoryPoint {
                    sampled_at_ms: frame.sampled_at_ms,
                    gpu: gpu.and_then(|gpu| gpu.gpu_utilization).map(u64::from),
                    memory: gpu
                        .and_then(|gpu| gpu.memory_percent)
                        .map(|percent| percent as u64),
                    temperature: gpu.and_then(|gpu| gpu.temperature).map(u64::from),
                    power: gpu
                        .and_then(|gpu| gpu.power_watts)
                        .map(|watts| watts as u64),
                }
            }));
        }
    }

    pub fn apply_snapshot(&mut self, snapshot: MonitorSnapshot) {
        if snapshot.sampled_at_ms < self.sampled_at_ms {
            // The clock moved back: keep sampling, but do not draw a line across it.
            for view in self.views.values_mut() {
                view.history.clear();
            }
            self.now_ms = snapshot.sampled_at_ms;
        }
        let snapshot = self.selection.apply(&snapshot);
        self.latest_snapshot = Some(snapshot.clone());
        self.now_ms = self.now_ms.max(snapshot.sampled_at_ms);
        self.sampled_at_ms = snapshot.sampled_at_ms;
        self.failure_count = snapshot.failures.len();
        self.error = snapshot.error.map(|error| error.message);
        let mut present = HashSet::new();
        for gpu in snapshot.gpus {
            let key = gpu.device.uuid.clone();
            present.insert(key.clone());
            self.adopt_provisional_identity(&key, gpu.device.index);
            let view = self.views.entry(key).or_default();
            view.index = gpu.device.index;
            view.error = None;
            view.push_history(HistoryPoint::from_sample(&gpu));
            view.gpu = Some(gpu);
        }
        for failure in snapshot.failures {
            let key = failure
                .uuid
                .or_else(|| {
                    self.views
                        .iter()
                        .find(|(_, view)| view.index == failure.index)
                        .map(|(key, _)| key.clone())
                })
                .unwrap_or_else(|| provisional_key(failure.index));
            present.insert(key.clone());
            let view = self.views.entry(key).or_default();
            view.index = failure.index;
            view.error = Some(failure.error.message);
            view.push_history(HistoryPoint::missing(snapshot.sampled_at_ms));
        }
        if let Some(error) = &self.error {
            // A global failure marks known devices stale instead of forgetting them.
            for (key, view) in &mut self.views {
                if !present.contains(key) {
                    view.error = Some(error.clone());
                    view.push_history(HistoryPoint::missing(snapshot.sampled_at_ms));
                }
            }
        } else {
            self.views.retain(|key, _| present.contains(key));
        }
        self.reorder();
        self.clamp_scroll();
    }

    /// A device first seen as a bare index keeps its history once its UUID arrives.
    fn adopt_provisional_identity(&mut self, uuid: &str, index: u32) {
        let provisional = provisional_key(index);
        if let Some(previous) = self.views.remove(&provisional) {
            self.views.entry(uuid.to_owned()).or_insert(previous);
            if self.selected.as_deref() == Some(provisional.as_str()) {
                self.selected = Some(uuid.to_owned());
            }
        }
    }

    fn reorder(&mut self) {
        self.order = self.views.keys().cloned().collect();
        self.order.sort_by(|a, b| {
            let left = &self.views[a];
            let right = &self.views[b];
            match (&left.gpu, &right.gpu) {
                (Some(left), Some(right)) => self.selection.compare(left, right),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => left.index.cmp(&right.index).then(a.cmp(b)),
            }
        });
        if self
            .selected
            .as_ref()
            .is_none_or(|key| !self.views.contains_key(key))
        {
            self.selected = self.order.first().cloned();
        }
    }

    // -- queries ----------------------------------------------------------

    pub fn ordered_views(&self) -> impl Iterator<Item = &DeviceView> {
        self.order.iter().filter_map(|uuid| self.views.get(uuid))
    }

    pub fn selected_view(&self) -> Option<&DeviceView> {
        self.selected.as_ref().and_then(|key| self.views.get(key))
    }

    fn selected_view_mut(&mut self) -> Option<&mut DeviceView> {
        self.selected
            .as_ref()
            .and_then(|key| self.views.get_mut(key))
    }

    pub fn selected_position(&self) -> usize {
        self.selected
            .as_ref()
            .and_then(|key| self.order.iter().position(|candidate| candidate == key))
            .unwrap_or(0)
    }

    pub fn device_count(&self) -> usize {
        self.order.len()
    }

    pub fn visible_process_count(&self) -> usize {
        self.selected_view()
            .map_or(0, |view| view.processes(&self.filter).len())
    }

    /// A device is stale when it failed, when the monitor failed, or when its last
    /// good sample is older than several acquisition intervals.
    pub fn is_stale(&self, view: &DeviceView) -> bool {
        view.error.is_some()
            || self.error.is_some()
            || view.gpu.as_ref().is_none_or(|gpu| {
                self.now_ms.abs_diff(gpu.sampled_at_ms)
                    > self.sample_interval_ms.saturating_mul(3).max(3000)
            })
    }

    fn reset_process_scroll(&mut self) {
        if let Some(view) = self.selected_view_mut() {
            view.process_scroll = 0;
        }
    }

    fn clamp_scroll(&mut self) {
        let page = self.process_rows.max(1);
        let count = self.visible_process_count();
        if let Some(view) = self.selected_view_mut() {
            view.process_scroll = view.process_scroll.min(count.saturating_sub(page));
        }
        let panel_page = self.panel_rows.max(1);
        self.panel_scroll = self
            .panel_scroll
            .min(self.panel_lines.len().saturating_sub(panel_page));
    }
}

fn provisional_key(index: u32) -> String {
    format!("index:{index}")
}

fn retention_ms() -> u64 {
    HISTORY_WINDOWS_MS.iter().copied().max().unwrap_or(60_000)
}

/// Accepts any requested window by snapping it to the closest offered one, so a
/// value from the command line and a value chosen with `w` are always the same set.
fn nearest_window(requested_ms: u64) -> u64 {
    HISTORY_WINDOWS_MS
        .into_iter()
        .min_by_key(|window| window.abs_diff(requested_ms))
        .unwrap_or(60_000)
}
