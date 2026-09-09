//! Cached live data and timed recordings implement the same nonblocking UI input.
use gpu_monitor_core::MonitorSnapshot;
use gpu_monitor_runtime::{AlertEvent, HistoryResponse, MonitorRuntime};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub trait Feed {
    fn poll(&mut self) -> Result<Vec<MonitorSnapshot>, String>;
    fn now_ms(&self) -> u64;
    fn interval_ms(&self) -> u64 {
        1000
    }
    fn retry(&mut self) -> Result<(), String> {
        Err("Retry is unavailable during playback".into())
    }
    fn history(&self, _window_ms: u64) -> Option<HistoryResponse> {
        None
    }
    fn events(&self) -> Vec<AlertEvent> {
        vec![]
    }
    fn label(&self) -> String {
        "Live".into()
    }
}
pub struct LiveFeed<'a> {
    runtime: &'a MonitorRuntime,
    last_sample: Option<u64>,
}
impl<'a> LiveFeed<'a> {
    pub fn new(runtime: &'a MonitorRuntime) -> Self {
        Self {
            runtime,
            last_sample: None,
        }
    }
}
impl Feed for LiveFeed<'_> {
    fn poll(&mut self) -> Result<Vec<MonitorSnapshot>, String> {
        let snapshot = self.runtime.latest()?;
        if self.last_sample == Some(snapshot.sampled_at_ms) {
            return Ok(vec![]);
        }
        self.last_sample = Some(snapshot.sampled_at_ms);
        Ok(vec![snapshot])
    }
    fn now_ms(&self) -> u64 {
        unix_ms()
    }
    fn interval_ms(&self) -> u64 {
        self.runtime.interval_ms()
    }
    fn retry(&mut self) -> Result<(), String> {
        self.runtime.retry()
    }
    fn history(&self, window_ms: u64) -> Option<HistoryResponse> {
        Some(self.runtime.history(window_ms))
    }
    fn events(&self) -> Vec<AlertEvent> {
        if self.runtime.alert_config().enabled {
            self.runtime.events()
        } else {
            vec![]
        }
    }
}
pub struct Playback {
    frames: Vec<MonitorSnapshot>,
    cursor: usize,
    started: Instant,
    schedule_ms: Vec<u64>,
    speed: f64,
    interval_ms: u64,
}
impl Playback {
    pub fn new(frames: Vec<MonitorSnapshot>, speed: f64) -> Result<Self, String> {
        if !speed.is_finite() || speed <= 0.0 {
            return Err("Playback speed must be finite and positive".into());
        }
        if frames.is_empty() {
            return Err("Recording contains no snapshots".into());
        }
        let mut schedule_ms = vec![0_u64];
        for pair in frames.windows(2) {
            // Record order is authoritative even when the machine's clock moves back.
            let next = schedule_ms
                .last()
                .unwrap()
                .saturating_add(pair[1].sampled_at_ms.saturating_sub(pair[0].sampled_at_ms));
            schedule_ms.push(next);
        }
        let mut intervals = frames
            .windows(2)
            .filter_map(|pair| {
                pair[1]
                    .sampled_at_ms
                    .checked_sub(pair[0].sampled_at_ms)
                    .filter(|gap| *gap > 0)
            })
            .take(1024)
            .collect::<Vec<_>>();
        intervals.sort_unstable();
        let interval_ms = intervals.get(intervals.len() / 2).copied().unwrap_or(1000);
        Ok(Self {
            frames,
            cursor: 0,
            started: Instant::now(),
            schedule_ms,
            speed,
            interval_ms,
        })
    }
    pub fn is_finished(&self) -> bool {
        self.cursor == self.frames.len()
    }
    pub fn drain_due(&mut self, elapsed: Duration) -> Vec<MonitorSnapshot> {
        let until = (elapsed.as_secs_f64() * self.speed * 1000.0) as u64;
        let first = self.cursor;
        while self.cursor < self.frames.len() && self.schedule_ms[self.cursor] <= until {
            self.cursor += 1;
        }
        self.frames[first..self.cursor].to_vec()
    }
}
impl Feed for Playback {
    fn poll(&mut self) -> Result<Vec<MonitorSnapshot>, String> {
        Ok(self.drain_due(self.started.elapsed()))
    }
    fn now_ms(&self) -> u64 {
        let elapsed = (self.started.elapsed().as_secs_f64() * self.speed * 1000.0) as u64;
        let index = self
            .schedule_ms
            .partition_point(|at| *at <= elapsed)
            .saturating_sub(1);
        if index + 1 == self.frames.len() {
            return self.frames[index].sampled_at_ms;
        }
        self.frames[index]
            .sampled_at_ms
            .saturating_add(elapsed.saturating_sub(self.schedule_ms[index]))
    }
    fn interval_ms(&self) -> u64 {
        self.interval_ms
    }
    fn label(&self) -> String {
        format!(
            "Replay {:.2}×{}",
            self.speed,
            if self.is_finished() {
                " · complete"
            } else {
                ""
            }
        )
    }
}
pub fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
