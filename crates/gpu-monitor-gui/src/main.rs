//! GPU Monitor GUI - Tauri main entry point

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use gpu_monitor_gui_lib::AppState;

fn main() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            gpu_monitor_gui_lib::get_gpu_info,
            gpu_monitor_gui_lib::get_gpu_count,
            gpu_monitor_gui_lib::is_gpu_available,
            gpu_monitor_gui_lib::retry_gpu_monitor
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
