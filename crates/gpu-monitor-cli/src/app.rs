//! Sampling, device navigation and per-device view state.

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use gpu_monitor_core::{GpuInfo, MonitorSnapshot};
use gpu_monitor_runtime::{AlertEvent, HistoryResponse};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    time::{Duration, Instant},
};

use crate::{feed::Feed, selection::Selection, tui::Tui, ui};

pub const MIN_INTERVAL_MS: u64 = 100;
const DEFAULT_HISTORY_MS: u64 = 60_000;

#[derive(Debug, Clone)]
pub struct HistoryPoint {
    pub sampled_at_ms: u64,
    pub gpu: Option<u64>,
    pub memory: Option<u64>,
}

#[derive(Default)]
pub struct DeviceView {
    pub index: u32,
    pub gpu: Option<GpuInfo>,
    pub error: Option<String>,
    pub history: VecDeque<HistoryPoint>,
    pub process_scroll: usize,
}

impl DeviceView {
    fn push_history(&mut self, point: HistoryPoint, window_ms: u64) {
        // A snapshot may be delivered more than once, but each acquisition is one sample.
        if self
            .history
            .back()
            .is_some_and(|last| last.sampled_at_ms >= point.sampled_at_ms)
        {
            return;
        }
        let cutoff = point.sampled_at_ms.saturating_sub(window_ms);
        self.history.push_back(point);
        while self
            .history
            .front()
            .is_some_and(|first| first.sampled_at_ms < cutoff)
        {
            self.history.pop_front();
        }
    }

    pub fn process_count(&self) -> usize {
        self.gpu.as_ref().map_or(0, |gpu| gpu.processes.len())
    }
}

pub struct App {
    exit: bool,
    interval: Duration,
    last_refresh: Option<Instant>,
    views: HashMap<String, DeviceView>,
    order: Vec<String>,
    selected: Option<String>,
    pub error: Option<String>,
    pub failure_count: usize,
    pub sampled_at_ms: u64,
    visible_process_rows: usize,
    pub selection: Selection,
    pub history_window_ms: u64,
    pub sample_interval_ms: u64,
    pub now_ms: u64,
    pub overview: bool,
    pub diagnostics: bool,
    pub show_events: bool,
    pub events: Vec<AlertEvent>,
    pub source_label: String,
    pub latest_snapshot: Option<MonitorSnapshot>,
    pub panel_scroll: u16,
    panel_max_scroll: u16,
}

impl App {
    pub fn new(interval_ms: u64) -> Self {
        Self {
            exit: false,
            interval: Duration::from_millis(interval_ms.max(MIN_INTERVAL_MS)),
            last_refresh: None,
            views: HashMap::new(),
            order: Vec::new(),
            selected: None,
            error: None,
            failure_count: 0,
            sampled_at_ms: 0,
            visible_process_rows: 0,
            selection: Selection::default(),
            history_window_ms: DEFAULT_HISTORY_MS,
            sample_interval_ms: interval_ms.max(MIN_INTERVAL_MS),
            now_ms: 0,
            overview: false,
            diagnostics: false,
            show_events: false,
            events: vec![],
            source_label: "Live".into(),
            latest_snapshot: None,
            panel_scroll: 0,
            panel_max_scroll: 0,
        }
    }

    pub fn with_options(mut self, selection: Selection, history_window_ms: u64) -> Self {
        self.selection = selection;
        self.history_window_ms = history_window_ms;
        self
    }

