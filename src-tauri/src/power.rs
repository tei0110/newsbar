//! macOS のスリープ・起床の通知を受けて、`AppState::sleep` を切り替える。
//!
//! 「スリープに入る」（NSWorkspaceWillSleepNotification）で眠り、
//! 「完全に起きた」（NSWorkspaceDidWakeNotification）で起きる。
//! DarkWake の間は「完全に起きた」が来ないので、自動更新は眠ったままとして止まる。

use tauri::AppHandle;

#[cfg(target_os = "macos")]
pub fn watch_system_sleep(app: &AppHandle) {
    use std::ptr::NonNull;

    use block2::RcBlock;
    use objc2_app_kit::{
        NSWorkspace, NSWorkspaceDidWakeNotification, NSWorkspaceWillSleepNotification,
    };
    use objc2_foundation::NSNotification;
    use tauri::Manager;

    use crate::state::AppState;

    let center = NSWorkspace::sharedWorkspace().notificationCenter();

    let handle = app.clone();
    let on_will_sleep = RcBlock::new(move |_: NonNull<NSNotification>| {
        handle.state::<AppState>().sleep.will_sleep();
    });
    let handle = app.clone();
    let on_did_wake = RcBlock::new(move |_: NonNull<NSNotification>| {
        handle.state::<AppState>().sleep.did_wake();
    });

    // SAFETY: 通知名は AppKit が用意する定数。ブロックは AppHandle（Send + Sync）だけを持つ。
    // queue に None を渡すと、通知を出したスレッド（メインスレッド）で呼ばれる。
    unsafe {
        let will_sleep = center.addObserverForName_object_queue_usingBlock(
            Some(NSWorkspaceWillSleepNotification),
            None,
            None,
            &on_will_sleep,
        );
        let did_wake = center.addObserverForName_object_queue_usingBlock(
            Some(NSWorkspaceDidWakeNotification),
            None,
            None,
            &on_did_wake,
        );
        // アプリが動いている間ずっと受け取るので、登録を解除しない（手放すと解除される）
        std::mem::forget(will_sleep);
        std::mem::forget(did_wake);
    }
}

#[cfg(not(target_os = "macos"))]
pub fn watch_system_sleep(_app: &AppHandle) {}
