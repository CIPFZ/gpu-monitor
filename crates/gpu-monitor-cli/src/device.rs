//! Per-device view state: the last good sample, the current failure reason, the
//! chart history and the scroll position that belongs to that device alone.
//!
//! History is retained for the longest offered window rather than the selected
//! one, so changing the window at runtime reveals data that was already
//! collected instead of restarting the series from empty.

use gpu_monitor_core::{GpuInfo, GpuProcess};
use std::collections::VecDeque;

use crate::{format, keymap::HISTORY_WINDOWS_MS};

/// Matches the runtime's own frame ceiling so a fast interval cannot grow the
/// per-device buffers without bound.
const MAX_HISTORY_FRAMES: usize = 36_001;

/// One sample of the charted metrics. `None` is a missing acquisition, never zero.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HistoryPoint {
    pub sampled_at_ms: u64,
    pub gpu: Option<u64>,
    pub memory: Option<u64>,
    pub temperature: Option<u64>,
    pub power: Option<u64>,
}

impl HistoryPoint {
    /// A frame that records only that an acquisition failed at this time.
    pub fn missing(sampled_at_ms: u64) -> Self {
        Self {
            sampled_at_ms,
            ..Self::default()
        }
    }

    pub fn from_sample(gpu: &GpuInfo) -> Self {
        Self {
            sampled_at_ms: gpu.sampled_at_ms,
            gpu: gpu.metrics.gpu_utilization.map(u64::from),
            memory: format::memory_ratio(gpu.memory.as_ref()).map(|percent| percent as u64),
            temperature: gpu.metrics.temperature.map(u64::from),
            power: gpu.metrics.power_watts().map(|watts| watts as u64),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProcessSort {
    /// Descending; an unknown allocation sorts last rather than as zero.
    #[default]
    Memory,
    Pid,
    /// Descending, so the longest running job is first.
    Elapsed,
    Name,
}

impl ProcessSort {
    pub const ALL: [Self; 4] = [Self::Memory, Self::Pid, Self::Elapsed, Self::Name];

    pub fn label(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::Pid => "pid",
            Self::Elapsed => "elapsed",
            Self::Name => "name",
        }
    }

    pub fn next(self) -> Self {
        let position = Self::ALL
            .iter()
            .position(|sort| *sort == self)
            .unwrap_or_default();
        Self::ALL[(position + 1) % Self::ALL.len()]
    }
}

/// Presentation-only process selection. It never changes what was sampled.
#[derive(Clone, Debug, Default)]
pub struct ProcessFilter {
    pub sort: ProcessSort,
    pub query: String,
    pub include_command: bool,
}

impl ProcessFilter {
    pub fn is_active(&self) -> bool {
        !self.query.trim().is_empty()
    }

    pub fn matches(&self, process: &GpuProcess) -> bool {
        let query = self.query.trim().to_lowercase();
        if query.is_empty() {
            return true;
        }
        if process.name.to_lowercase().contains(&query)
            || process.pid.to_string().contains(&query)
            || format::process_owner(process)
                .to_lowercase()
                .contains(&query)
        {
            return true;
        }
        // Arguments are only searchable when the operator chose to display them.
        self.include_command
            && process.command.as_ref().is_some_and(|command| {
                command
                    .iter()
                    .any(|argument| argument.to_lowercase().contains(&query))
            })
    }
}

#[derive(Default)]
pub struct DeviceView {
    pub index: u32,
    pub gpu: Option<GpuInfo>,
    /// Reason the most recent acquisition failed, if it did.
    pub error: Option<String>,
    pub history: VecDeque<HistoryPoint>,
    pub process_scroll: usize,
}

impl DeviceView {
    /// A snapshot may be delivered more than once, but each acquisition is one
    /// sample, so an already recorded time is not appended again.
    pub fn push_history(&mut self, point: HistoryPoint) {
        if self
            .history
            .back()
            .is_some_and(|last| last.sampled_at_ms >= point.sampled_at_ms)
        {
            return;
        }
        let retention_ms = HISTORY_WINDOWS_MS.iter().copied().max().unwrap_or(60_000);
        let cutoff = point.sampled_at_ms.saturating_sub(retention_ms);
        self.history.push_back(point);
        while self
            .history
            .front()
            .is_some_and(|first| first.sampled_at_ms < cutoff)
            || self.history.len() > MAX_HISTORY_FRAMES
        {
            self.history.pop_front();
        }
    }

    pub fn replace_history(&mut self, points: impl IntoIterator<Item = HistoryPoint>) {
        self.history = points.into_iter().collect();
    }

    /// Total processes reported for this device, before any display filter.
    pub fn sampled_process_count(&self) -> usize {
        self.gpu.as_ref().map_or(0, |gpu| gpu.processes.len())
    }

    pub fn processes<'a>(&'a self, filter: &ProcessFilter) -> Vec<&'a GpuProcess> {
        let mut processes: Vec<&GpuProcess> = self
            .gpu
            .as_ref()
            .map(|gpu| {
                gpu.processes
                    .iter()
                    .filter(|process| filter.matches(process))
                    .collect()
            })
            .unwrap_or_default();
        sort_processes(&mut processes, filter.sort);
        processes
    }

