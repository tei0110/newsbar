//! アプリ全体で共有する状態と、設定の読み書き。

use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

use chrono::{DateTime, Utc};
use newsbar_core::store::Store;
use newsbar_core::summarize::claude::DEFAULT_MODEL;
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;

use crate::secrets::KeyCache;

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
    /// キーチェーンから読んだ APIキー（起動中に1回だけ読む）
    pub api_key: KeyCache,
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

const KEY_READER_GEOMETRY: &str = "reader_geometry";
/// 「広げて読む」画面の最小の大きさ（見た目の大きさ）
pub const READER_MIN_SIZE: (f64, f64) = (720.0, 460.0);

/// ウィンドウの位置と大きさ。画面の画素密度に左右されない「見た目の大きさ」（論理ピクセル）で持つ。
/// 実際の画素数（物理ピクセル）で持つと、Retina と等倍の外部ディスプレイを行き来したときに
/// 大きさが半分・倍にずれるため。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Geometry {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// 保存してある「広げて読む」画面の位置と大きさ。未保存や壊れた値なら `None`
pub fn load_reader_geometry(store: &Store) -> Option<Geometry> {
    let json = store.get_setting(KEY_READER_GEOMETRY).ok().flatten()?;
    serde_json::from_str(&json).ok()
}

/// 「広げて読む」画面の位置と大きさを保存する
pub fn save_reader_geometry(store: &Store, geometry: &Geometry) -> Result<(), String> {
    let json = serde_json::to_string(geometry).map_err(|e| e.to_string())?;
    store
        .set_setting(KEY_READER_GEOMETRY, &json)
        .map_err(|e| format!("画面の大きさを保存できませんでした: {e}"))
}

/// 保存してある位置と大きさを、そのまま使ってよい形に直す。
///
/// - 大きさが最小より小さければ最小まで広げる
/// - 左上がどの画面（`screens`、見た目の大きさ）にも入っていなければ、位置は使わない（`None`、中央に開く）
pub fn usable_geometry(saved: Geometry, screens: &[Geometry]) -> (f64, f64, Option<(f64, f64)>) {
    let width = saved.width.max(READER_MIN_SIZE.0);
    let height = saved.height.max(READER_MIN_SIZE.1);
    let on_screen = screens.iter().any(|s| {
        saved.x >= s.x && saved.x < s.x + s.width && saved.y >= s.y && saved.y < s.y + s.height
    });
    (width, height, on_screen.then_some((saved.x, saved.y)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geometry(x: f64, y: f64, width: f64, height: f64) -> Geometry {
        Geometry {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn reader_geometry_round_trips() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(load_reader_geometry(&store), None);
        let g = geometry(120.0, 80.0, 1040.0, 720.0);
        save_reader_geometry(&store, &g).unwrap();
        assert_eq!(load_reader_geometry(&store), Some(g));
    }

    #[test]
    fn broken_reader_geometry_is_ignored() {
        let store = Store::open_in_memory().unwrap();
        store.set_setting(KEY_READER_GEOMETRY, "not json").unwrap();
        assert_eq!(load_reader_geometry(&store), None);
    }

    #[test]
    fn too_small_size_is_raised_to_minimum() {
        let screens = [geometry(0.0, 0.0, 1440.0, 900.0)];
        let (w, h, _) = usable_geometry(geometry(100.0, 100.0, 300.0, 200.0), &screens);
        assert_eq!((w, h), READER_MIN_SIZE);
    }

    #[test]
    fn position_on_a_screen_is_kept() {
        // 内蔵ディスプレイの右に外部ディスプレイがある
        let screens = [
            geometry(0.0, 0.0, 1440.0, 900.0),
            geometry(1440.0, 0.0, 3440.0, 1440.0),
        ];
        let saved = geometry(2000.0, 300.0, 1040.0, 720.0);
        assert_eq!(
            usable_geometry(saved, &screens),
            (1040.0, 720.0, Some((2000.0, 300.0)))
        );
    }

    #[test]
    fn position_off_every_screen_is_dropped() {
        // 外部ディスプレイを外したあと、そこにあった位置は使わない
        let screens = [geometry(0.0, 0.0, 1440.0, 900.0)];
        let saved = geometry(2000.0, 300.0, 1040.0, 720.0);
        assert_eq!(usable_geometry(saved, &screens), (1040.0, 720.0, None));
    }

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
