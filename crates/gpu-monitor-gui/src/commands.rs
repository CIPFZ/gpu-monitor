//! IPC reads the shared runtime; file IO and desktop notifications run off the UI thread.
use gpu_monitor_core::MonitorSnapshot;
use gpu_monitor_runtime::{
    AlertConfig, AlertEvent, HistoryResponse, MonitorRuntime, RecordingStatus,
};
use serde::Serialize;
use std::{path::PathBuf, sync::Arc, time::Duration};
use tauri::State;

pub struct AppState {
    worker: Arc<MonitorRuntime>,
}
impl Default for AppState {
    fn default() -> Self {
        Self {
            worker: Arc::new(MonitorRuntime::new(Duration::from_secs(1))),
        }
    }
}
impl AppState {
    pub fn pending_recording(&self) -> Option<Arc<MonitorRuntime>> {
        let status = self.worker.recording_status();
        (status.active || status.finishing).then(|| self.worker.clone())
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
#[derive(Serialize)]
pub struct MonitorTools {
    history: HistoryResponse,
    events: Vec<AlertEvent>,
    alert_config: AlertConfig,
    recording: RecordingStatus,
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
#[tauri::command]
pub fn get_monitor_tools(state: State<AppState>, window_ms: u64) -> MonitorTools {
    MonitorTools {
        history: state.worker.history(window_ms),
        events: state.worker.events(),
        alert_config: state.worker.alert_config(),
        recording: state.worker.recording_status(),
    }
}
#[tauri::command]
pub fn configure_alerts(state: State<AppState>, config: AlertConfig) -> Result<(), CommandError> {
    state.worker.configure_alerts(config).map_err(Into::into)
}
#[tauri::command]
pub async fn start_recording(
    state: State<'_, AppState>,
    path: String,
    include_commands: bool,
) -> Result<RecordingStatus, CommandError> {
    let worker = state.worker.clone();
    tauri::async_runtime::spawn_blocking(move || {
        worker.start_recording(PathBuf::from(path), include_commands)
    })
    .await
    .map_err(|error| CommandError::from(error.to_string()))?
    .map_err(Into::into)
}
#[tauri::command]
pub fn stop_recording(state: State<AppState>) -> Result<RecordingStatus, CommandError> {
    state.worker.stop_recording().map_err(Into::into)
}
#[tauri::command]
pub async fn load_recording(path: String) -> Result<Vec<MonitorSnapshot>, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        gpu_monitor_runtime::load_recording(&PathBuf::from(path))
    })
    .await
    .map_err(|error| CommandError::from(error.to_string()))?
    .map_err(Into::into)
}
/// Only existing runtime events can become notifications; the frontend opts in.
#[tauri::command]
pub async fn notify_event(state: State<'_, AppState>, event_id: u64) -> Result<(), CommandError> {
    let event = state
        .worker
        .events()
        .into_iter()
        .find(|event| event.id == event_id)
        .ok_or_else(|| {
            CommandError::from("This monitoring event is no longer available".to_owned())
        })?;
    tauri::async_runtime::spawn_blocking(move || {
        notify_rust::Notification::new()
            .summary("GPU Monitor")
            .body(&event.message)
            .timeout(6000)
            .show()
            .map(|_| ())
    })
    .await
    .map_err(|error| CommandError::from(error.to_string()))?
    .map_err(|error| CommandError::from(format!("Desktop notification unavailable: {error}")))
}
