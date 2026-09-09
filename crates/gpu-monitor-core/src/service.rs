//! Sampling lifecycle, fault isolation and bounded initialization retries.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::monitor::NvmlBackend;
use crate::{DeviceFailure, ErrorKind, GpuInfo, MonitorSnapshot, SampleError};

type SampleResult<T> = std::result::Result<T, SampleError>;

pub(crate) trait Backend: Send {
    fn device_count(&self) -> SampleResult<u32>;
    fn sample_device(
        &mut self,
        index: u32,
        sampled_at_ms: u64,
    ) -> std::result::Result<GpuInfo, DeviceFailure>;
}

type Factory = Box<dyn FnMut() -> SampleResult<Box<dyn Backend>> + Send>;
// Monotonic time schedules retries; wall time labels samples.
type Clock = Box<dyn Fn() -> (Duration, u64) + Send>;

/// Synchronous sampling service. Constructing it does no driver work.
///
/// Call `sample` on a background worker in graphical applications. Initialization
/// and enumeration failures retry after 1, 2, 4, ... seconds, capped at 30 seconds.
/// Device failures never discard other devices' results.
pub struct MonitorService {
    backend: Option<Box<dyn Backend>>,
    factory: Factory,
    clock: Clock,
    retry_at: Duration,
    retry_delay: Duration,
    last_error: Option<SampleError>,
}

impl Default for MonitorService {
    fn default() -> Self {
        Self::new()
    }
}

impl MonitorService {
    pub fn new() -> Self {
        let origin = Instant::now();
        Self::with_dependencies(
            Box::new(|| NvmlBackend::new().map(|backend| Box::new(backend) as Box<dyn Backend>)),
            Box::new(move || {
                let wall = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default();
                (
                    origin.elapsed(),
                    wall.as_millis().min(u64::MAX as u128) as u64,
                )
            }),
        )
    }

    fn with_dependencies(factory: Factory, clock: Clock) -> Self {
        Self {
            backend: None,
            factory,
            clock,
            retry_at: Duration::ZERO,
            retry_delay: Duration::from_secs(1),
            last_error: None,
        }
    }

    /// Request a fresh initialization on the next sample, bypassing backoff.
    pub fn retry(&mut self) {
        self.backend = None;
        self.retry_at = Duration::ZERO;
        self.retry_delay = Duration::from_secs(1);
        self.last_error = None;
    }

    pub fn sample(&mut self) -> MonitorSnapshot {
        let (now, sampled_at_ms) = (self.clock)();
        let mut snapshot = MonitorSnapshot {
            schema_version: crate::SCHEMA_VERSION,
            sampled_at_ms,
            gpus: Vec::new(),
            failures: Vec::new(),
            error: None,
        };
        if self.backend.is_none() {
            if now < self.retry_at {
                snapshot.error = self.last_error.clone();
                return snapshot;
            }
            match (self.factory)() {
                Ok(backend) => self.backend = Some(backend),
                Err(error) => {
                    snapshot.error = Some(self.defer_retry(now, error));
                    return snapshot;
                }
            }
        }
        let backend = self.backend.as_mut().expect("initialized above");
        let count = match backend.device_count() {
            Ok(0) => Err(SampleError {
                kind: ErrorKind::NoDevices,
                message: "No NVIDIA GPU devices found".to_owned(),
            }),
            result => result,
        };
        let count = match count {
            Ok(count) => count,
            Err(error) => {
                snapshot.error = Some(self.defer_retry(now, error));
                return snapshot;
            }
        };
        for index in 0..count {
            match backend.sample_device(index, sampled_at_ms) {
                Ok(gpu) => snapshot.gpus.push(gpu),
                Err(failure) => snapshot.failures.push(failure),
            }
        }
        // An unloaded/replaced driver invalidates existing handles. Publish the
        // current partial results before trying a new session next time.
        let session_error = snapshot
            .failures
            .iter()
            .map(|failure| &failure.error)
            .chain(
                snapshot
                    .gpus
                    .iter()
                    .flat_map(|gpu| gpu.issues.iter().map(|issue| &issue.error)),
            )
            .find(|error| error.kind == ErrorKind::Uninitialized)
            .cloned();
        if let Some(error) = session_error {
            self.defer_retry(now, error);
        } else {
            self.retry_delay = Duration::from_secs(1);
            self.last_error = None;
        }
        snapshot
    }

