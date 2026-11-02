//! 更新処理: フィードの取得 → 取り込み → 要約。

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::task::JoinSet;

use crate::feed::{FeedError, fetch_feed};
use crate::ingest::ingest;
use crate::model::{Article, FeedItem, SummaryStatus};
use crate::store::Store;
use crate::summarize::{SummarizeError, Summarizer, SummaryInput, truncate_chars};

/// APIキーが未設定のときに画面に出す文言
pub const MISSING_KEY_MESSAGE: &str = "APIキーが設定されていません（設定 → AI要約）";

/// 更新処理が使う外部のもの
pub struct RefreshDeps {
    pub http: reqwest::Client,
    /// AI 要約の担当。APIキーが未設定なら `None`
    pub summarizer: Option<Arc<dyn Summarizer>>,
    /// 要約の言語（"ja" / "en"）
    pub language: String,
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

    match &deps.summarizer {
        Some(summarizer) => {
            summarize_pending(store, summarizer.as_ref(), &deps.language, &mut report).await;
        }
        None => report.summary_error = Some(MISSING_KEY_MESSAGE.to_string()),
    }
    report
}

/// 要約待ちの記事を、上限まで要約する。
///
/// 記事ごとの失敗（AI が断った、応答が読めない）はその記事だけ Failed にして続ける。
/// キーの問題・利用上限・通信の失敗はこの回の要約をやめ、記事は Pending のまま残す。
async fn summarize_pending(
    store: &Mutex<Store>,
    summarizer: &dyn Summarizer,
    language: &str,
    report: &mut RefreshReport,
) {
    let pending = match lock(store).pending_summaries(crate::MAX_SUMMARIES_PER_REFRESH) {
        Ok(list) => list,
        Err(e) => {
            report.summary_error = Some(format!("要約待ちの記事を読み込めませんでした: {e}"));
            return;
        }
    };

    for article in pending {
        let Some(input) = summary_input(&article, language) else {
            let _ = lock(store).set_summary_status(article.id, SummaryStatus::Skipped);
            continue;
        };
        match summarizer.summarize(&input).await {
            Ok(summary) => {
                if lock(store).save_summary(article.id, &summary).is_ok() {
                    report.summarized += 1;
                }
            }
            Err(SummarizeError::Refused | SummarizeError::InvalidResponse(_)) => {
                let _ = lock(store).set_summary_status(article.id, SummaryStatus::Failed);
            }
            Err(e) => {
                report.summary_error = Some(e.to_string());
                break;
            }
        }
    }
}

/// 要約の材料を作る。フィードの本文が短すぎれば `None`（要約しない）。
fn summary_input(article: &Article, language: &str) -> Option<SummaryInput> {
    let body = article
        .body_text
        .as_deref()
        .filter(|b| b.chars().count() >= crate::ENOUGH_FEED_TEXT_CHARS)?;
    Some(SummaryInput {
        title: article.original_title.clone(),
        source_name: article.source_name.clone(),
        url: article.url.clone(),
        body_text: truncate_chars(body, crate::MAX_BODY_CHARS),
        language: language.to_string(),
    })
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
