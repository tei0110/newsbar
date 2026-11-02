//! APIキーの保存場所（macOS のキーチェーン）。データベースやログには書かない。

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

pub fn has_api_key() -> bool {
    matches!(load_api_key(), Ok(Some(_)))
}

#[cfg(test)]
mod tests {
    use super::*;

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
