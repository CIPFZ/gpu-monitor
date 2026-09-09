//! One sampling thread owns the driver; IPC only reads completed snapshots.

use gpu_monitor_core::{ErrorKind, MonitorService, MonitorSnapshot, SampleError};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender, TrySendError},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, SystemTime, UNIX_EPOCH},
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

pub(crate) struct SamplingWorker {
    latest: Arc<Mutex<Option<MonitorSnapshot>>>,
    retry_tx: SyncSender<()>,
    stopping: Arc<AtomicBool>,
    // Dropping a JoinHandle detaches it. We deliberately never join an NVML call
    // on the UI thread: a wedged driver must not prevent the window from closing.
    _thread: Option<JoinHandle<()>>,
}

impl SamplingWorker {
    pub(crate) fn new() -> Self {
        Self::spawn(MonitorService::new(), Duration::from_secs(1))
    }

    fn spawn(mut sampler: impl Sampler, interval: Duration) -> Self {
        let latest = Arc::new(Mutex::new(None));
        let cache = latest.clone();
        let stopping = Arc::new(AtomicBool::new(false));
        let stop = stopping.clone();
        // Repeated retry clicks coalesce instead of creating unbounded work.
        let (retry_tx, retry_rx) = mpsc::sync_channel(1);
        let handle = thread::Builder::new()
            .name("gpu-sampler".into())
            .spawn(move || {
                while !stop.load(Ordering::Acquire) {
                    let snapshot = sampler.sample();
                    if stop.load(Ordering::Acquire) {
                        break;
                    }
                    match cache.lock() {
                        Ok(mut latest) => *latest = Some(snapshot),
                        Err(_) => break,
                    }
                    match retry_rx.recv_timeout(interval) {
                        Ok(()) => sampler.retry(),
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
            });
        let handle = match handle {
            Ok(handle) => Some(handle),
            Err(error) => {
                let sampled_at_ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis()
                    .min(u64::MAX as u128) as u64;
                *latest.lock().expect("new mutex") = Some(MonitorSnapshot {
                    sampled_at_ms,
                    gpus: Vec::new(),
                    failures: Vec::new(),
                    error: Some(SampleError {
                        kind: ErrorKind::Unknown,
                        message: format!("Failed to start GPU sampling thread: {error}"),
                    }),
                });
                None
            }
        };
        Self {
            latest,
            retry_tx,
            stopping,
            _thread: handle,
        }
    }

    pub(crate) fn latest(&self) -> Result<MonitorSnapshot, String> {
        self.latest
            .lock()
            .map_err(|_| "GPU sample cache lock was poisoned".to_owned())?
            .clone()
            .ok_or_else(|| "Waiting for the first GPU sample".to_owned())
    }

    pub(crate) fn retry(&self) -> Result<(), String> {
        match self.retry_tx.try_send(()) {
            Ok(()) | Err(TrySendError::Full(())) => Ok(()),
            Err(TrySendError::Disconnected(())) => Err("GPU sampling thread is unavailable".into()),
        }
    }
}

impl Drop for SamplingWorker {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        let _ = self.retry_tx.try_send(());
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
            MonitorSnapshot {
                sampled_at_ms: self.count as u64,
                gpus: Vec::new(),
                failures: Vec::new(),
                error: None,
            }
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
    fn slow_sampling_keeps_cache_readable_retry_coalesced_and_shutdown_nonblocking() {
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (retry_tx, retry_rx) = mpsc::channel();
        let (exited_tx, exited_rx) = mpsc::channel();
        let worker = SamplingWorker::spawn(
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
        // The first driver call is blocked; neither IPC operation waits for it.
        assert!(worker.latest().unwrap_err().contains("first GPU sample"));
        for _ in 0..100 {
            worker.retry().unwrap();
        }
        assert!(entered_rx.try_recv().is_err());
        release_tx.send(()).unwrap();
        retry_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(entered_rx.recv_timeout(Duration::from_secs(2)).unwrap(), 2);
        // A subsequent blocked call leaves the previous completed sample intact.
        assert_eq!(worker.latest().unwrap().sampled_at_ms, 1);
        assert!(retry_rx.try_recv().is_err());
        drop(worker);
        assert!(exited_rx.try_recv().is_err());
        release_tx.send(()).unwrap();
        exited_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(entered_rx.try_recv().is_err());
    }

    #[test]
    fn dropping_an_idle_worker_wakes_it_without_waiting_for_interval() {
        struct ImmediateSampler(Sender<()>);
        impl Sampler for ImmediateSampler {
            fn sample(&mut self) -> MonitorSnapshot {
                MonitorSnapshot {
                    sampled_at_ms: 1,
                    gpus: Vec::new(),
                    failures: Vec::new(),
                    error: None,
                }
            }
            fn retry(&mut self) {}
        }
        impl Drop for ImmediateSampler {
            fn drop(&mut self) {
                let _ = self.0.send(());
            }
        }
        let (exited_tx, exited_rx) = mpsc::channel();
        let worker = SamplingWorker::spawn(ImmediateSampler(exited_tx), Duration::from_secs(60));
        drop(worker);
        exited_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    }
}
