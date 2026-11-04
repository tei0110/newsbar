//! メニューバーのアイコンと、そこから開く小窓。

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_positioner::{Position, WindowExt};

use crate::scheduler;
use crate::state::{
    AppState, Geometry, READER_MIN_SIZE, load_reader_geometry, save_reader_geometry,
    usable_geometry,
};

const TRAY_ID: &str = "main";
pub const POPOVER: &str = "popover";
pub const SETTINGS: &str = "settings";
pub const READER: &str = "reader";

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

/// 「広げて読む」画面（左に一覧、右に詳しい要約）を開く。開いていれば前に出す。
///
/// - 前回閉じたときの位置と大きさ（見た目の大きさで保存）で開く
/// - 開いている間だけ Dock と Cmd+Tab に出す（ふだんはメニューバー常駐なので出さない）
pub fn open_reader(app: &AppHandle) -> tauri::Result<()> {
    if let Some(popover) = app.get_webview_window(POPOVER) {
        let _ = popover.hide();
    }
    #[cfg(target_os = "macos")]
    app.set_activation_policy(tauri::ActivationPolicy::Regular)?;

    if let Some(window) = app.get_webview_window(READER) {
        window.show()?;
        window.set_focus()?;
        return Ok(());
    }

    let saved = {
        let state = app.state::<AppState>();
        let store = state.store.lock().unwrap_or_else(|p| p.into_inner());
        load_reader_geometry(&store)
    };
    let (min_width, min_height) = READER_MIN_SIZE;
    let mut builder = WebviewWindowBuilder::new(app, READER, WebviewUrl::App("reader.html".into()))
        .title("newsbar")
        .min_inner_size(min_width, min_height)
        .resizable(true);
    builder = match saved {
        Some(saved) => {
            let (width, height, position) = usable_geometry(saved, &screens(app));
            builder = builder.inner_size(width, height);
            match position {
                Some((x, y)) => builder.position(x, y),
                None => builder.center(),
            }
        }
        None => builder.inner_size(1040.0, 720.0).center(),
    };
    let window = builder.build()?;

    let handle = window.clone();
    window.on_window_event(move |event| match event {
        // 閉じる前に、位置と大きさを見た目の大きさで保存する
        WindowEvent::CloseRequested { .. } => save_geometry(&handle),
        // 閉じたら、メニューバーだけの常駐に戻す
        WindowEvent::Destroyed => {
            #[cfg(target_os = "macos")]
            let _ = handle
                .app_handle()
                .set_activation_policy(tauri::ActivationPolicy::Accessory);
        }
        _ => {}
    });
    window.set_focus()?;
    Ok(())
}

/// 画面ごとの範囲（見た目の大きさ）
fn screens(app: &AppHandle) -> Vec<Geometry> {
    app.available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| {
            let scale = m.scale_factor();
            let pos = m.position().to_logical::<f64>(scale);
            let size = m.size().to_logical::<f64>(scale);
            Geometry {
                x: pos.x,
                y: pos.y,
                width: size.width,
                height: size.height,
            }
        })
        .collect()
}

fn save_geometry(window: &tauri::WebviewWindow) {
    let (Ok(scale), Ok(pos), Ok(size)) = (
        window.scale_factor(),
        window.outer_position(),
        window.inner_size(),
    ) else {
        return;
    };
    let pos = pos.to_logical::<f64>(scale);
    let size = size.to_logical::<f64>(scale);
    let geometry = Geometry {
        x: pos.x,
        y: pos.y,
        width: size.width,
        height: size.height,
    };
    let state = window.state::<AppState>();
    let store = state.store.lock().unwrap_or_else(|p| p.into_inner());
    let _ = save_reader_geometry(&store, &geometry);
}
