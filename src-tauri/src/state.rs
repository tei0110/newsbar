//! アプリ全体で共有する状態と、設定の読み書き。

use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

use chrono::{DateTime, Utc};
use newsbar_core::store::Store;
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;

/// 自動更新の間隔として選べる値（分）
pub const INTERVAL_CHOICES: [u32; 4] = [30, 60, 180, 360];
pub const DEFAULT_INTERVAL: u32 = 60;
const KEY_INTERVAL: &str = "refresh_interval_minutes";

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
}

/// 保存済みの設定を読む。未保存や壊れた値は初期値にする。
pub fn load_settings(store: &Store) -> SettingsDto {
    let interval = store
        .get_setting(KEY_INTERVAL)
        .ok()
        .flatten()
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|v| INTERVAL_CHOICES.contains(v))
        .unwrap_or(DEFAULT_INTERVAL);
    SettingsDto {
        refresh_interval_minutes: interval,
    }
}

/// 設定を保存する。選べない値なら保存せずに理由を返す。
pub fn save_settings(store: &Store, settings: &SettingsDto) -> Result<(), String> {
    if !INTERVAL_CHOICES.contains(&settings.refresh_interval_minutes) {
        return Err("自動更新の間隔は 30分・1時間・3時間・6時間 から選んでください".to_string());
    }
    store
        .set_setting(KEY_INTERVAL, &settings.refresh_interval_minutes.to_string())
        .map_err(|e| format!("設定を保存できませんでした: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_when_nothing_saved() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(
            load_settings(&store),
            SettingsDto {
                refresh_interval_minutes: 60
            }
        );
    }

    #[test]
    fn saved_interval_is_loaded() {
        let store = Store::open_in_memory().unwrap();
        let s = SettingsDto {
            refresh_interval_minutes: 180,
        };
        save_settings(&store, &s).unwrap();
        assert_eq!(load_settings(&store), s);
    }

    #[test]
    fn invalid_interval_is_rejected() {
        let store = Store::open_in_memory().unwrap();
        let r = save_settings(
            &store,
            &SettingsDto {
                refresh_interval_minutes: 5,
            },
        );
        assert!(r.is_err());
        assert_eq!(load_settings(&store).refresh_interval_minutes, 60);
    }

    #[test]
    fn broken_saved_value_falls_back_to_default() {
        let store = Store::open_in_memory().unwrap();
        store.set_setting(KEY_INTERVAL, "abc").unwrap();
        assert_eq!(load_settings(&store).refresh_interval_minutes, 60);
    }
}
