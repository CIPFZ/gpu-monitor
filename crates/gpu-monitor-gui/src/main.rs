//! GPU Monitor GUI - Tauri main entry point

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
use commands::{get_gpu_count, get_gpu_info, is_gpu_available, AppState};

fn main() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            get_gpu_info,
            get_gpu_count,
            is_gpu_available
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
