//! アプリ全体で共有する状態と、設定の読み書き。

use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

use chrono::{DateTime, Utc};
use newsbar_core::store::Store;
use newsbar_core::summarize::claude::DEFAULT_MODEL;
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;

/// 自動更新の間隔として選べる値（分）
pub const INTERVAL_CHOICES: [u32; 4] = [30, 60, 180, 360];
pub const DEFAULT_INTERVAL: u32 = 60;
const KEY_INTERVAL: &str = "refresh_interval_minutes";
const KEY_MODEL: &str = "model";
const KEY_LANGUAGE: &str = "language";

pub struct AppState {
    pub store: Mutex<Store>,
    pub http: reqwest::Client,
    pub status: Mutex<AppStatus>,
    /// 次の更新をすぐ始めたいときに通知する
    pub wake: Notify,
    pub refreshing: AtomicBool,
}

/// 画面に出す更新の状態
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct AppStatus {
    pub last_refreshed_at: Option<DateTime<Utc>>,
    pub feed_error_count: usize,
    pub summary_error: Option<String>,
    pub refreshing: bool,
}

/// 設定画面でやり取りする設定
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettingsDto {
    pub refresh_interval_minutes: u32,
    pub model: String,
    pub language: String,
}

/// 要約の言語として選べる値
pub const LANGUAGE_CHOICES: [&str; 2] = ["ja", "en"];
pub const DEFAULT_LANGUAGE: &str = "ja";

/// 保存済みの設定を読む。未保存や壊れた値は初期値にする。
pub fn load_settings(store: &Store) -> SettingsDto {
    let get = |key: &str| store.get_setting(key).ok().flatten();
    let interval = get(KEY_INTERVAL)
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|v| INTERVAL_CHOICES.contains(v))
        .unwrap_or(DEFAULT_INTERVAL);
    let model = get(KEY_MODEL)
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_MODEL.to_string());
    let language = get(KEY_LANGUAGE)
        .filter(|l| LANGUAGE_CHOICES.contains(&l.as_str()))
        .unwrap_or_else(|| DEFAULT_LANGUAGE.to_string());
    SettingsDto {
        refresh_interval_minutes: interval,
        model,
        language,
    }
}

/// 設定を保存する。選べない値なら、どれも保存せずに理由を返す。
pub fn save_settings(store: &Store, settings: &SettingsDto) -> Result<(), String> {
    if !INTERVAL_CHOICES.contains(&settings.refresh_interval_minutes) {
        return Err("自動更新の間隔は 30分・1時間・3時間・6時間 から選んでください".to_string());
    }
    let model = settings.model.trim();
    if model.is_empty() {
        return Err("モデルを入力してください".to_string());
    }
    if !LANGUAGE_CHOICES.contains(&settings.language.as_str()) {
        return Err("要約の言語は 日本語・English から選んでください".to_string());
    }
    let save = |key: &str, value: &str| {
        store
            .set_setting(key, value)
            .map_err(|e| format!("設定を保存できませんでした: {e}"))
    };
    save(KEY_INTERVAL, &settings.refresh_interval_minutes.to_string())?;
    save(KEY_MODEL, model)?;
    save(KEY_LANGUAGE, &settings.language)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(interval: u32, model: &str, language: &str) -> SettingsDto {
        SettingsDto {
            refresh_interval_minutes: interval,
            model: model.to_string(),
            language: language.to_string(),
        }
    }

    #[test]
    fn defaults_when_nothing_saved() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(
            load_settings(&store),
            settings(60, "claude-haiku-5-5", "ja")
        );
    }

    #[test]
    fn saved_settings_are_loaded() {
        let store = Store::open_in_memory().unwrap();
        let s = settings(180, "claude-sonnet-5-5", "en");
        save_settings(&store, &s).unwrap();
        assert_eq!(load_settings(&store), s);
    }

    #[test]
    fn model_is_trimmed() {
        let store = Store::open_in_memory().unwrap();
        save_settings(&store, &settings(60, "  claude-haiku-5-5\n", "ja")).unwrap();
        assert_eq!(load_settings(&store).model, "claude-haiku-5-5");
    }

    #[test]
    fn invalid_interval_is_rejected() {
        let store = Store::open_in_memory().unwrap();
        let r = save_settings(&store, &settings(5, "claude-haiku-5-5", "ja"));
        assert!(r.is_err());
        assert_eq!(load_settings(&store).refresh_interval_minutes, 60);
    }

    #[test]
    fn blank_model_and_unknown_language_are_rejected() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(
            save_settings(&store, &settings(60, "   ", "ja")).unwrap_err(),
            "モデルを入力してください"
        );
        assert_eq!(
            save_settings(&store, &settings(60, "claude-haiku-5-5", "fr")).unwrap_err(),
            "要約の言語は 日本語・English から選んでください"
        );
        assert_eq!(
            load_settings(&store),
            settings(60, "claude-haiku-5-5", "ja")
        );
    }

    #[test]
    fn broken_saved_value_falls_back_to_default() {
        let store = Store::open_in_memory().unwrap();
        store.set_setting(KEY_INTERVAL, "abc").unwrap();
        store.set_setting(KEY_LANGUAGE, "xx").unwrap();
        let loaded = load_settings(&store);
        assert_eq!(loaded.refresh_interval_minutes, 60);
        assert_eq!(loaded.language, "ja");
    }
}