    /// True when the driver reported the process query as partially unavailable.
    pub fn processes_incomplete(&self) -> bool {
        self.gpu.as_ref().is_some_and(|gpu| {
            gpu.issues
                .iter()
                .any(|issue| issue.metric.contains("process"))
        })
    }
}

fn sort_processes(processes: &mut [&GpuProcess], sort: ProcessSort) {
    processes.sort_by(|a, b| {
        let primary = match sort {
            ProcessSort::Memory => descending(a.gpu_memory, b.gpu_memory),
            ProcessSort::Pid => a.pid.cmp(&b.pid),
            ProcessSort::Elapsed => descending(a.elapsed_seconds, b.elapsed_seconds),
            ProcessSort::Name => a.name.cmp(&b.name),
        };
        primary.then(a.pid.cmp(&b.pid))
    });
}

/// Larger first, with an unavailable reading last instead of treated as zero.
fn descending<T: Ord>(a: Option<T>, b: Option<T>) -> std::cmp::Ordering {
    match (a, b) {
        (Some(a), Some(b)) => b.cmp(&a),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn process(pid: u32, name: &str, memory: Option<u64>, elapsed: Option<u64>) -> GpuProcess {
        GpuProcess {
            pid,
            name: name.to_owned(),
            gpu_memory: memory,
            elapsed_seconds: elapsed,
            ..GpuProcess::default()
        }
    }

    fn view(processes: Vec<GpuProcess>) -> DeviceView {
        let mut gpu: GpuInfo = serde_json::from_value(json!({
            "device": {"index": 0, "name": "GPU", "uuid": "uuid-0", "pci_bus_id": "0",
                       "driver_version": "test", "cuda_version": null,
                       "power_limit": 250, "power_limit_max": 300},
            "metrics": {}, "memory": null, "processes": [], "sampled_at_ms": 1, "issues": []
        }))
        .unwrap();
        gpu.processes = processes;
        DeviceView {
            gpu: Some(gpu),
            ..DeviceView::default()
        }
    }

    #[test]
    fn an_unknown_allocation_sorts_last_instead_of_counting_as_zero() {
        let view = view(vec![
            process(3, "c", None, None),
            process(1, "a", Some(10), None),
            process(2, "b", Some(90), None),
        ]);
        let filter = ProcessFilter::default();
        assert_eq!(
            view.processes(&filter)
                .iter()
                .map(|process| process.pid)
                .collect::<Vec<_>>(),
            [2, 1, 3]
        );
    }

    #[test]
    fn every_sort_order_is_reachable_and_stable_on_ties() {
        let view = view(vec![
            process(9, "b", Some(5), Some(10)),
            process(4, "a", Some(5), Some(10)),
        ]);
        let mut sort = ProcessSort::default();
        let mut seen = vec![sort];
        for _ in 1..ProcessSort::ALL.len() {
            sort = sort.next();
            seen.push(sort);
        }
        assert_eq!(seen, ProcessSort::ALL.to_vec());
        assert_eq!(sort.next(), ProcessSort::default(), "the order cycles");
        for sort in ProcessSort::ALL {
            let filter = ProcessFilter {
                sort,
                ..ProcessFilter::default()
            };
            let order = view
                .processes(&filter)
                .iter()
                .map(|process| process.pid)
                .collect::<Vec<_>>();
            assert_eq!(order, [4, 9], "ties fall back to the PID for {sort:?}");
        }
    }

    #[test]
    fn the_query_matches_identity_but_reaches_arguments_only_when_shown() {
        let mut secret = process(77, "python", Some(1), None);
        secret.user = Some("alice".into());
        secret.command = Some(vec!["python".into(), "--dataset=imagenet".into()]);
        let view = view(vec![secret, process(78, "renderer", Some(1), None)]);
        let mut filter = ProcessFilter {
            query: "imagenet".into(),
            ..ProcessFilter::default()
        };
        assert!(
            view.processes(&filter).is_empty(),
            "hidden arguments must not be searchable"
        );
        filter.include_command = true;
        assert_eq!(view.processes(&filter).len(), 1);
        for query in ["ALICE", "77", "render"] {
            filter.query = query.into();
            assert_eq!(view.processes(&filter).len(), 1, "query {query} failed");
        }
        filter.query = "   ".into();
        assert!(!filter.is_active());
        assert_eq!(view.processes(&filter).len(), 2);
    }

    #[test]
    fn history_keeps_the_longest_window_and_ignores_repeated_deliveries() {
        let mut view = DeviceView::default();
        for second in 0..10 {
            view.push_history(HistoryPoint {
                sampled_at_ms: second * 1000,
                gpu: Some(1),
                ..HistoryPoint::default()
            });
        }
        view.push_history(HistoryPoint {
            sampled_at_ms: 9000,
            gpu: Some(99),
            ..HistoryPoint::default()
        });
        assert_eq!(view.history.len(), 10, "a repeated sample is not a new one");
        assert_eq!(view.history.back().unwrap().gpu, Some(1));
        let longest = *HISTORY_WINDOWS_MS.iter().max().unwrap();
        view.push_history(HistoryPoint::missing(longest + 5000));
        assert_eq!(
            view.history.front().map(|point| point.sampled_at_ms),
            Some(5000),
            "samples older than the longest window are dropped"
        );
        assert_eq!(view.history.len(), 6);
        assert_eq!(
            view.history.back().unwrap().gpu,
            None,
            "a failed acquisition still occupies its position in time"
        );
    }
}
