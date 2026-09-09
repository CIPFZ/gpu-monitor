use gpu_monitor_core::MonitorSnapshot;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

pub const MAX_HISTORY_MS: u64 = 60 * 60 * 1000;
const MAX_HISTORY_FRAMES: usize = 36_001;
const MAX_HISTORY_POINTS: usize = 120_000;

#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryGpu {
    pub uuid: String,
    pub index: u32,
    pub gpu_utilization: Option<u32>,
    pub memory_percent: Option<f64>,
    pub temperature: Option<u32>,
    pub power_watts: Option<f64>,
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryFrame {
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub sampled_at_ms: u64,
    pub gpus: Vec<HistoryGpu>,
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryResponse {
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub window_ms: u64,
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub interval_ms: u64,
    pub frames: Vec<HistoryFrame>,
}

#[derive(Default)]
pub(crate) struct History {
    // Monotonic elapsed time bounds retention even if the wall clock changes.
    frames: VecDeque<(u64, HistoryFrame)>,
    points: usize,
}

impl History {
    pub fn push(&mut self, snapshot: &MonitorSnapshot, elapsed_ms: u64) {
        let frame = HistoryFrame {
            sampled_at_ms: snapshot.sampled_at_ms,
            gpus: snapshot
                .gpus
                .iter()
                .take(MAX_HISTORY_POINTS)
                .map(|gpu| HistoryGpu {
                    uuid: gpu.device.uuid.clone(),
                    index: gpu.device.index,
                    gpu_utilization: gpu.metrics.gpu_utilization,
                    memory_percent: gpu
                        .memory
                        .as_ref()
                        .filter(|m| m.total > 0)
                        .map(|m| m.used as f64 / m.total as f64 * 100.0),
                    temperature: gpu.metrics.temperature,
                    power_watts: gpu.metrics.power_usage.map(|mw| mw as f64 / 1000.0),
                })
                .collect(),
        };
        self.points += frame.gpus.len();
        self.frames.push_back((elapsed_ms, frame));
        while self
            .frames
            .front()
            .is_some_and(|(at, _)| elapsed_ms.saturating_sub(*at) > MAX_HISTORY_MS)
            || self.frames.len() > MAX_HISTORY_FRAMES
            || self.points > MAX_HISTORY_POINTS
        {
            if let Some((_, frame)) = self.frames.pop_front() {
                self.points -= frame.gpus.len();
            }
        }
    }

    pub fn read(&self, window_ms: u64, elapsed_ms: u64) -> HistoryResponse {
        let window_ms = window_ms.min(MAX_HISTORY_MS);
        HistoryResponse {
            window_ms,
            interval_ms: 0,
            frames: self
                .frames
                .iter()
                .filter(|(at, _)| elapsed_ms.saturating_sub(*at) <= window_ms)
                .map(|(_, frame)| frame.clone())
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn time_retention_ignores_wall_clock_jumps_and_keeps_missing_frames() {
        let mut history = History::default();
        let snapshot: MonitorSnapshot = serde_json::from_value(serde_json::json!({
            "sampled_at_ms": 900, "gpus": [], "failures": [], "error": null
        }))
        .unwrap();
        history.push(&snapshot, 0);
        let mut second = snapshot.clone();
        second.sampled_at_ms = 10;
        history.push(&second, 1000);
        assert_eq!(history.read(60_000, 1000).frames.len(), 2);
        history.push(&second, MAX_HISTORY_MS + 1);
        assert_eq!(history.read(u64::MAX, MAX_HISTORY_MS + 1).frames.len(), 2);
        assert!(history.read(500, MAX_HISTORY_MS + 600).frames.is_empty());
    }

    #[test]
    fn point_budget_bounds_large_gpu_sets_and_missing_values_stay_null() {
        let mut history = History::default();
        let mut sample = crate::test_snapshot();
        sample.gpus[0].metrics.temperature = None;
        sample.gpus[0].memory = None;
        sample.gpus = vec![sample.gpus[0].clone(); 1000];
        for now in 0..200 {
            history.push(&sample, now);
        }
        assert_eq!(history.points, MAX_HISTORY_POINTS);
        let response = history.read(MAX_HISTORY_MS, 200);
        assert_eq!(response.frames.len(), 120);
        let first = &response.frames[0].gpus[0];
        assert_eq!(first.gpu_utilization, Some(0));
        assert_eq!(first.temperature, None);
        assert_eq!(first.memory_percent, None);
    }
}
