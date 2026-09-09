//! Shared background sampling, bounded history, recording and alert evaluation.
//! UI reads never execute driver or disk I/O. Drop signals shutdown without
//! joining a potentially blocked driver call.

mod alerts;
mod history;
mod recording;

pub use alerts::{AlertConfig, AlertEvent, AlertKind, AlertState};
pub use history::{HistoryFrame, HistoryGpu, HistoryResponse, MAX_HISTORY_MS};
pub use recording::{load_recording, RecordingStatus, MAX_RECORDING_BYTES, MAX_RECORDING_FRAMES};

use gpu_monitor_core::{MonitorService, MonitorSnapshot};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender, TrySendError},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

trait Sampler: Send + 'static {
    fn sample(&mut self) -> MonitorSnapshot;
    fn retry(&mut self);
}
impl Sampler for MonitorService {
    fn sample(&mut self) -> MonitorSnapshot {
        self.sample()
    }
    fn retry(&mut self) {
        self.retry();
    }
}

#[derive(Default)]
struct Shared {
    latest: Option<MonitorSnapshot>,
    history: history::History,
    alerts: alerts::Alerts,
    error: Option<String>,
}

pub struct MonitorRuntime {
    shared: Arc<Mutex<Shared>>,
    recorder: Arc<recording::Recorder>,
    retry_tx: SyncSender<()>,
    stopping: Arc<AtomicBool>,
    origin: Instant,
    interval_ms: u64,
    _thread: Option<JoinHandle<()>>,
}

impl MonitorRuntime {
    /// Intervals below 100 ms are clamped to avoid driver hammering and bounded
    /// history truncation. All clients can inspect the effective interval.
    pub fn new(interval: Duration) -> Self {
        Self::spawn(MonitorService::new(), interval)
    }

