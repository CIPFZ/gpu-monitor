//! Tauri IPC reads cached samples and signals retries; it never calls NVML.

use crate::worker::SamplingWorker;
use gpu_monitor_core::MonitorSnapshot;
use serde::Serialize;
use tauri::State;

pub struct AppState {
    worker: SamplingWorker,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            worker: SamplingWorker::new(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CommandError {
    pub message: String,
}

impl From<String> for CommandError {
    fn from(message: String) -> Self {
        Self { message }
    }
}

#[tauri::command]
pub fn get_gpu_info(state: State<AppState>) -> Result<MonitorSnapshot, CommandError> {
    state.worker.latest().map_err(Into::into)
}

#[tauri::command]
pub fn retry_gpu_monitor(state: State<AppState>) -> Result<(), CommandError> {
    state.worker.retry().map_err(Into::into)
}

#[tauri::command]
pub fn get_gpu_count(state: State<AppState>) -> Result<u32, CommandError> {
    let snapshot = state.worker.latest()?;
    if let Some(error) = snapshot.error {
        return Err(error.message.into());
    }
    Ok((snapshot.gpus.len() + snapshot.failures.len()) as u32)
}

#[tauri::command]
pub fn is_gpu_available(state: State<AppState>) -> bool {
    state
        .worker
        .latest()
        .is_ok_and(|snapshot| !snapshot.gpus.is_empty())
}
