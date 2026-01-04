mod commands;
mod capture;
mod model;
mod storage;
mod analysis;
mod assistant;

use commands::{
    AppState,
    get_config, save_config, test_model_connection,
    start_capture, stop_capture, get_capture_status,
    chat_with_assistant, get_summaries,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            test_model_connection,
            start_capture,
            stop_capture,
            get_capture_status,
            chat_with_assistant,
            get_summaries,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