    fn spawn(mut sampler: impl Sampler, interval: Duration) -> Self {
        let interval = interval.max(Duration::from_millis(100));
        let origin = Instant::now();
        let shared = Arc::new(Mutex::new(Shared::default()));
        let cache = shared.clone();
        let recorder = Arc::new(recording::Recorder::default());
        let recording = recorder.clone();
        let stopping = Arc::new(AtomicBool::new(false));
        let stop = stopping.clone();
        let (retry_tx, retry_rx) = mpsc::sync_channel(1);
        let handle = thread::Builder::new()
            .name("gpu-sampler".into())
            .spawn(move || {
                while !stop.load(Ordering::Acquire) {
                    let sample_started = Instant::now();
                    let snapshot = sampler.sample();
                    if stop.load(Ordering::Acquire) {
                        break;
                    }
                    let elapsed = elapsed_ms(origin);
                    recording.submit(&snapshot);
                    match cache.lock() {
                        Ok(mut shared) => {
                            shared.history.push(&snapshot, elapsed);
                            shared.alerts.observe(
                                &snapshot,
                                elapsed,
                                interval.as_millis().min(u64::MAX as u128) as u64,
                            );
                            shared.latest = Some(snapshot);
                        }
                        Err(_) => break,
                    }
                    // Account for sampling time, with a small floor if the driver
                    // consistently exceeds the requested interval.
                    let wait = interval
                        .saturating_sub(sample_started.elapsed())
                        .max(Duration::from_millis(10));
                    match retry_rx.recv_timeout(wait) {
                        Ok(()) => sampler.retry(),
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
            });
        let handle = match handle {
            Ok(handle) => Some(handle),
            Err(error) => {
                shared.lock().expect("new mutex").error =
                    Some(format!("Failed to start GPU sampling thread: {error}"));
                None
            }
        };
        Self {
            shared,
            recorder,
            retry_tx,
            stopping,
            origin,
            interval_ms: interval.as_millis().min(u64::MAX as u128) as u64,
            _thread: handle,
        }
    }

    pub fn interval_ms(&self) -> u64 {
        self.interval_ms
    }

    pub fn latest(&self) -> Result<MonitorSnapshot, String> {
        let shared = self
            .shared
            .lock()
            .map_err(|_| "GPU sample cache lock was poisoned")?;
        if let Some(error) = &shared.error {
            return Err(error.clone());
        }
        shared
            .latest
            .clone()
            .ok_or_else(|| "Waiting for the first GPU sample".into())
    }

    pub fn retry(&self) -> Result<(), String> {
        match self.retry_tx.try_send(()) {
            Ok(()) | Err(TrySendError::Full(())) => Ok(()),
            Err(TrySendError::Disconnected(())) => Err("GPU sampling thread is unavailable".into()),
        }
    }

    pub fn history(&self, window_ms: u64) -> HistoryResponse {
        let mut response = self
            .shared
            .lock()
            .map(|shared| shared.history.read(window_ms, elapsed_ms(self.origin)))
            .unwrap_or_else(|_| HistoryResponse {
                window_ms: window_ms.min(MAX_HISTORY_MS),
                interval_ms: self.interval_ms,
                frames: Vec::new(),
            });
        response.interval_ms = self.interval_ms;
        response
    }

    pub fn events(&self) -> Vec<AlertEvent> {
        self.shared
            .lock()
            .map(|shared| shared.alerts.events())
            .unwrap_or_default()
    }

    pub fn alert_config(&self) -> AlertConfig {
        self.shared
            .lock()
            .map(|shared| shared.alerts.config.clone())
            .unwrap_or_default()
    }

    pub fn configure_alerts(&self, config: AlertConfig) -> Result<(), String> {
        self.shared
            .lock()
            .map_err(|_| "GPU sample cache lock was poisoned")?
            .alerts
            .configure(config)
    }

    pub fn start_recording(
        &self,
        path: PathBuf,
        include_commands: bool,
    ) -> Result<RecordingStatus, String> {
        // Capture before opening the session: the initial frame is queued before
        // the sampler can submit later frames, preserving capture order.
        self.recorder
            .start(path, include_commands, self.latest().ok())
    }

    pub fn stop_recording(&self) -> Result<RecordingStatus, String> {
        self.recorder.stop()
    }

    pub fn recording_status(&self) -> RecordingStatus {
        self.recorder.status()
    }
}

fn elapsed_ms(origin: Instant) -> u64 {
    origin.elapsed().as_millis().min(u64::MAX as u128) as u64
}

#[cfg(test)]
fn test_snapshot() -> MonitorSnapshot {
    serde_json::from_value(serde_json::json!({
        "sampled_at_ms": 1000, "failures": [], "error": null,
        "gpus": [{"device": {"index": 0, "uuid": "GPU-a", "name": "GPU", "pci_bus_id": "bus", "driver_version": "driver"},
            "metrics": {"gpu_utilization": 0, "temperature": 90},
            "memory": {"used": 96, "total": 100, "free": 4}, "sampled_at_ms": 1000,
            "issues": [], "processes": [{"pid": 12, "name": "python", "gpu_memory": 5, "process_type": "Compute",
                "command": ["python", "--token=secret"]}]}]
    })).unwrap()
}

impl Drop for MonitorRuntime {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        let _ = self.retry_tx.try_send(());
        let _ = self.recorder.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{Receiver, Sender};
    struct ControlledSampler {
        entered: Sender<usize>,
        release: Receiver<()>,
        retried: Sender<()>,
        exited: Sender<()>,
        count: usize,
    }
    impl Sampler for ControlledSampler {
        fn sample(&mut self) -> MonitorSnapshot {
            self.count += 1;
            self.entered.send(self.count).unwrap();
            self.release.recv().unwrap();
            serde_json::from_value(serde_json::json!({"sampled_at_ms":self.count,"gpus":[],"failures":[],"error":null})).unwrap()
        }
        fn retry(&mut self) {
            self.retried.send(()).unwrap();
        }
    }
    impl Drop for ControlledSampler {
        fn drop(&mut self) {
            let _ = self.exited.send(());
        }
    }

    #[test]
    fn slow_driver_preserves_cache_coalesces_retry_and_never_blocks_shutdown() {
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (retry_tx, retry_rx) = mpsc::channel();
        let (exited_tx, exited_rx) = mpsc::channel();
        let runtime = MonitorRuntime::spawn(
            ControlledSampler {
                entered: entered_tx,
                release: release_rx,
                retried: retry_tx,
                exited: exited_tx,
                count: 0,
            },
            Duration::from_secs(60),
        );
        assert_eq!(entered_rx.recv_timeout(Duration::from_secs(2)).unwrap(), 1);
        assert!(runtime.latest().unwrap_err().contains("first GPU sample"));
        for _ in 0..100 {
            runtime.retry().unwrap();
        }
        release_tx.send(()).unwrap();
        retry_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(entered_rx.recv_timeout(Duration::from_secs(2)).unwrap(), 2);
        assert_eq!(runtime.latest().unwrap().sampled_at_ms, 1);
        assert_eq!(runtime.history(60_000).frames.len(), 1);
        assert!(retry_rx.try_recv().is_err());
        drop(runtime);
        assert!(exited_rx.try_recv().is_err());
        release_tx.send(()).unwrap();
        exited_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    }

    #[test]
    fn idle_worker_wakes_on_drop() {
        struct Immediate(Sender<()>);
        impl Sampler for Immediate {
            fn sample(&mut self) -> MonitorSnapshot {
                serde_json::from_value(
                    serde_json::json!({"sampled_at_ms":1,"gpus":[],"failures":[],"error":null}),
                )
                .unwrap()
            }
            fn retry(&mut self) {}
        }
        impl Drop for Immediate {
            fn drop(&mut self) {
                let _ = self.0.send(());
            }
        }
        let (tx, rx) = mpsc::channel();
        let runtime = MonitorRuntime::spawn(Immediate(tx), Duration::from_secs(60));
        drop(runtime);
        rx.recv_timeout(Duration::from_secs(2)).unwrap();
    }
}
