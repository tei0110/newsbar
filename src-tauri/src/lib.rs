//! newsbar の Mac アプリ本体（メニューバー・小窓・設定画面）。

mod autostart;
mod commands;
mod display;
mod scheduler;
mod secrets;
mod state;
mod tray;

use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

use newsbar_core::presets::seed_defaults;
use newsbar_core::store::Store;
use tauri::{Manager, WindowEvent};

use crate::state::{AppState, AppStatus};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_positioner::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::list_articles,
            commands::unread_counts,
            commands::mark_read,
            commands::mark_all_read,
            commands::open_url,
            commands::refresh_now,
            commands::get_status,
            commands::list_sources,
            commands::add_source,
            commands::delete_source,
            commands::set_source_enabled,
            commands::list_recommended,
            commands::add_recommended,
            commands::get_settings,
            commands::save_settings,
            commands::set_api_key,
            commands::has_api_key,
            commands::get_autostart,
            commands::set_autostart,
            commands::open_settings,
            commands::open_reader,
        ])
        .setup(|app| {
            // Dock にアイコンを出さない（メニューバー常駐アプリ）
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            // データベースはアプリのデータフォルダに置く（Sandbox ではコンテナ内になる）
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let store = Store::open(&dir.join("newsbar.db"))?;
            seed_defaults(&store)?;
            app.manage(AppState {
                store: Mutex::new(store),
                http: newsbar_core::http_client(),
                status: Mutex::new(AppStatus::default()),
                wake: tokio::sync::Notify::new(),
                refreshing: AtomicBool::new(false),
                api_key: secrets::KeyCache::new(),
            });

            tray::setup(app.handle())?;

            // 小窓は、外側をクリックしてフォーカスが外れたら隠す
            if let Some(popover) = app.get_webview_window(tray::POPOVER) {
                let handle = popover.clone();
                popover.on_window_event(move |event| {
                    if let WindowEvent::Focused(false) = event {
                        let _ = handle.hide();
                    }
                });
            }

            scheduler::spawn(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("newsbar を起動できませんでした");
}