    fn defer_retry(&mut self, now: Duration, error: SampleError) -> SampleError {
        self.backend = None;
        self.retry_at = now.saturating_add(self.retry_delay);
        self.retry_delay = (self.retry_delay * 2).min(Duration::from_secs(30));
        self.last_error = Some(error.clone());
        error
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DeviceInfo, GpuMetrics};
    use std::sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc,
    };

    struct FakeBackend {
        count_error: bool,
        lost_index: Option<u32>,
    }
    impl Backend for FakeBackend {
        fn device_count(&self) -> SampleResult<u32> {
            if self.count_error {
                Err(driver_error())
            } else {
                Ok(2)
            }
        }
        fn sample_device(
            &mut self,
            index: u32,
            sampled_at_ms: u64,
        ) -> std::result::Result<GpuInfo, DeviceFailure> {
            if self.lost_index == Some(index) {
                return Err(DeviceFailure {
                    index,
                    uuid: Some(format!("gpu-{index}")),
                    error: SampleError {
                        kind: ErrorKind::DeviceLost,
                        message: "device fell off bus".into(),
                    },
                });
            }
            Ok(GpuInfo {
                device: DeviceInfo {
                    index,
                    name: "GPU".into(),
                    uuid: format!("gpu-{index}"),
                    pci_bus_id: "bus".into(),
                    driver_version: "driver".into(),
                    cuda_version: None,
                    power_limit: None,
                    power_limit_max: None,
                },
                metrics: GpuMetrics::default(),
                memory: None,
                processes: Vec::new(),
                sampled_at_ms,
                issues: Vec::new(),
            })
        }
    }
    fn driver_error() -> SampleError {
        SampleError {
            kind: ErrorKind::Uninitialized,
            message: "driver not loaded (original detail)".into(),
        }
    }
    fn fake_clock(seconds: Arc<AtomicU64>) -> Clock {
        Box::new(move || {
            let seconds = seconds.load(Ordering::SeqCst);
            (Duration::from_secs(seconds), seconds * 1000)
        })
    }

    #[test]
    fn one_lost_device_preserves_healthy_devices_and_error_identity() {
        for lost_index in [0, 1] {
            let mut service = MonitorService::with_dependencies(
                Box::new(move || {
                    Ok(Box::new(FakeBackend {
                        count_error: false,
                        lost_index: Some(lost_index),
                    }))
                }),
                fake_clock(Arc::new(AtomicU64::new(42))),
            );
            let snapshot = service.sample();
            assert!(snapshot.error.is_none());
            assert_eq!(snapshot.gpus.len(), 1);
            assert_eq!(snapshot.gpus[0].device.index, 1 - lost_index);
            assert_eq!(snapshot.gpus[0].sampled_at_ms, 42000);
            assert_eq!(snapshot.failures[0].index, lost_index);
            assert_eq!(snapshot.failures[0].uuid, Some(format!("gpu-{lost_index}")));
            assert_eq!(snapshot.failures[0].error.kind, ErrorKind::DeviceLost);
        }
    }

    #[test]
    fn initialization_is_lazy_preserves_error_and_recovers_with_backoff() {
        let calls = Arc::new(AtomicUsize::new(0));
        let attempts = calls.clone();
        let seconds = Arc::new(AtomicU64::new(0));
        let mut service = MonitorService::with_dependencies(
            Box::new(move || {
                if attempts.fetch_add(1, Ordering::SeqCst) < 2 {
                    return Err(driver_error());
                }
                Ok(Box::new(FakeBackend {
                    count_error: false,
                    lost_index: None,
                }))
            }),
            fake_clock(seconds.clone()),
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(
            service.sample().error.unwrap().message,
            driver_error().message
        );
        service.sample();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        seconds.store(1, Ordering::SeqCst);
        service.sample();
        seconds.store(2, Ordering::SeqCst);
        service.sample();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        seconds.store(3, Ordering::SeqCst);
        assert_eq!(service.sample().gpus.len(), 2);
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn explicit_retry_bypasses_backoff() {
        let calls = Arc::new(AtomicUsize::new(0));
        let attempts = calls.clone();
        let mut service = MonitorService::with_dependencies(
            Box::new(move || {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err(driver_error())
            }),
            fake_clock(Arc::new(AtomicU64::new(0))),
        );
        service.sample();
        service.sample();
        service.retry();
        service.sample();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn enumeration_failure_reinitializes_and_recovers() {
        let calls = Arc::new(AtomicUsize::new(0));
        let attempts = calls.clone();
        let seconds = Arc::new(AtomicU64::new(0));
        let mut service = MonitorService::with_dependencies(
            Box::new(move || {
                Ok(Box::new(FakeBackend {
                    count_error: attempts.fetch_add(1, Ordering::SeqCst) == 0,
                    lost_index: None,
                }))
            }),
            fake_clock(seconds.clone()),
        );
        assert_eq!(
            service.sample().error.unwrap().kind,
            ErrorKind::Uninitialized
        );
        seconds.store(1, Ordering::SeqCst);
        let snapshot = service.sample();
        assert!(snapshot.error.is_none());
        assert_eq!(snapshot.gpus.len(), 2);
    }

    #[test]
    fn metric_session_invalidation_preserves_sample_then_reinitializes() {
        struct InvalidatedBackend;
        impl Backend for InvalidatedBackend {
            fn device_count(&self) -> SampleResult<u32> {
                Ok(1)
            }
            fn sample_device(
                &mut self,
                index: u32,
                time: u64,
            ) -> std::result::Result<GpuInfo, DeviceFailure> {
                let mut gpu = FakeBackend {
                    count_error: false,
                    lost_index: None,
                }
                .sample_device(index, time)?;
                gpu.issues.push(crate::MetricIssue {
                    metric: "temperature".into(),
                    error: driver_error(),
                });
                Ok(gpu)
            }
        }
        let attempts = Arc::new(AtomicUsize::new(0));
        let calls = attempts.clone();
        let seconds = Arc::new(AtomicU64::new(0));
        let mut service = MonitorService::with_dependencies(
            Box::new(move || {
                if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                    Ok(Box::new(InvalidatedBackend))
                } else {
                    Ok(Box::new(FakeBackend {
                        count_error: false,
                        lost_index: None,
                    }))
                }
            }),
            fake_clock(seconds.clone()),
        );
        let first = service.sample();
        assert!(first.error.is_none());
        assert_eq!(first.gpus.len(), 1);
        assert_eq!(first.gpus[0].issues[0].error.kind, ErrorKind::Uninitialized);
        assert_eq!(
            service.sample().error.unwrap().kind,
            ErrorKind::Uninitialized
        );
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
        seconds.store(1, Ordering::SeqCst);
        let recovered = service.sample();
        assert!(recovered.error.is_none());
        assert_eq!(recovered.gpus.len(), 2);
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn repeated_initialization_failures_cap_retry_delay_at_thirty_seconds() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let calls = attempts.clone();
        let seconds = Arc::new(AtomicU64::new(0));
        let mut service = MonitorService::with_dependencies(
            Box::new(move || {
                calls.fetch_add(1, Ordering::SeqCst);
                Err(driver_error())
            }),
            fake_clock(seconds.clone()),
        );
        for (expected, time) in [0, 1, 3, 7, 15, 31, 61, 91].into_iter().enumerate() {
            if time > 0 {
                seconds.store(time - 1, Ordering::SeqCst);
                assert!(service.sample().error.is_some());
                assert_eq!(attempts.load(Ordering::SeqCst), expected);
            }
            seconds.store(time, Ordering::SeqCst);
            assert!(service.sample().error.is_some());
            assert_eq!(attempts.load(Ordering::SeqCst), expected + 1);
        }
    }
}
