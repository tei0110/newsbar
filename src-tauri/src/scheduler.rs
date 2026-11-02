//! 自動更新。起動直後に1回、その後は設定の間隔ごと（または設定変更などで起こされたとき）に更新する。

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use chrono::{DateTime, Utc};
use newsbar_core::refresh::{RefreshDeps, RefreshReport, refresh};
use newsbar_core::summarize::Summarizer;
use newsbar_core::summarize::claude::ClaudeSummarizer;
use tauri::{AppHandle, Emitter, Manager};

use crate::state::{AppState, load_settings};
use crate::{secrets, tray};

pub const ARTICLES_UPDATED: &str = "articles-updated";

/// 次の更新の時刻を過ぎたかを確かめる間隔。スリープ明けは、遅くともこの時間で更新が始まる
const CHECK_INTERVAL: Duration = Duration::from_secs(60);

/// 自動更新を始める。
pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            run_refresh(&app).await;
            let state = app.state::<AppState>();
            // 待ち時間ではなく実際の時刻で判断する（is_due の説明を参照）
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(CHECK_INTERVAL) => {
                        if refresh_is_due(&app) {
                            break;
                        }
                    }
                    _ = state.wake.notified() => break,
                }
            }
        }
    });
}

/// 前回の更新から `interval_minutes` 分以上たったか（まだ一度も更新していなければ true）。
///
/// 実際の時刻で比べる。`tokio::time::sleep` の時計は Mac のスリープ中に止まるので、
/// 待ち時間だけに頼るとスリープ明けの更新が最大で更新間隔ぶん遅れるため。
pub fn is_due(
    last_refreshed_at: Option<DateTime<Utc>>,
    interval_minutes: u32,
    now: DateTime<Utc>,
) -> bool {
    last_refreshed_at
        .is_none_or(|last| now - last >= chrono::Duration::minutes(i64::from(interval_minutes)))
}

/// 今すぐ更新すべきか（設定の間隔と、前回の更新時刻から判断する）。更新中なら false
pub fn refresh_is_due(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();
    if state.refreshing.load(Ordering::SeqCst) {
        return false;
    }
    let minutes = {
        let store = state.store.lock().unwrap_or_else(|p| p.into_inner());
        load_settings(&store).refresh_interval_minutes
    };
    let last = state.status.lock().ok().and_then(|s| s.last_refreshed_at);
    is_due(last, minutes, Utc::now())
}

/// 1回更新する。すでに更新中なら何もしないで `None`。
pub async fn run_refresh(app: &AppHandle) -> Option<RefreshReport> {
    let state = app.state::<AppState>();
    if state.refreshing.swap(true, Ordering::SeqCst) {
        return None;
    }
    set_refreshing(app, true);

    let settings = {
        let store = state.store.lock().unwrap_or_else(|p| p.into_inner());
        load_settings(&store)
    };
    let deps = RefreshDeps {
        http: state.http.clone(),
        summarizer: make_summarizer(&state, &settings.model),
        language: settings.language,
    };
    let report = refresh(&state.store, &deps, Utc::now()).await;

    if let Ok(mut status) = state.status.lock() {
        status.last_refreshed_at = Some(Utc::now());
        status.feed_error_count = report.feed_errors.len();
        status.summary_error = report.summary_error.clone();
    }
    state.refreshing.store(false, Ordering::SeqCst);
    set_refreshing(app, false);
    update_unread(app);
    Some(report)
}

/// APIキーがあれば、Claude の要約役を作る。キーチェーンを読むのは起動中に1回だけ。
fn make_summarizer(state: &AppState, model: &str) -> Option<Arc<dyn Summarizer>> {
    let key = state.api_key.get(secrets::load_api_key)?;
    let summarizer = ClaudeSummarizer::new(state.http.clone(), &key, model).ok()?;
    Some(Arc::new(summarizer))
}

fn set_refreshing(app: &AppHandle, refreshing: bool) {
    let state = app.state::<AppState>();
    if let Ok(mut status) = state.status.lock() {
        status.refreshing = refreshing;
    }
    let _ = app.emit(ARTICLES_UPDATED, ());
}

/// メニューバーの未読件数を更新し、画面に知らせる。
pub fn update_unread(app: &AppHandle) {
    let state = app.state::<AppState>();
    let total = state
        .store
        .lock()
        .ok()
        .and_then(|s| s.unread_counts().ok())
        .map(|c| c.total)
        .unwrap_or(0);
    tray::set_unread(app, total);
    let _ = app.emit(ARTICLES_UPDATED, ());
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration as ChronoDuration, TimeZone};

    fn at(hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 2, hour, minute, 0).unwrap()
    }

    #[test]
    fn never_refreshed_is_due() {
        assert!(is_due(None, 60, at(8, 0)));
    }

    #[test]
    fn due_only_after_interval_has_passed() {
        let last = Some(at(7, 0));
        assert!(!is_due(last, 60, at(7, 59)));
        assert!(is_due(last, 60, at(8, 0)));
    }

    #[test]
    fn overnight_sleep_is_due_on_wake() {
        // 前の日の 23:10 に更新して Mac がスリープし、翌朝 8:00 に起きた
        let last = Some(at(23, 10) - ChronoDuration::days(1));
        assert!(is_due(last, 60, at(8, 0)));
    }
}
