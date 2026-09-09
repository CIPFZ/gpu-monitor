//! Sampling, device navigation and per-device view state.

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use gpu_monitor_core::{GpuInfo, MonitorService, MonitorSnapshot};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    time::{Duration, Instant},
};

use crate::{tui::Tui, ui};

pub const MIN_INTERVAL_MS: u64 = 100;
const HISTORY_SIZE: usize = 60;

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
    fn push_history(&mut self, point: HistoryPoint) {
        // A snapshot may be delivered more than once, but each acquisition is one sample.
        if self
            .history
            .back()
            .is_some_and(|last| last.sampled_at_ms == point.sampled_at_ms)
        {
            return;
        }
        self.history.push_back(point);
        if self.history.len() > HISTORY_SIZE {
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
        }
    }

    pub fn run(&mut self, terminal: &mut Tui, monitor: &mut MonitorService) -> anyhow::Result<()> {
        while !self.exit {
            if self
                .last_refresh
                .is_none_or(|last| last.elapsed() >= self.interval)
            {
                self.apply_snapshot(monitor.sample());
                self.last_refresh = Some(Instant::now());
            }
            terminal.draw(|frame| ui::draw(frame, self))?;
            let timeout = self.last_refresh.map_or(Duration::ZERO, |last| {
                self.interval
                    .saturating_sub(last.elapsed())
                    .min(Duration::from_millis(100))
            });
            if event::poll(timeout)? {
                if let Event::Key(key) = event::read()? {
                    if self.handle_key(key) {
                        monitor.retry();
                        self.last_refresh = None;
                    }
                }
            }
        }
        Ok(())
    }

    pub fn apply_snapshot(&mut self, snapshot: MonitorSnapshot) {
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
            view.push_history(HistoryPoint {
                sampled_at_ms: gpu.sampled_at_ms,
                gpu: gpu.metrics.gpu_utilization.map(u64::from),
                memory: gpu
                    .memory
                    .as_ref()
                    .map(|memory| memory.usage_percent() as u64),
            });
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
            view.push_history(HistoryPoint {
                sampled_at_ms: snapshot.sampled_at_ms,
                gpu: None,
                memory: None,
            });
        }
        if let Some(error) = &self.error {
            for (key, view) in &mut self.views {
                if !present.contains(key) {
                    view.error = Some(error.clone());
                    view.push_history(HistoryPoint {
                        sampled_at_ms: snapshot.sampled_at_ms,
                        gpu: None,
                        memory: None,
                    });
                }
            }
        } else {
            self.views.retain(|key, _| present.contains(key));
        }
        self.order = self.views.keys().cloned().collect();
        self.order
            .sort_by_key(|key| (self.views[key].index, key.clone()));
        if self
            .selected
            .as_ref()
            .is_none_or(|key| !self.views.contains_key(key))
        {
            self.selected = self.order.first().cloned();
        }
        self.clamp_scroll();
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
            KeyCode::Right | KeyCode::Tab | KeyCode::Char(']') => self.select_relative(true),
            KeyCode::Left | KeyCode::BackTab | KeyCode::Char('[') => self.select_relative(false),
            code => {
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
