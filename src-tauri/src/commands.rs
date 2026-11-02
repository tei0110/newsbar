//! 画面（TypeScript）から呼ぶコマンド。エラーはすべて日本語の文字列で返す。

use chrono::Utc;
use newsbar_core::model::{Article, Source, UnreadCounts};
use newsbar_core::presets::RECOMMENDED_SOURCES;
use newsbar_core::refresh::{RefreshReport, validate_feed_url};
use newsbar_core::store::Store;
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::display::age_label;
use crate::state::{
    AppState, AppStatus, SettingsDto, load_settings, save_settings as store_settings,
};
use crate::{scheduler, secrets, tray};

/// 画面に渡す記事
#[derive(Debug, Clone, Serialize)]
pub struct ArticleDto {
    #[serde(flatten)]
    pub article: Article,
    pub age_label: String,
}

/// おすすめの情報源と、追加済みかどうか
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecommendedDto {
    pub name: String,
    pub feed_url: String,
    pub label: String,
    pub added: bool,
}

/// 情報源として選べる分野ラベル
pub const LABELS: [&str; 3] = ["AI", "セキュリティ", "技術全般"];

fn recommended_with_status(sources: &[Source]) -> Vec<RecommendedDto> {
    RECOMMENDED_SOURCES
        .iter()
        .map(|p| RecommendedDto {
            name: p.name.to_string(),
            feed_url: p.feed_url.to_string(),
            label: p.label.to_string(),
            added: sources.iter().any(|s| s.feed_url == p.feed_url),
        })
        .collect()
}

fn check_new_source(name: &str, label: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("名前を入力してください".to_string());
    }
    if !LABELS.contains(&label) {
        return Err("分野は AI・セキュリティ・技術全般 から選んでください".to_string());
    }
    Ok(())
}

fn with_store<T>(
    state: &AppState,
    f: impl FnOnce(&Store) -> Result<T, newsbar_core::store::StoreError>,
) -> Result<T, String> {
    let store = state
        .store
        .lock()
        .map_err(|_| "データベースを使えません".to_string())?;
    f(&store).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_articles(
    state: State<'_, AppState>,
    label: Option<String>,
    include_read: bool,
) -> Result<Vec<ArticleDto>, String> {
    let now = Utc::now();
    let articles = with_store(&state, |s| s.list_articles(label.as_deref(), include_read))?;
    Ok(articles
        .into_iter()
        .map(|article| ArticleDto {
            age_label: age_label(article.published_at, now),
            article,
        })
        .collect())
}

#[tauri::command]
pub fn unread_counts(state: State<'_, AppState>) -> Result<UnreadCounts, String> {
    with_store(&state, |s| s.unread_counts())
}

#[tauri::command]
pub fn mark_read(app: AppHandle, state: State<'_, AppState>, id: i64) -> Result<(), String> {
    with_store(&state, |s| s.mark_read(id, Utc::now()))?;
    scheduler::update_unread(&app);
    Ok(())
}

#[tauri::command]
pub fn mark_all_read(
    app: AppHandle,
    state: State<'_, AppState>,
    label: Option<String>,
) -> Result<usize, String> {
    let n = with_store(&state, |s| s.mark_all_read(label.as_deref(), Utc::now()))?;
    scheduler::update_unread(&app);
    Ok(n)
}

#[tauri::command]
pub fn open_url(app: AppHandle, url: String) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("開けない URL です".to_string());
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| format!("ブラウザで開けませんでした: {e}"))
}

#[tauri::command]
pub async fn refresh_now(app: AppHandle) -> Result<RefreshReport, String> {
    scheduler::run_refresh(&app)
        .await
        .ok_or_else(|| "いま更新中です".to_string())
}

#[tauri::command]
pub fn get_status(state: State<'_, AppState>) -> Result<AppStatus, String> {
    state
        .status
        .lock()
        .map(|s| s.clone())
        .map_err(|_| "状態を読めません".to_string())
}

#[tauri::command]
pub fn list_sources(state: State<'_, AppState>) -> Result<Vec<Source>, String> {
    with_store(&state, |s| s.list_sources())
}

