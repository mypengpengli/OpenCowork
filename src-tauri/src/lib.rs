mod analysis;
mod assistant;
mod capture;
mod commands;
mod model;
mod skills;
mod storage;

use crate::skills::start_skills_watcher;
use crate::storage::StorageManager;
use commands::{
    cancel_request,
    chat_with_assistant,
    clear_all_summaries,
    clear_summaries,
    close_notification,
    create_skill,
    delete_profile,
    delete_skill,
    ensure_bash_runtime,
    focus_main_window,
    get_capture_status,
    get_config,
    get_recent_alerts,
    get_skill,
    get_skills_dir,
    get_summaries,
    get_system_locale,
    invoke_skill,
    list_profiles,
    // Skills 相关命令
    list_skills,
    load_profile,
    log_ui_locale,
    open_external_url,
    open_release_page,
    open_screenshots_dir,
    open_skills_dir,
    read_image_base64,
    save_clipboard_image,
    save_config,
    save_profile,
    // 通知窗口相关命令
    show_notification,
    start_capture,
    stop_capture,
    test_model_connection,
    AppState,
};
use std::sync::Arc;
use tauri::Manager;

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
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState::new())
        .setup(|app| {
            let state = app.state::<AppState>();
            let skills_version = Arc::clone(&state.skills_version);
            let on_changed = Arc::new(move || {
                skills_version.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            });
            match start_skills_watcher(&app.handle(), Some(on_changed)) {
                Ok(watcher) => {
                    let mut guard = state.skills_watcher.lock().unwrap();
                    *guard = Some(watcher);
                }
                Err(err) => {
                    eprintln!("Skills watcher init failed: {}", err);
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            get_system_locale,
            log_ui_locale,
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
            cancel_request,
            get_summaries,
            get_recent_alerts,
            clear_summaries,
            clear_all_summaries,
            open_screenshots_dir,
            open_release_page,
            open_external_url,
            save_clipboard_image,
            read_image_base64,
            ensure_bash_runtime,
            // Skills 相关命令
            list_skills,
            get_skill,
            invoke_skill,
            create_skill,
            delete_skill,
            get_skills_dir,
            open_skills_dir,
            // 通知窗口相关命令
            show_notification,
            close_notification,
            focus_main_window,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
