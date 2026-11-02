//! メニューバーのアイコンと、そこから開く小窓。

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_positioner::{Position, WindowExt};

use crate::scheduler;
use crate::state::AppState;

const TRAY_ID: &str = "main";
pub const POPOVER: &str = "popover";
pub const SETTINGS: &str = "settings";

/// メニューバーにアイコンを置く。
pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let settings = MenuItem::with_id(app, "settings", "設定…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "終了", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&settings, &quit])?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(Image::from_bytes(include_bytes!("../icons/tray@2x.png"))?)
        .icon_as_template(true)
        .tooltip("newsbar")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "settings" => {
                if let Err(e) = open_settings(app) {
                    eprintln!("設定画面を開けませんでした: {e}");
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            let app = tray.app_handle();
            tauri_plugin_positioner::on_tray_event(app, &event);
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_popover(app);
            }
        })
        .build(app)?;
    Ok(())
}

/// 小窓を出し入れする。
fn toggle_popover(app: &AppHandle) {
    let Some(window) = app.get_webview_window(POPOVER) else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
        return;
    }
    let _ = window.move_window(Position::TrayBottomCenter);
    let _ = window.show();
    let _ = window.set_focus();
    let _ = app.emit_to(POPOVER, "popover-shown", ());

    // スリープ明けなどで更新の時刻を過ぎていたら、開いたときにすぐ更新を始める
    if scheduler::refresh_is_due(app) {
        app.state::<AppState>().wake.notify_one();
    }
}

/// メニューバーのアイコン横に未読件数を出す（0件なら消す）。
pub fn set_unread(app: &AppHandle, count: u32) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let title = (count > 0).then(|| count.to_string());
        let _ = tray.set_title(title);
    }
}

/// 設定画面を開く（なければ作る）。
pub fn open_settings(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(SETTINGS) {
        window.show()?;
        window.set_focus()?;
        return Ok(());
    }
    if let Some(popover) = app.get_webview_window(POPOVER) {
        let _ = popover.hide();
    }
    let window = WebviewWindowBuilder::new(app, SETTINGS, WebviewUrl::App("settings.html".into()))
        .title("newsbar の設定")
        .inner_size(560.0, 640.0)
        .resizable(true)
        .center()
        .build()?;
    window.set_focus()?;
    Ok(())
}
