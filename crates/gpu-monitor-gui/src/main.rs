//! GPU Monitor GUI - Tauri main entry point

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use gpu_monitor_gui_lib::AppState;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tauri::Manager;

fn main() {
    let exiting = Arc::new(AtomicBool::new(false));
    let ready = Arc::new(AtomicBool::new(false));
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            gpu_monitor_gui_lib::get_gpu_info,
            gpu_monitor_gui_lib::get_gpu_count,
            gpu_monitor_gui_lib::is_gpu_available,
            gpu_monitor_gui_lib::retry_gpu_monitor,
            gpu_monitor_gui_lib::get_monitor_tools,
            gpu_monitor_gui_lib::configure_alerts,
            gpu_monitor_gui_lib::start_recording,
            gpu_monitor_gui_lib::stop_recording,
            gpu_monitor_gui_lib::load_recording,
            gpu_monitor_gui_lib::notify_event
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(move |app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                if ready.load(Ordering::Acquire) {
                    return;
                }
                if exiting.load(Ordering::Acquire) {
                    api.prevent_exit();
                    return;
                }
                if let Some(worker) = app.state::<AppState>().pending_recording() {
                    api.prevent_exit();
                    if exiting.swap(true, Ordering::AcqRel) {
                        return;
                    }
                    let app = app.clone();
                    let ready = ready.clone();
                    tauri::async_runtime::spawn_blocking(move || {
                        let mut status = worker
                            .stop_recording()
                            .unwrap_or_else(|_| worker.recording_status());
                        let deadline = Instant::now() + Duration::from_secs(5);
                        while status.finishing && Instant::now() < deadline {
                            std::thread::sleep(Duration::from_millis(25));
                            status = worker.recording_status();
                        }
                        let failed = status.finishing
                            || status.error.is_some()
                            || status.dropped_samples > 0;
                        if failed {
                            eprintln!("Recording may be incomplete at shutdown: {:?}", status);
                        }
                        ready.store(true, Ordering::Release);
                        app.exit(i32::from(failed));
                    });
                }
            }
        });
}
