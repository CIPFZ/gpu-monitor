//! Tauri IPC commands for GPU monitoring

use gpu_monitor_core::{GpuInfo, GpuMonitor};
use serde::Serialize;
use std::sync::Mutex;
use tauri::State;

/// Application state holding the GPU monitor instance
pub struct AppState {
    pub monitor: Mutex<Option<GpuMonitor>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            monitor: Mutex::new(GpuMonitor::new().ok()),
        }
    }
}

/// Error response for IPC commands
#[derive(Debug, Serialize)]
pub struct CommandError {
    pub message: String,
}

impl From<gpu_monitor_core::Error> for CommandError {
    fn from(err: gpu_monitor_core::Error) -> Self {
        Self {
            message: err.to_string(),
        }
    }
}

/// Get all GPU information
#[tauri::command]
pub fn get_gpu_info(state: State<AppState>) -> Result<Vec<GpuInfo>, CommandError> {
    let guard = state.monitor.lock().map_err(|e| CommandError {
        message: format!("Failed to acquire lock: {}", e),
    })?;

    match guard.as_ref() {
        Some(monitor) => monitor.get_all_gpu_info().map_err(|e| e.into()),
        None => Err(CommandError {
            message: "GPU monitor not initialized. Make sure NVIDIA drivers are installed."
                .to_string(),
        }),
    }
}

/// Get GPU count
#[tauri::command]
pub fn get_gpu_count(state: State<AppState>) -> Result<u32, CommandError> {
    let guard = state.monitor.lock().map_err(|e| CommandError {
        message: format!("Failed to acquire lock: {}", e),
    })?;

    match guard.as_ref() {
        Some(monitor) => monitor.device_count().map_err(|e| e.into()),
        None => Err(CommandError {
            message: "GPU monitor not initialized".to_string(),
        }),
    }
}

/// Check if GPU monitoring is available
#[tauri::command]
pub fn is_gpu_available(state: State<AppState>) -> bool {
    let guard = state.monitor.lock();
    match guard {
        Ok(g) => g.is_some(),
        Err(_) => false,
    }
}
