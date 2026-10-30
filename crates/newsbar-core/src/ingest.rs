//! 段階2-3: 新着の判定と取り込み。
//!
//! `cargo test -p newsbar-core --test ingest_test` が全部通れば完成。
//!
//! ルール（設計書 8.1）:
//! - 取り込むのは、公開から 48時間以内（`crate::NEW_ITEM_WINDOW_HOURS`）の、まだ取り込んでいない記事だけ
//! - 公開日時がない記事、未来の日時の記事は「いま公開された」とみなす
//! - URL がない記事、key（guid か正規化 URL）が作れない記事は取り込まない
//! - 1つの情報源から1回に取り込むのは、新しい順に `crate::MAX_NEW_PER_SOURCE` 件まで
//! - 取り込まなかった古い記事も含めて、key があるものはすべて「取り込み済み」にする
//!   （次回以降に新着として出てこないようにするため）

use chrono::{DateTime, Utc};

use crate::model::{FeedItem, NewArticle, Source};
use crate::store::{Store, StoreError};

use crate::url_norm::normalize_url;

/// 記事を見分ける key。guid があれば guid、なければ正規化した URL、どちらもなければ `None`。
pub fn item_key(item: &FeedItem) -> Option<String> {
    if let Some(guid) = &item.guid {
        return Some(guid.clone());
    }
    if let Some(url) = &item.url {
        return normalize_url(url);
    }
    None
}

/// 新着として取り込む記事を選ぶ（公開日時の新しい順）。
///
/// `is_seen` は key を受け取り、取り込み済みなら true を返す関数。
pub fn select_new_items(
    items: &[FeedItem],
    now: DateTime<Utc>,
    is_seen: impl Fn(&str) -> bool,
) -> Vec<FeedItem> {
    let window = chrono::Duration::hours(crate::NEW_ITEM_WINDOW_HOURS);
    let mut new_items: Vec<FeedItem> = items
        .iter()
        .filter(|item| {
            let published = item.published.unwrap_or(now);
            now.signed_duration_since(published) <= window
        })
        .filter(|item| {
            if let Some(key) = item_key(item) {
                !is_seen(&key)
            } else {
                false
            }
        })
        .filter(|item| item.url.is_some())
        .cloned()
        .collect();
    new_items.sort_by_key(|item| std::cmp::Reverse(item.published.unwrap_or(now).min(now)));
    new_items.truncate(crate::MAX_NEW_PER_SOURCE);
    new_items
}

/// 取り込みの結果
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IngestReport {
    /// 新しく保存した件数
    pub inserted: usize,
    /// 同じ URL がすでにあって保存しなかった件数
    pub duplicates: usize,
}

/// 1つの情報源の記事を取り込む。
///
/// `select_new_items` で選んだ記事を、URL を正規化して `insert_article` する
/// （published なし／未来は now、fetched_at は now、body_text には item.text を入れる）。
/// そのあと key がある全アイテムを `mark_seen` する。
pub fn ingest(
    store: &Store,
    source: &Source,
    items: &[FeedItem],
    now: DateTime<Utc>,
) -> Result<IngestReport, StoreError> {
    let new_items = select_new_items(items, now, |key| {
        store.is_seen(source.id, key).unwrap_or(false)
    });
    let mut report = IngestReport::default();
    for item in &new_items {
        let Some(url) = item.url.as_deref().and_then(normalize_url) else {
            continue; // URL がない、または正規化できない記事はスキップ
        };
        let published_at = item.published.unwrap_or(now).min(now);
        let new_article: NewArticle = NewArticle {
            source_id: source.id,
            url,
            original_title: item.title.clone(),
            published_at,
            fetched_at: now,
            body_text: item.text.clone(),
        };
        match store.insert_article(&new_article)? {
            Some(_) => report.inserted += 1, // 保存できた
            None => report.duplicates += 1,  // 同じURLがあった
        }
    }
    for item in items {
        if let Some(key) = item_key(item) {
            store.mark_seen(source.id, &key, now)?;
        }
    }
    Ok(report)
}
