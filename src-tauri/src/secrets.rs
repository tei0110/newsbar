//! APIキーの保存場所（macOS のキーチェーン）。データベースやログには書かない。

use std::sync::Mutex;

use keyring::Entry;

const SERVICE: &str = "io.github.tei0110.newsbar";
const ACCOUNT: &str = "anthropic_api_key";

fn entry() -> Result<Entry, String> {
    Entry::new(SERVICE, ACCOUNT).map_err(|e| format!("キーチェーンを使えません: {e}"))
}

/// 前後の空白を除く。空なら `None`（キーを消す、の意味）。
pub fn normalize_api_key(raw: &str) -> Option<String> {
    let key = raw.trim();
    (!key.is_empty()).then(|| key.to_string())
}

/// APIキーを保存する。空なら保存済みのキーを消す。
pub fn save_api_key(raw: &str) -> Result<(), String> {
    let entry = entry()?;
    match normalize_api_key(raw) {
        Some(key) => entry
            .set_password(&key)
            .map_err(|e| format!("APIキーを保存できませんでした: {e}")),
        None => match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("APIキーを消せませんでした: {e}")),
        },
    }
}

/// 保存済みの APIキーを読む。なければ `None`。
pub fn load_api_key() -> Result<Option<String>, String> {
    match entry()?.get_password() {
        Ok(key) => Ok(Some(key)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("APIキーを読めませんでした: {e}")),
    }
}

/// 読み込んだ APIキーを、アプリの起動中だけメモリに覚えておく。
///
/// キーチェーンを読むたびに macOS が確認を出すことがあるので、読むのは起動中に1回だけにする。
/// 読めなかった（拒否された）場合も「キーなし」として覚え、設定画面で保存し直すまで読み直さない。
pub struct KeyCache {
    cached: Mutex<Option<Option<String>>>,
}

impl KeyCache {
    pub fn new() -> Self {
        Self {
            cached: Mutex::new(None),
        }
    }

    /// 覚えているキーを返す。まだ読んでいなければ `load` で1回だけ読む。
    pub fn get(&self, load: impl FnOnce() -> Result<Option<String>, String>) -> Option<String> {
        let mut cached = self.cached.lock().unwrap_or_else(|p| p.into_inner());
        cached.get_or_insert_with(|| load().ok().flatten()).clone()
    }

    /// 保存し直したキーを覚え直す（`None` は「キーなし」）。
    pub fn set(&self, value: Option<String>) {
        *self.cached.lock().unwrap_or_else(|p| p.into_inner()) = Some(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn cache_loads_only_once() {
        let cache = KeyCache::new();
        let loads = Cell::new(0);
        let load = || {
            loads.set(loads.get() + 1);
            Ok(Some("sk-ant-test".to_string()))
        };
        assert_eq!(cache.get(load).as_deref(), Some("sk-ant-test"));
        assert_eq!(cache.get(load).as_deref(), Some("sk-ant-test"));
        assert_eq!(loads.get(), 1);
    }

    #[test]
    fn missing_or_denied_key_is_also_remembered() {
        let cache = KeyCache::new();
        let loads = Cell::new(0);
        let denied = || {
            loads.set(loads.get() + 1);
            Err("拒否されました".to_string())
        };
        assert_eq!(cache.get(denied), None);
        assert_eq!(cache.get(denied), None);
        assert_eq!(loads.get(), 1);
    }

    #[test]
    fn set_replaces_cached_key_without_loading() {
        let cache = KeyCache::new();
        cache.set(Some("sk-ant-new".to_string()));
        let never = || -> Result<Option<String>, String> { panic!("読み直さない") };
        assert_eq!(cache.get(never).as_deref(), Some("sk-ant-new"));
        cache.set(None);
        assert_eq!(cache.get(never), None);
    }

    #[test]
    fn api_key_is_trimmed() {
        assert_eq!(
            normalize_api_key("  sk-ant-test\n").as_deref(),
            Some("sk-ant-test")
        );
    }

    #[test]
    fn blank_api_key_means_delete() {
        assert_eq!(normalize_api_key(""), None);
        assert_eq!(normalize_api_key(" \n\t"), None);
    }
}
