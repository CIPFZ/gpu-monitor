use crate::args::{Cli, SortBy};
use gpu_monitor_core::{GpuInfo, MonitorSnapshot};
use std::cmp::Ordering;

#[derive(Clone, Debug, Default)]
pub struct Selection {
    pub uuids: Vec<String>,
    pub min_free_gib: Option<f64>,
    pub user: Option<String>,
    pub sort: SortBy,
    pub include_command: bool,
}
impl From<&Cli> for Selection {
    fn from(cli: &Cli) -> Self {
        Self {
            uuids: cli.gpus.clone(),
            min_free_gib: cli.min_free_gib,
            user: cli.user.clone(),
            sort: cli.sort,
            include_command: cli.include_command,
        }
    }
}
impl Selection {
    pub fn has_device_filter(&self) -> bool {
        !self.uuids.is_empty() || self.min_free_gib.is_some()
    }
    pub fn matches_uuid(&self, uuid: &str) -> bool {
        self.uuids.is_empty() || self.uuids.iter().any(|selected| selected == uuid)
    }
    pub fn matches_event(&self, event: &gpu_monitor_runtime::AlertEvent) -> bool {
        event
            .gpu_uuid
            .as_deref()
            .is_none_or(|uuid| uuid.starts_with("index:") || self.matches_uuid(uuid))
    }
    pub fn apply(&self, snapshot: &MonitorSnapshot) -> MonitorSnapshot {
        let mut selected = snapshot.clone();
        selected.gpus.retain(|gpu| {
            self.matches_uuid(&gpu.device.uuid)
                && self.min_free_gib.is_none_or(|minimum| {
                    gpu.memory
                        .as_ref()
                        .is_some_and(|memory| memory.free as f64 / 1024_f64.powi(3) >= minimum)
                })
        });
        // An unidentified failure may belong to a selected UUID: keep the uncertainty visible.
        selected.failures.retain(|failure| {
            failure
                .uuid
                .as_ref()
                .is_none_or(|uuid| self.matches_uuid(uuid))
        });
        for gpu in &mut selected.gpus {
            if let Some(owner) = &self.user {
                let uid = owner.parse::<u32>().ok();
                gpu.processes.retain(|process| {
                    process.user.as_deref() == Some(owner.as_str())
                        || (uid.is_some() && process.uid == uid)
                });
            }
            if !self.include_command {
                for process in &mut gpu.processes {
                    process.command = None;
                }
            }
        }
        selected.gpus.sort_by(|a, b| self.compare(a, b));
        selected
    }
    pub fn compare(&self, a: &GpuInfo, b: &GpuInfo) -> Ordering {
        let primary = match self.sort {
            SortBy::Index => a.device.index.cmp(&b.device.index),
            SortBy::FreeMemory => descending(
                a.memory.as_ref().map(|memory| memory.free),
                b.memory.as_ref().map(|memory| memory.free),
            ),
            SortBy::Utilization => descending(a.metrics.gpu_utilization, b.metrics.gpu_utilization),
            SortBy::Temperature => descending(a.metrics.temperature, b.metrics.temperature),
        };
        primary
            .then(a.device.index.cmp(&b.device.index))
            .then(a.device.uuid.cmp(&b.device.uuid))
    }
}
fn descending<T: Ord>(a: Option<T>, b: Option<T>) -> Ordering {
    match (a, b) {
        (Some(a), Some(b)) => b.cmp(&a),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}