#[tauri::command]
pub async fn add_source(
    state: State<'_, AppState>,
    name: String,
    feed_url: String,
    label: String,
) -> Result<Source, String> {
    let name = name.trim().to_string();
    let feed_url = feed_url.trim().to_string();
    check_new_source(&name, &label)?;
    validate_feed_url(&state.http, &feed_url).await?;
    let source = with_store(&state, |s| s.add_source(&name, &feed_url, &label, true))?;
    state.wake.notify_one();
    Ok(source)
}

#[tauri::command]
pub fn delete_source(app: AppHandle, state: State<'_, AppState>, id: i64) -> Result<(), String> {
    with_store(&state, |s| s.delete_source(id))?;
    scheduler::update_unread(&app);
    Ok(())
}

#[tauri::command]
pub fn set_source_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    enabled: bool,
) -> Result<(), String> {
    with_store(&state, |s| s.set_source_enabled(id, enabled))?;
    if enabled {
        state.wake.notify_one();
    }
    scheduler::update_unread(&app);
    Ok(())
}

#[tauri::command]
pub fn list_recommended(state: State<'_, AppState>) -> Result<Vec<RecommendedDto>, String> {
    let sources = with_store(&state, |s| s.list_sources())?;
    Ok(recommended_with_status(&sources))
}

#[tauri::command]
pub fn add_recommended(state: State<'_, AppState>, feed_url: String) -> Result<Source, String> {
    let preset = RECOMMENDED_SOURCES
        .iter()
        .find(|p| p.feed_url == feed_url)
        .ok_or_else(|| "おすすめの一覧にない URL です".to_string())?;
    let source = with_store(&state, |s| {
        s.add_source(preset.name, preset.feed_url, preset.label, true)
    })?;
    state.wake.notify_one();
    Ok(source)
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<SettingsDto, String> {
    let store = state
        .store
        .lock()
        .map_err(|_| "データベースを使えません".to_string())?;
    Ok(load_settings(&store))
}

#[tauri::command]
pub fn save_settings(state: State<'_, AppState>, settings: SettingsDto) -> Result<(), String> {
    {
        let store = state
            .store
            .lock()
            .map_err(|_| "データベースを使えません".to_string())?;
        store_settings(&store, &settings)?;
    }
    // 新しい間隔で待ち直す
    state.wake.notify_one();
    Ok(())
}

/// APIキーをキーチェーンに保存する（空なら消す）。保存したらすぐ更新して要約を始める。
#[tauri::command]
pub fn set_api_key(state: State<'_, AppState>, key: String) -> Result<(), String> {
    secrets::save_api_key(&key)?;
    state.api_key.set(secrets::normalize_api_key(&key));
    state.wake.notify_one();
    Ok(())
}

/// APIキーが保存されているか（キーそのものは画面に渡さない）。
#[tauri::command]
pub fn has_api_key(state: State<'_, AppState>) -> bool {
    state.api_key.get(secrets::load_api_key).is_some()
}

#[tauri::command]
pub fn open_settings(app: AppHandle) -> Result<(), String> {
    tray::open_settings(&app).map_err(|e| format!("設定画面を開けませんでした: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(feed_url: &str) -> Source {
        Source {
            id: 1,
            name: "x".into(),
            feed_url: feed_url.into(),
            label: "AI".into(),
            enabled: true,
            last_fetched_at: None,
            last_error: None,
        }
    }

    #[test]
    fn recommended_marks_added_ones() {
        let added_url = RECOMMENDED_SOURCES[0].feed_url;
        let list = recommended_with_status(&[source(added_url)]);
        assert_eq!(list.len(), RECOMMENDED_SOURCES.len());
        assert!(list[0].added);
        assert_eq!(list[0].name, RECOMMENDED_SOURCES[0].name);
        assert!(list[1..].iter().all(|r| !r.added));
    }

    #[test]
    fn new_source_needs_name_and_known_label() {
        assert!(check_new_source("Blog", "AI").is_ok());
        assert_eq!(
            check_new_source("  ", "AI").unwrap_err(),
            "名前を入力してください"
        );
        assert_eq!(
            check_new_source("Blog", "その他").unwrap_err(),
            "分野は AI・セキュリティ・技術全般 から選んでください"
        );
    }
}
