//! 更新処理: フィードの取得 → 取り込み →（要約）。

use std::sync::Mutex;

use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::task::JoinSet;

use crate::feed::{FeedError, fetch_feed};
use crate::ingest::ingest;
use crate::model::FeedItem;
use crate::store::Store;

/// 更新処理が使う外部のもの
pub struct RefreshDeps {
    pub http: reqwest::Client,
}

/// 1回の更新の結果
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct RefreshReport {
    pub new_articles: usize,
    pub summarized: usize,
    /// (情報源の id, 理由)
    pub feed_errors: Vec<(i64, String)>,
    pub summary_error: Option<String>,
}

/// 有効な情報源をすべて取得して取り込む。
///
/// 失敗しても panic せず、理由を `RefreshReport` と各情報源の last_error に残す。
/// ストアのロックは await をまたいで持たない。
pub async fn refresh(
    store: &Mutex<Store>,
    deps: &RefreshDeps,
    now: DateTime<Utc>,
) -> RefreshReport {
    let mut report = RefreshReport::default();

    let sources = match lock(store).list_sources() {
        Ok(list) => list.into_iter().filter(|s| s.enabled).collect::<Vec<_>>(),
        Err(e) => {
            report
                .feed_errors
                .push((0, format!("情報源を読み込めませんでした: {e}")));
            return report;
        }
    };

    let mut tasks = JoinSet::new();
    for source in sources {
        let http = deps.http.clone();
        tasks.spawn(async move {
            let result = fetch_feed(&http, &source.feed_url).await;
            (source, result)
        });
    }
    let mut fetched: Vec<(crate::model::Source, Result<Vec<FeedItem>, FeedError>)> = Vec::new();
    while let Some(joined) = tasks.join_next().await {
        if let Ok(pair) = joined {
            fetched.push(pair);
        }
    }

    {
        let store = lock(store);
        for (source, result) in fetched {
            match result {
                Ok(items) => {
                    let _ = store.record_fetch(source.id, now, None);
                    match ingest(&store, &source, &items, now) {
                        Ok(r) => report.new_articles += r.inserted,
                        Err(e) => report.feed_errors.push((source.id, e.to_string())),
                    }
                }
                Err(e) => {
                    let message = e.to_string();
                    let _ = store.record_fetch(source.id, now, Some(&message));
                    report.feed_errors.push((source.id, message));
                }
            }
        }
        let _ = store.purge(now);
    }
    report.feed_errors.sort();
    report
}

/// 情報源として追加する前に、フィードとして読めるか確かめる。読めた記事数か、日本語の理由を返す。
pub async fn validate_feed_url(http: &reqwest::Client, feed_url: &str) -> Result<usize, String> {
    let feed_url = feed_url.trim();
    if url::Url::parse(feed_url).is_err() {
        return Err("URL の形式が正しくありません".to_string());
    }
    fetch_feed(http, feed_url)
        .await
        .map(|items| items.len())
        .map_err(|e| e.to_string())
}

/// ほかのスレッドが panic してもロックを取り続けられるようにする。
fn lock(store: &Mutex<Store>) -> std::sync::MutexGuard<'_, Store> {
    store
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
