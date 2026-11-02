//! 自動更新。起動直後に1回、その後は設定の間隔ごと（または設定変更などで起こされたとき）に更新する。

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use chrono::Utc;
use newsbar_core::refresh::{RefreshDeps, RefreshReport, refresh};
use newsbar_core::summarize::Summarizer;
use newsbar_core::summarize::claude::ClaudeSummarizer;
use tauri::{AppHandle, Emitter, Manager};

use crate::state::{AppState, load_settings};
use crate::{secrets, tray};

pub const ARTICLES_UPDATED: &str = "articles-updated";

/// 自動更新を始める。
pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            run_refresh(&app).await;
            let state = app.state::<AppState>();
            let minutes = {
                let store = state.store.lock().unwrap_or_else(|p| p.into_inner());
                load_settings(&store).refresh_interval_minutes
            };
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(u64::from(minutes) * 60)) => {}
                _ = state.wake.notified() => {}
            }
        }
    });
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
        summarizer: make_summarizer(&state.http, &settings.model),
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

/// キーチェーンに APIキーがあれば、Claude の要約役を作る。
fn make_summarizer(http: &reqwest::Client, model: &str) -> Option<Arc<dyn Summarizer>> {
    let key = secrets::load_api_key().ok().flatten()?;
    let summarizer = ClaudeSummarizer::new(http.clone(), &key, model).ok()?;
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
