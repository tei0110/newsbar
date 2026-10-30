//! 段階2-1: URL の正規化。
//!
//! 同じ記事が別の情報源から別の書き方の URL で届いても、1件にまとめるために使う。
//! `cargo test -p newsbar-core --test url_norm_test` が全部通れば完成。
//!
//! ルール:
//! - http / https 以外、または URL として解釈できないものは `None`
//! - スキームとホストは小文字、既定のポート（http:80 / https:443）とフラグメント（#以降）は除く
//! - クエリから `utm_*`・`fbclid`・`gclid`・`ref` を除く。残りの順序は保つ。空になったら `?` ごと除く
//! - パスがルート（`/`）以外なら、末尾のスラッシュを除く
//!
//! ヒント: `url::Url` は、スキームとホストの小文字化と既定ポートの除去を自動で行う。
//! クエリは `query_pairs()` で読み、`query_pairs_mut()` か `set_query()` で書き戻す。

/// URL を比較用に正規化する。
pub fn normalize_url(raw: &str) -> Option<String> {
    let mut u = url::Url::parse(raw).ok()?;
    if u.scheme() != "http" && u.scheme() != "https" {
        return None;
    }
    u.set_fragment(None);
    let mut pairs = u.query_pairs().into_owned().collect::<Vec<_>>();
    pairs.retain(|(k, _)| !k.starts_with("utm_") && k != "fbclid" && k != "gclid" && k != "ref");
    if pairs.is_empty() {
        u.set_query(None);
    } else {
        u.query_pairs_mut().clear().extend_pairs(pairs);
    }
    let path = u.path().to_string();
    if path != "/"
        && let Some(new_path) = path.strip_suffix('/')
    {
        u.set_path(new_path);
    }

    Some(u.to_string())
}
