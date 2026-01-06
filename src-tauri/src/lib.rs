mod commands;
mod capture;
mod model;
mod storage;
mod analysis;
mod assistant;

use crate::storage::StorageManager;
use commands::{
    AppState,
    get_config, save_config, list_profiles, save_profile, load_profile, delete_profile,
    test_model_connection,
    start_capture, stop_capture, get_capture_status,
    chat_with_assistant, get_summaries,
    get_recent_alerts,
    clear_summaries, clear_all_summaries,
    open_screenshots_dir,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let storage = StorageManager::new();
    if let Ok(config) = storage.load_config() {
        if config.storage.auto_clear_on_start {
            if let Err(err) = storage.delete_all_summaries() {
                eprintln!("启动清空历史失败: {}", err);
            }
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            list_profiles,
            save_profile,
            load_profile,
            delete_profile,
            test_model_connection,
            start_capture,
            stop_capture,
            get_capture_status,
            chat_with_assistant,
            get_summaries,
            get_recent_alerts,
            clear_summaries,
            clear_all_summaries,
            open_screenshots_dir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
