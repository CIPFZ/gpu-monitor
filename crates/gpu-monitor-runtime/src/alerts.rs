use gpu_monitor_core::MonitorSnapshot;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

const MAX_EVENTS: usize = 500;
const MAX_DEVICES: usize = 1024;

#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AlertConfig {
    pub enabled: bool,
    pub temperature_threshold: f64,
    pub temperature_recovery: f64,
    pub memory_threshold: f64,
    pub memory_recovery: f64,
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub duration_ms: u64,
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub cooldown_ms: u64,
}

impl Default for AlertConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            temperature_threshold: 85.0,
            temperature_recovery: 80.0,
            memory_threshold: 95.0,
            memory_recovery: 90.0,
            duration_ms: 10_000,
            cooldown_ms: 60_000,
        }
    }
}

impl AlertConfig {
    pub fn validate(&self) -> Result<(), String> {
        for (name, threshold, recovery, max) in [
            (
                "Temperature",
                self.temperature_threshold,
                self.temperature_recovery,
                200.0,
            ),
            ("Memory", self.memory_threshold, self.memory_recovery, 100.0),
        ] {
            if !threshold.is_finite()
                || !recovery.is_finite()
                || threshold <= 0.0
                || threshold > max
                || recovery < 0.0
                || recovery >= threshold
            {
                return Err(format!(
                    "{name} thresholds require 0 <= recovery < threshold <= {max}"
                ));
            }
        }
        if self.duration_ms > 3_600_000 || self.cooldown_ms > 86_400_000 {
            return Err("Alert duration must be <= 1 hour and cooldown <= 24 hours".into());
        }
        Ok(())
    }
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertKind {
    Temperature,
    Memory,
    DeviceUnavailable,
    MonitorUnavailable,
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertState {
    Firing,
    Recovered,
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertEvent {
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub id: u64,
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub at_ms: u64,
    pub gpu_uuid: Option<String>,
    pub kind: AlertKind,
    pub state: AlertState,
    pub message: String,
    pub value: Option<f64>,
}

#[derive(Default)]
struct ThresholdState {
    pending_since: Option<u64>,
    active: bool,
    last_fired: Option<u64>,
}

impl ThresholdState {
    fn update(
        &mut self,
        value: Option<f64>,
        now: u64,
        threshold: f64,
        recovery: f64,
        duration: u64,
        cooldown: u64,
    ) -> Option<AlertState> {
        let Some(value) = value.filter(|v| v.is_finite()) else {
            // Missing observations break a sustained condition; they cannot prove recovery.
            self.pending_since = None;
            return None;
        };
        if self.active {
            if value <= recovery {
                self.active = false;
                self.pending_since = None;
                return Some(AlertState::Recovered);
            }
            return None;
        }
        if value < threshold {
            self.pending_since = None;
            return None;
        }
        let since = *self.pending_since.get_or_insert(now);
        if now.saturating_sub(since) >= duration
            && self
                .last_fired
                .is_none_or(|last| now.saturating_sub(last) >= cooldown)
        {
            self.active = true;
            self.pending_since = None;
            self.last_fired = Some(now);
            return Some(AlertState::Firing);
        }
        None
    }
}

#[derive(Default)]
struct DeviceState {
    temperature: ThresholdState,
    memory: ThresholdState,
    unavailable: bool,
    index: u32,
    last_seen: u64,
}

#[derive(Default)]
pub(crate) struct Alerts {
    pub config: AlertConfig,
    devices: HashMap<String, DeviceState>,
    unavailable: bool,
    next_id: u64,
    events: VecDeque<AlertEvent>,
    last_observed: Option<u64>,
}

impl Alerts {
    pub fn configure(&mut self, config: AlertConfig) -> Result<(), String> {
        config.validate()?;
        self.config = config;
        self.devices.clear();
        self.unavailable = false;
        self.last_observed = None;
        Ok(())
    }

    pub fn events(&self) -> Vec<AlertEvent> {
        self.events.iter().cloned().collect()
    }

    fn emit(
        &mut self,
        at_ms: u64,
        gpu_uuid: Option<String>,
        kind: AlertKind,
        state: AlertState,
        value: Option<f64>,
        message: String,
    ) {
        self.next_id = self.next_id.saturating_add(1);
        self.events.push_back(AlertEvent {
            id: self.next_id,
            at_ms,
            gpu_uuid,
            kind,
            state,
            value,
            message,
        });
        while self.events.len() > MAX_EVENTS {
            self.events.pop_front();
        }
    }

    pub fn observe(&mut self, snapshot: &MonitorSnapshot, elapsed_ms: u64, interval_ms: u64) {
        if !self.config.enabled {
            return;
        }
        if self.last_observed.is_some_and(|previous| {
            elapsed_ms.saturating_sub(previous) > interval_ms.saturating_mul(3)
        }) {
            // A blocked sampler supplies no evidence of a continuous breach.
            // Existing active alerts still need a measured recovery value.
            for state in self.devices.values_mut() {
                state.temperature.pending_since = None;
                state.memory.pending_since = None;
            }
        }
        self.last_observed = Some(elapsed_ms);
        let now = snapshot.sampled_at_ms;
        let unavailable = snapshot.error.is_some();
        if unavailable != self.unavailable {
            self.unavailable = unavailable;
            self.emit(
                now,
                None,
                AlertKind::MonitorUnavailable,
                if unavailable {
                    AlertState::Firing
                } else {
                    AlertState::Recovered
                },
                None,
                snapshot
                    .error
                    .as_ref()
                    .map(|e| e.message.clone())
                    .unwrap_or_else(|| "GPU monitoring recovered".into()),
            );
        }
        if unavailable {
            for state in self.devices.values_mut() {
                state.temperature.pending_since = None;
                state.memory.pending_since = None;
            }
            return;
        }
        let config = self.config.clone();
        let mut seen = HashSet::new();
        let mut transitions = Vec::new();
        for gpu in &snapshot.gpus {
            let uuid = gpu.device.uuid.clone();
            seen.insert(uuid.clone());
            // Before a UUID can be queried an outage is indexed provisionally.
            // Resolve that identity on the first successful sample.
            let provisional = format!("index:{}", gpu.device.index);
            if self
                .devices
                .remove(&provisional)
                .is_some_and(|state| state.unavailable)
            {
                transitions.push((
                    provisional,
                    AlertKind::DeviceUnavailable,
                    AlertState::Recovered,
                    None,
                ));
            }
            if !self.devices.contains_key(&uuid) && self.devices.len() >= MAX_DEVICES {
                if let Some(oldest) = self
                    .devices
                    .iter()
                    .min_by_key(|(_, d)| d.last_seen)
                    .map(|(id, _)| id.clone())
                {
                    self.devices.remove(&oldest);
                }
            }
            let device = self.devices.entry(uuid.clone()).or_default();
            device.last_seen = elapsed_ms;
            device.index = gpu.device.index;
            if device.unavailable {
                device.unavailable = false;
                transitions.push((
                    uuid.clone(),
                    AlertKind::DeviceUnavailable,
                    AlertState::Recovered,
                    None,
                ));
            }
            let temperature = gpu.metrics.temperature.map(|v| v as f64);
            let memory = gpu
                .memory
                .as_ref()
                .filter(|m| m.total > 0)
                .map(|m| m.used as f64 / m.total as f64 * 100.0);
            if let Some(state) = device.temperature.update(
                temperature,
                elapsed_ms,
                config.temperature_threshold,
                config.temperature_recovery,
                config.duration_ms,
                config.cooldown_ms,
            ) {
                transitions.push((uuid.clone(), AlertKind::Temperature, state, temperature));
            }
            if let Some(state) = device.memory.update(
                memory,
                elapsed_ms,
                config.memory_threshold,
                config.memory_recovery,
                config.duration_ms,
                config.cooldown_ms,
            ) {
                transitions.push((uuid, AlertKind::Memory, state, memory));
            }
        }
        // Explicit failures can introduce an unavailable device before its first good sample.
        for failure in &snapshot.failures {
            let uuid = failure
                .uuid
                .clone()
                .or_else(|| {
                    self.devices
                        .iter()
                        .find(|(_, state)| state.index == failure.index)
                        .map(|(uuid, _)| uuid.clone())
                })
                .unwrap_or_else(|| format!("index:{}", failure.index));
            if self.devices.len() < MAX_DEVICES || self.devices.contains_key(&uuid) {
                let state = self.devices.entry(uuid).or_default();
                state.index = failure.index;
            }
        }
        // A good enumeration that no longer includes a known UUID is a device outage.
        for (uuid, device) in &mut self.devices {
            if !seen.contains(uuid) {
                device.temperature.pending_since = None;
                device.memory.pending_since = None;
                if !device.unavailable {
                    device.unavailable = true;
                    transitions.push((
                        uuid.clone(),
                        AlertKind::DeviceUnavailable,
                        AlertState::Firing,
                        None,
                    ));
                }
            }
        }
        for (uuid, kind, state, value) in transitions {
            let message = format!(
                "GPU {uuid}: {kind:?} {}{}",
                if state == AlertState::Firing {
                    "alert"
                } else {
                    "recovered"
                },
                value.map(|v| format!(" ({v:.1})")).unwrap_or_default()
            );
            self.emit(now, Some(uuid), kind, state, value, message);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn threshold_requires_continuity_hysteresis_and_cooldown() {
        let mut state = ThresholdState::default();
        let mut update = |value, now| state.update(value, now, 85.0, 80.0, 10, 60);
        assert_eq!(update(Some(90.0), 0), None);
        assert_eq!(update(None, 9), None);
        assert_eq!(update(Some(90.0), 10), None);
        assert_eq!(update(Some(90.0), 20), Some(AlertState::Firing));
        assert_eq!(update(None, 21), None);
        assert_eq!(update(Some(82.0), 22), None);
        assert_eq!(update(Some(80.0), 23), Some(AlertState::Recovered));
        assert_eq!(update(Some(90.0), 24), None);
        assert_eq!(update(Some(90.0), 79), None);
        assert_eq!(update(Some(90.0), 80), Some(AlertState::Firing));
    }

    #[test]
    fn rejects_non_finite_reversed_and_excessive_configuration() {
        let mut config = AlertConfig::default();
        assert!(config.validate().is_ok());
        config.memory_recovery = config.memory_threshold;
        assert!(config.validate().is_err());
        config = AlertConfig::default();
        config.temperature_threshold = f64::NAN;
        assert!(config.validate().is_err());
        config = AlertConfig::default();
        config.duration_ms = u64::MAX;
        assert!(config.validate().is_err());
    }

    #[test]
    fn global_outage_is_deduplicated_and_event_log_is_bounded() {
        let mut alerts = Alerts::default();
        let mut snapshot: MonitorSnapshot = serde_json::from_value(serde_json::json!({
            "sampled_at_ms":1,"gpus":[],"failures":[],"error":null
        }))
        .unwrap();
        for n in 0..MAX_EVENTS + 10 {
            snapshot.error = Some(gpu_monitor_core::SampleError {
                kind: gpu_monitor_core::ErrorKind::Unknown,
                message: "offline".into(),
            });
            alerts.observe(&snapshot, n as u64, 10_000);
            alerts.observe(&snapshot, n as u64, 10_000);
            snapshot.error = None;
            alerts.observe(&snapshot, n as u64, 10_000);
        }
        assert_eq!(alerts.events.len(), MAX_EVENTS);
        assert_eq!(alerts.next_id as usize, (MAX_EVENTS + 10) * 2);
    }

    #[test]
    fn device_outage_breaks_pending_threshold_and_recovers_by_uuid() {
        let mut alerts = Alerts::default();
        let mut snapshot = crate::test_snapshot();
        alerts.observe(&snapshot, 0, 10_000);
        let good = snapshot.gpus.remove(0);
        snapshot.failures.push(gpu_monitor_core::DeviceFailure {
            index: 0,
            uuid: Some("GPU-a".into()),
            error: gpu_monitor_core::SampleError {
                kind: gpu_monitor_core::ErrorKind::DeviceLost,
                message: "lost".into(),
            },
        });
        alerts.observe(&snapshot, 9_000, 10_000);
        assert_eq!(alerts.events().len(), 1);
        assert_eq!(alerts.events()[0].kind, AlertKind::DeviceUnavailable);
        snapshot.gpus.push(good);
        snapshot.failures.clear();
        alerts.observe(&snapshot, 11_000, 10_000);
        assert_eq!(alerts.events().len(), 2);
        assert_eq!(alerts.events()[1].state, AlertState::Recovered);
        alerts.observe(&snapshot, 21_000, 10_000);
        let events = alerts.events();
        assert_eq!(events.len(), 4);
        assert_eq!(events[2].kind, AlertKind::Temperature);
        assert_eq!(events[3].kind, AlertKind::Memory);
        // Global missing observations cannot recover either threshold.
        snapshot.error = Some(gpu_monitor_core::SampleError {
            kind: gpu_monitor_core::ErrorKind::Unknown,
            message: "offline".into(),
        });
        alerts.observe(&snapshot, 22_000, 10_000);
        assert_eq!(
            alerts.events().last().unwrap().kind,
            AlertKind::MonitorUnavailable
        );
        assert!(alerts.devices["GPU-a"].temperature.active);
    }

    #[test]
    fn first_failure_without_uuid_recovers_when_identity_becomes_available() {
        let mut alerts = Alerts::default();
        let good = crate::test_snapshot();
        let mut missing = good.clone();
        missing.gpus.clear();
        missing.failures.push(gpu_monitor_core::DeviceFailure {
            index: 0,
            uuid: None,
            error: gpu_monitor_core::SampleError {
                kind: gpu_monitor_core::ErrorKind::Unknown,
                message: "unreadable".into(),
            },
        });
        alerts.observe(&missing, 0, 10_000);
        alerts.observe(&good, 1000, 10_000);
        assert_eq!(alerts.events().len(), 2);
        assert_eq!(alerts.events()[1].state, AlertState::Recovered);
        assert!(!alerts.devices.contains_key("index:0"));
    }

    #[test]
    fn driver_gap_restarts_pending_but_respects_long_sampling_intervals() {
        let mut alerts = Alerts::default();
        let sample = crate::test_snapshot();
        alerts.observe(&sample, 0, 1000);
        alerts.observe(&sample, 120_000, 1000);
        assert!(alerts.events().is_empty());
        for now in (121_000..=130_000).step_by(1000) {
            alerts.observe(&sample, now, 1000);
        }
        assert_eq!(alerts.events().len(), 2);
        // A later hole does not generate recovery for an active alert.
        alerts.observe(&sample, 300_000, 1000);
        assert_eq!(alerts.events().len(), 2);

        let mut slow = Alerts::default();
        slow.observe(&sample, 0, 30_000);
        slow.observe(&sample, 30_000, 30_000);
        assert_eq!(slow.events().len(), 2);
    }

    #[test]
    fn known_uuid_state_survives_index_reordering() {
        let mut alerts = Alerts::default();
        let mut sample = crate::test_snapshot();
        let mut second = sample.gpus[0].clone();
        second.device.uuid = "GPU-b".into();
        second.device.index = 1;
        sample.gpus.push(second);
        alerts.observe(&sample, 0, 1000);
        sample.gpus[0].device.index = 1;
        sample.gpus[1].device.index = 0;
        alerts.observe(&sample, 1000, 1000);
        assert!(alerts.events().is_empty());
        let failed = sample.gpus.remove(1);
        sample.failures.push(gpu_monitor_core::DeviceFailure {
            index: 0,
            uuid: None,
            error: gpu_monitor_core::SampleError {
                kind: gpu_monitor_core::ErrorKind::DeviceLost,
                message: "lost".into(),
            },
        });
        alerts.observe(&sample, 2000, 1000);
        assert_eq!(alerts.events()[0].gpu_uuid.as_deref(), Some("GPU-b"));
        sample.gpus.push(failed);
        sample.failures.clear();
        alerts.observe(&sample, 3000, 1000);
        assert_eq!(alerts.events()[1].gpu_uuid.as_deref(), Some("GPU-b"));
        assert_eq!(alerts.events()[1].state, AlertState::Recovered);
    }
}
