//! 更新処理: フィードの取得 → 取り込み → 要約。

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::task::JoinSet;

use crate::extract::{ExtractError, fetch_article_text};
use crate::feed::{FeedError, fetch_feed};
use crate::ingest::ingest;
use crate::model::{Article, FeedItem, Summary, SummaryStatus};
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
    /// 有効な情報源が1つ以上あり、そのすべてにつながらなかった（オフラインやスリープ明け直後）
    pub offline: bool,
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

    report.offline = !fetched.is_empty()
        && fetched
            .iter()
            .all(|(_, result)| matches!(result, Err(FeedError::Network(_))));

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
            summarize_pending(store, deps, summarizer.as_ref(), now, &mut report).await;
        }
        None => report.summary_error = Some(MISSING_KEY_MESSAGE.to_string()),
    }
    report
}

/// 1回の更新で見に行く要約待ちの記事の上限。本文が取れずに飛ばす記事は要約の上限に数えないので、
/// そのぶん多めに見る（ただし、記事ページの取得が増えすぎないよう上限は設ける）
const PENDING_SCAN_LIMIT: usize = crate::MAX_SUMMARIES_PER_REFRESH * 3;

/// 要約待ちの記事を、上限まで要約する。
///
/// 上限（`MAX_SUMMARIES_PER_REFRESH`）に数えるのは、実際に AI を呼んだ件数だけ。
/// 記事ごとの失敗（AI が断った、応答が読めない、要約が空）はその記事だけ Failed にして続ける。
/// キーの問題・利用上限・通信の失敗はこの回の要約をやめ、記事は Pending のまま残す。
/// 記事ページの取得が一時的に失敗した記事も、公開から 48 時間までは Pending のまま残して次の更新で試す。
async fn summarize_pending(
    store: &Mutex<Store>,
    deps: &RefreshDeps,
    summarizer: &dyn Summarizer,
    now: DateTime<Utc>,
    report: &mut RefreshReport,
) {
    let pending = match lock(store).pending_summaries(PENDING_SCAN_LIMIT) {
        Ok(list) => list,
        Err(e) => {
            report.summary_error = Some(format!("要約待ちの記事を読み込めませんでした: {e}"));
            return;
        }
    };

    let mut calls = 0;
    for article in pending {
        if calls >= crate::MAX_SUMMARIES_PER_REFRESH {
            break;
        }
        let body = match resolve_body(store, &deps.http, &article, now).await {
            Body::Text(body) => body,
            Body::Retry => continue, // Pending のまま、次の更新でもう一度取りに行く
            Body::Skip => {
                let _ = lock(store).set_summary_status(article.id, SummaryStatus::Skipped);
                continue;
            }
        };
        let input = SummaryInput {
            title: article.original_title.clone(),
            source_name: article.source_name.clone(),
            url: article.url.clone(),
            body_text: truncate_chars(&body, crate::MAX_BODY_CHARS),
            language: deps.language.clone(),
        };
        calls += 1;
        match summarizer.summarize(&input).await {
            Ok(summary) if is_blank(&summary) => {
                let _ = lock(store).set_summary_status(article.id, SummaryStatus::Failed);
            }
            Ok(summary) => {
                if lock(store).save_summary(article.id, &summary).is_ok() {
                    report.summarized += 1;
                }
            }
            Err(SummarizeError::Refused | SummarizeError::InvalidResponse(_)) => {
                let _ = lock(store).set_summary_status(article.id, SummaryStatus::Failed);
            }
            // 本文だと思ったものがアクセス制限ページなどだった。本文が取れなかった記事と同じ扱い
            Err(SummarizeError::NotArticle) => {
                let _ = lock(store).set_summary_status(article.id, SummaryStatus::Skipped);
            }
            Err(e) => {
                report.summary_error = Some(e.to_string());
                break;
            }
        }
    }
}

/// どれかの項目が空（空白だけを含む）の要約。成功として保存しない
fn is_blank(summary: &Summary) -> bool {
    [&summary.title, &summary.one_line, &summary.detail]
        .iter()
        .any(|field| field.trim().is_empty())
}

/// 要約の材料にする本文を決める。
///
/// 1. フィードの本文が十分に長ければ、それを使う
/// 2. 短ければ記事ページから本文を取り出し、取れたら保存して使う
/// 3. それもだめなら `None`（要約しない。設計書 8.2 ③）。短いフィードの本文だけで要約すると、
///    Hacker News の定型文（"Points: 120" など）から中身のない要約ができてしまうため
async fn resolve_body(
    store: &Mutex<Store>,
    http: &reqwest::Client,
    article: &Article,
    now: DateTime<Utc>,
) -> Body {
    let feed_text = article.body_text.clone().unwrap_or_default();
    if feed_text.chars().count() >= crate::ENOUGH_FEED_TEXT_CHARS {
        return Body::Text(feed_text);
    }
    match fetch_article_text(http, &article.url).await {
        Ok(Some(page_text)) => {
            let _ = lock(store).set_body_text(article.id, &page_text);
            Body::Text(page_text)
        }
        Err(e) if is_transient(&e) && within_retry_window(article, now) => Body::Retry,
        _ => Body::Skip,
    }
}

/// 要約の材料を決めた結果
enum Body {
    /// この本文で要約する
    Text(String),
    /// 記事ページの取得に一時的に失敗した。Pending のまま、次の更新でもう一度試す
    Retry,
    /// 本文が取れない（取れる見込みもない）ので要約しない
    Skip,
}

/// もう一度試せば取れるかもしれない失敗か（通信エラー・サーバーの不調・アクセスしすぎ）。
/// 403 や 404 などはサイト側の判断なので、何度試しても変わらない
fn is_transient(e: &ExtractError) -> bool {
    match e {
        ExtractError::Network(_) => true,
        ExtractError::Status(code) => *code == 429 || (500..=599).contains(code),
        ExtractError::TooLarge => false,
    }
}

/// 公開から `NEW_ITEM_WINDOW_HOURS` 以内か。取れないページをいつまでも取りに行き続けないための区切り
fn within_retry_window(article: &Article, now: DateTime<Utc>) -> bool {
    now - article.published_at < chrono::Duration::hours(crate::NEW_ITEM_WINDOW_HOURS)
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
