//! newsbar の Mac アプリ本体（メニューバー・小窓・設定画面）。

mod tray;

use tauri::{Manager, WindowEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_positioner::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // Dock にアイコンを出さない（メニューバー常駐アプリ）
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

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
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("newsbar を起動できませんでした");
}