    pub fn run(&mut self, terminal: &mut Tui, feed: &mut impl Feed) -> anyhow::Result<()> {
        while !self.exit {
            // Poll only the background cache. Driver latency never blocks input handling.
            self.now_ms = feed.now_ms();
            self.sample_interval_ms = feed.interval_ms();
            self.source_label = feed.label();
            if self
                .last_refresh
                .is_none_or(|last| last.elapsed() >= self.interval.min(Duration::from_millis(100)))
            {
                match feed.poll() {
                    Ok(snapshots) => {
                        let changed = !snapshots.is_empty();
                        for snapshot in snapshots {
                            self.apply_snapshot(snapshot);
                        }
                        if changed {
                            if let Some(history) = feed.history(self.history_window_ms) {
                                self.apply_history(history);
                            }
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
            terminal.draw(|frame| ui::draw(frame, self))?;
            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    if self.handle_key(key) {
                        if let Err(error) = feed.retry() {
                            self.error = Some(error);
                        }
                        self.last_refresh = None;
                    }
                }
            }
        }
        Ok(())
    }

    fn apply_history(&mut self, history: HistoryResponse) {
        let start = history
            .frames
            .windows(2)
            .enumerate()
            .filter(|(_, pair)| pair[1].sampled_at_ms < pair[0].sampled_at_ms)
            .map(|(index, _)| index + 1)
            .last()
            .unwrap_or(0);
        self.sample_interval_ms = history.interval_ms;
        for (uuid, view) in &mut self.views {
            view.history = history
                .frames
                .iter()
                .skip(start)
                .map(|frame| {
                    let gpu = frame.gpus.iter().find(|gpu| gpu.uuid == *uuid);
                    HistoryPoint {
                        sampled_at_ms: frame.sampled_at_ms,
                        gpu: gpu.and_then(|gpu| gpu.gpu_utilization).map(u64::from),
                        memory: gpu
                            .and_then(|gpu| gpu.memory_percent)
                            .map(|value| value as u64),
                    }
                })
                .collect();
        }
    }

    pub fn ordered_views(&self) -> impl Iterator<Item = &DeviceView> {
        self.order.iter().filter_map(|uuid| self.views.get(uuid))
    }

    pub fn apply_snapshot(&mut self, snapshot: MonitorSnapshot) {
        if snapshot.sampled_at_ms < self.sampled_at_ms {
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
            let fallback = format!("index:{}", gpu.device.index);
            if let Some(previous) = self.views.remove(&fallback) {
                self.views.entry(key.clone()).or_insert(previous);
                if self.selected.as_deref() == Some(&fallback) {
                    self.selected = Some(key.clone());
                }
            }
            let view = self.views.entry(key).or_default();
            view.index = gpu.device.index;
            view.error = None;
            view.push_history(
                HistoryPoint {
                    sampled_at_ms: gpu.sampled_at_ms,
                    gpu: gpu.metrics.gpu_utilization.map(u64::from),
                    memory: gpu
                        .memory
                        .as_ref()
                        .map(|memory| memory.usage_percent() as u64),
                },
                self.history_window_ms,
            );
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
                .unwrap_or_else(|| format!("index:{}", failure.index));
            present.insert(key.clone());
            let view = self.views.entry(key).or_default();
            view.index = failure.index;
            view.error = Some(failure.error.message);
            view.push_history(
                HistoryPoint {
                    sampled_at_ms: snapshot.sampled_at_ms,
                    gpu: None,
                    memory: None,
                },
                self.history_window_ms,
            );
        }
        if let Some(error) = &self.error {
            for (key, view) in &mut self.views {
                if !present.contains(key) {
                    view.error = Some(error.clone());
                    view.push_history(
                        HistoryPoint {
                            sampled_at_ms: snapshot.sampled_at_ms,
                            gpu: None,
                            memory: None,
                        },
                        self.history_window_ms,
                    );
                }
            }
        } else {
            self.views.retain(|key, _| present.contains(key));
        }
        self.order = self.views.keys().cloned().collect();
        self.order.sort_by(|a, b| {
            let a_view = &self.views[a];
            let b_view = &self.views[b];
            match (&a_view.gpu, &b_view.gpu) {
                (Some(a), Some(b)) => self.selection.compare(a, b),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => a_view.index.cmp(&b_view.index).then(a.cmp(b)),
            }
        });
        if self
            .selected
            .as_ref()
            .is_none_or(|key| !self.views.contains_key(key))
        {
            self.selected = self.order.first().cloned();
        }
        self.clamp_scroll();
    }

    pub fn is_stale(&self, view: &DeviceView) -> bool {
        view.error.is_some()
            || self.error.is_some()
            || view.gpu.as_ref().is_none_or(|gpu| {
                self.now_ms.abs_diff(gpu.sampled_at_ms)
                    > self.sample_interval_ms.saturating_mul(3).max(3000)
            })
    }

    pub fn set_panel_viewport(&mut self, lines: usize, height: u16) {
        self.panel_max_scroll = lines.saturating_sub(height as usize).min(u16::MAX as usize) as u16;
        self.panel_scroll = self.panel_scroll.min(self.panel_max_scroll);
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

    // Called with the actual table body height, after layout, before rendering rows.
    pub fn set_process_viewport(&mut self, rows: usize) {
        self.visible_process_rows = rows;
        self.clamp_scroll();
    }

    fn clamp_scroll(&mut self) {
        let visible = self.visible_process_rows.max(1);
        if let Some(view) = self.selected_view_mut() {
            view.process_scroll = view
                .process_scroll
                .min(view.process_count().saturating_sub(visible));
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

    /// Returns true when the user requests immediate reinitialization.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if key.kind == KeyEventKind::Release {
            return false;
        }
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.exit = true;
            return false;
        }
        let page = self.visible_process_rows.max(1);
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.exit = true,
            KeyCode::Char('r') => return true,
            KeyCode::Char('t') => {
                self.overview = !self.overview;
                self.diagnostics = false;
                self.show_events = false;
            }
            KeyCode::Char('d') => {
                self.diagnostics = !self.diagnostics;
                self.show_events = false;
                self.panel_scroll = 0;
            }
            KeyCode::Char('a') => {
                self.show_events = !self.show_events;
                self.diagnostics = false;
                self.panel_scroll = 0;
            }
            KeyCode::Right | KeyCode::Tab | KeyCode::Char(']') => self.select_relative(true),
            KeyCode::Left | KeyCode::BackTab | KeyCode::Char('[') => self.select_relative(false),
            code => {
                if self.diagnostics || self.show_events {
                    self.panel_scroll = match code {
                        KeyCode::Up | KeyCode::Char('k') => self.panel_scroll.saturating_sub(1),
                        KeyCode::Down | KeyCode::Char('j') => self
                            .panel_scroll
                            .saturating_add(1)
                            .min(self.panel_max_scroll),
                        KeyCode::Home => 0,
                        KeyCode::End => self.panel_max_scroll,
                        KeyCode::PageUp => self.panel_scroll.saturating_sub(10),
                        KeyCode::PageDown => self
                            .panel_scroll
                            .saturating_add(10)
                            .min(self.panel_max_scroll),
                        _ => self.panel_scroll,
                    };
                    return false;
                }
                if self.overview {
                    match code {
                        KeyCode::Down | KeyCode::Char('j') => self.select_relative(true),
                        KeyCode::Up | KeyCode::Char('k') => self.select_relative(false),
                        KeyCode::Enter => self.overview = false,
                        _ => {}
                    }
                    return false;
                }
                if self.visible_process_rows == 0 {
                    return false;
                }
                if let Some(view) = self.selected_view_mut() {
                    let max = view.process_count().saturating_sub(page);
                    view.process_scroll = match code {
                        KeyCode::Up | KeyCode::Char('k') => view.process_scroll.saturating_sub(1),
                        KeyCode::Down | KeyCode::Char('j') => {
                            view.process_scroll.saturating_add(1).min(max)
                        }
                        KeyCode::PageUp => view.process_scroll.saturating_sub(page),
                        KeyCode::PageDown => view.process_scroll.saturating_add(page).min(max),
                        KeyCode::Home => 0,
                        KeyCode::End => max,
                        _ => view.process_scroll,
                    };
                }
            }
        }
        false
    }
}
