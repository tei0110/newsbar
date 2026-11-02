use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, TimeZone, Utc};
use newsbar_core::model::{NewArticle, Summary, SummaryStatus};
use newsbar_core::presets::{DEFAULT_SOURCES, RECOMMENDED_SOURCES, seed_defaults};
use newsbar_core::refresh::{RefreshDeps, refresh, validate_feed_url};
use newsbar_core::store::Store;
use newsbar_core::summarize::{SummarizeError, Summarizer, SummaryInput};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// rss2.xml の記事（10/25 10:00 と 10/24 08:00）が 48時間以内に入る時刻
fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 25, 12, 0, 0).unwrap()
}

fn deps() -> RefreshDeps {
    RefreshDeps {
        http: newsbar_core::http_client(),
        summarizer: None,
        language: "ja".into(),
    }
}

/// 決まった結果を返す要約役。タイトルが "refuse" で始まる記事だけ断る
struct FakeSummarizer {
    error: Option<fn() -> SummarizeError>,
    calls: Mutex<Vec<SummaryInput>>,
}

impl FakeSummarizer {
    fn ok() -> Arc<Self> {
        Arc::new(Self {
            error: None,
            calls: Mutex::new(Vec::new()),
        })
    }
    fn failing(error: fn() -> SummarizeError) -> Arc<Self> {
        Arc::new(Self {
            error: Some(error),
            calls: Mutex::new(Vec::new()),
        })
    }
}

#[async_trait::async_trait]
impl Summarizer for FakeSummarizer {
    async fn summarize(&self, input: &SummaryInput) -> Result<Summary, SummarizeError> {
        self.calls.lock().unwrap().push(input.clone());
        if let Some(make_error) = self.error {
            return Err(make_error());
        }
        if input.title.starts_with("refuse") {
            return Err(SummarizeError::Refused);
        }
        let mut summary = Summary {
            title: format!("要約: {}", input.title),
            one_line: "一言".into(),
            detail: "詳しい要約".into(),
        };
        // タイトルが "blank <項目名>" で始まる記事は、その項目を空白だけにして返す
        if input.title.starts_with("blank title") {
            summary.title = " \n".into();
        } else if input.title.starts_with("blank one_line") {
            summary.one_line = String::new();
        } else if input.title.starts_with("blank detail") {
            summary.detail = "  ".into();
        }
        Ok(summary)
    }
}

fn deps_with(summarizer: Arc<FakeSummarizer>) -> RefreshDeps {
    RefreshDeps {
        http: newsbar_core::http_client(),
        summarizer: Some(summarizer),
        language: "ja".into(),
    }
}

/// 取得には失敗する（接続できない）情報源を1つ作り、要約待ちの記事を `bodies` の数だけ入れる。
/// 記事は新しい順に「title 0」「title 1」…となる
fn store_with_pending(titles_and_bodies: &[(&str, &str)]) -> (Mutex<Store>, i64) {
    store_with_pending_at("https://x.example", titles_and_bodies)
}

/// 記事の URL を `{base}/{番号}` にする版（記事ページを偽のサーバーから返すとき用）
fn store_with_pending_at(base: &str, titles_and_bodies: &[(&str, &str)]) -> (Mutex<Store>, i64) {
    let store = Store::open_in_memory().unwrap();
    let src = store
        .add_source("A", "http://127.0.0.1:9/feed", "AI", true)
        .unwrap();
    for (i, (title, body)) in titles_and_bodies.iter().enumerate() {
        store
            .insert_article(&NewArticle {
                source_id: src.id,
                url: format!("{base}/{i}"),
                original_title: title.to_string(),
                published_at: now() - Duration::minutes(i as i64),
                fetched_at: now(),
                body_text: (!body.is_empty()).then(|| body.to_string()),
            })
            .unwrap();
    }
    (Mutex::new(store), src.id)
}

fn statuses(store: &Mutex<Store>) -> Vec<(String, SummaryStatus)> {
    store
        .lock()
        .unwrap()
        .list_articles(None, true)
        .unwrap()
        .into_iter()
        .map(|a| (a.original_title, a.summary_status))
        .collect()
}

#[tokio::test]
async fn refresh_ingests_and_records_errors() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/a.xml"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(fixture("rss2.xml")))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/b.xml"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let store = Store::open_in_memory().unwrap();
    let a = store
        .add_source("A", &format!("{}/a.xml", server.uri()), "AI", true)
        .unwrap();
    let b = store
        .add_source("B", &format!("{}/b.xml", server.uri()), "AI", true)
        .unwrap();
    let store = Mutex::new(store);

    let report = refresh(&store, &deps(), now()).await;

    // link のない3件目を除く2件
    assert_eq!(report.new_articles, 2);
    assert_eq!(report.feed_errors, vec![(b.id, "HTTP 500".to_string())]);
    let store = store.lock().unwrap();
    let sources = store.list_sources().unwrap();
    let got_a = sources.iter().find(|s| s.id == a.id).unwrap();
    let got_b = sources.iter().find(|s| s.id == b.id).unwrap();
    assert_eq!(got_a.last_error, None);
    assert_eq!(got_a.last_fetched_at, Some(now()));
    assert_eq!(got_b.last_error.as_deref(), Some("HTTP 500"));
    assert_eq!(store.list_articles(None, false).unwrap().len(), 2);
}

#[tokio::test]
async fn disabled_sources_are_not_fetched() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(fixture("rss2.xml")))
        .expect(0)
        .mount(&server)
        .await;
    let store = Store::open_in_memory().unwrap();
    store
        .add_source("A", &format!("{}/a.xml", server.uri()), "AI", false)
        .unwrap();
    let report = refresh(&Mutex::new(store), &deps(), now()).await;
    assert_eq!(report.new_articles, 0);
}

#[tokio::test]
async fn offline_refresh_keeps_existing_articles() {
    let store = Store::open_in_memory().unwrap();
    let src = store
        .add_source("A", "http://127.0.0.1:9/feed", "AI", true)
        .unwrap();
    store
        .insert_article(&NewArticle {
            source_id: src.id,
            url: "https://x.example/kept".into(),
            original_title: "kept".into(),
            published_at: now(),
            fetched_at: now(),
            body_text: None,
        })
        .unwrap();
    let store = Mutex::new(store);

    let report = refresh(&store, &deps(), now()).await;

    assert_eq!(report.feed_errors.len(), 1);
    assert_eq!(report.feed_errors[0].0, src.id);
    assert_eq!(
        store
            .lock()
            .unwrap()
            .list_articles(None, false)
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn validate_accepts_feed() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(fixture("atom.xml")))
        .mount(&server)
        .await;
    let n = validate_feed_url(&newsbar_core::http_client(), &format!("{}/f", server.uri()))
        .await
        .unwrap();
    assert_eq!(n, 2);
}

#[tokio::test]
async fn validate_rejects_html_page() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/html")
                .set_body_string("<!DOCTYPE html><html><body>Example</body></html>"),
        )
        .mount(&server)
        .await;
    let msg = validate_feed_url(&newsbar_core::http_client(), &format!("{}/", server.uri()))
        .await
        .unwrap_err();
    assert!(
        msg.contains("フィードとして読み込めませんでした"),
        "got {msg}"
    );
}

#[test]
fn seed_defaults_is_idempotent() {
    let store = Store::open_in_memory().unwrap();
    seed_defaults(&store).unwrap();
    seed_defaults(&store).unwrap();
    let sources = store.list_sources().unwrap();
    assert_eq!(sources.len(), DEFAULT_SOURCES.len());
    assert!(sources.iter().all(|s| s.enabled));
    assert_eq!(sources[0].name, "Simon Willison");
}

#[test]
fn seed_defaults_does_not_restore_deleted_sources() {
    let store = Store::open_in_memory().unwrap();
    seed_defaults(&store).unwrap();
    let first = store.list_sources().unwrap()[0].id;
    store.delete_source(first).unwrap();
    seed_defaults(&store).unwrap();
    assert_eq!(
        store.list_sources().unwrap().len(),
        DEFAULT_SOURCES.len() - 1
    );
}

#[test]
fn presets_match_spec_counts() {
    assert_eq!(DEFAULT_SOURCES.len(), 15);
    assert_eq!(RECOMMENDED_SOURCES.len(), 15);
    let labels = ["AI", "セキュリティ", "技術全般"];
    for p in DEFAULT_SOURCES.iter().chain(RECOMMENDED_SOURCES) {
        assert!(
            labels.contains(&p.label),
            "{} has label {}",
            p.name,
            p.label
        );
        assert!(p.feed_url.starts_with("https://"), "{}", p.feed_url);
    }
}

fn long_body() -> String {
    "本文".repeat(300) // 600文字（ENOUGH_FEED_TEXT_CHARS 以上）
}

#[tokio::test]
async fn summaries_are_saved() {
    let body = long_body();
    let (store, _) = store_with_pending(&[("first", &body), ("second", &body)]);
    let fake = FakeSummarizer::ok();

    let report = refresh(&store, &deps_with(fake.clone()), now()).await;

    assert_eq!(report.summarized, 2);
    assert_eq!(report.summary_error, None);
    let articles = store.lock().unwrap().list_articles(None, true).unwrap();
    assert!(
        articles
            .iter()
            .all(|a| a.summary_status == SummaryStatus::Done)
    );
    assert_eq!(articles[0].title.as_deref(), Some("要約: first"));
    let calls = fake.calls.lock().unwrap();
    assert_eq!(calls[0].source_name, "A");
    assert_eq!(calls[0].language, "ja");
    assert_eq!(calls[0].body_text, body);
}

#[tokio::test]
async fn long_body_is_truncated_before_summarizing() {
    let body = "あ".repeat(newsbar_core::MAX_BODY_CHARS + 50);
    let (store, _) = store_with_pending(&[("first", &body)]);
    let fake = FakeSummarizer::ok();

    refresh(&store, &deps_with(fake.clone()), now()).await;

    let calls = fake.calls.lock().unwrap();
    assert_eq!(
        calls[0].body_text.chars().count(),
        newsbar_core::MAX_BODY_CHARS
    );
}

/// 記事ページを返す偽のサーバー。`pages` は (パス, HTMLか404か)
async fn article_server(pages: &[(&str, Option<Vec<u8>>)]) -> MockServer {
    let server = MockServer::start().await;
    for (p, page) in pages {
        let response = match page {
            Some(html) => {
                ResponseTemplate::new(200).set_body_raw(html.clone(), "text/html; charset=utf-8")
            }
            None => ResponseTemplate::new(404),
        };
        Mock::given(method("GET"))
            .and(path(*p))
            .respond_with(response)
            .mount(&server)
            .await;
    }
    server
}

#[tokio::test]
async fn short_feed_text_fetches_article_page() {
    let server = article_server(&[("/0", Some(fixture("article.html")))]).await;
    let (store, _) = store_with_pending_at(&server.uri(), &[("short", "短い概要")]);
    let fake = FakeSummarizer::ok();

    let report = refresh(&store, &deps_with(fake.clone()), now()).await;

    assert_eq!(report.summarized, 1);
    assert!(
        fake.calls.lock().unwrap()[0]
            .body_text
            .contains("Paragraph one")
    );
    let saved = store.lock().unwrap().list_articles(None, true).unwrap();
    assert_eq!(saved[0].summary_status, SummaryStatus::Done);
    assert!(
        saved[0]
            .body_text
            .as_deref()
            .is_some_and(|b| b.contains("Paragraph one")),
        "取り出した本文を保存しておく"
    );
}

#[tokio::test]
async fn unreachable_article_falls_back_to_feed_text() {
    let server = article_server(&[("/0", None)]).await;
    let feed_text = "フィードの概要".repeat(10);
    let (store, _) = store_with_pending_at(&server.uri(), &[("short", &feed_text)]);
    let fake = FakeSummarizer::ok();

    let report = refresh(&store, &deps_with(fake.clone()), now()).await;

    assert_eq!(report.summarized, 1);
    assert_eq!(fake.calls.lock().unwrap()[0].body_text, feed_text);
    assert_eq!(statuses(&store)[0].1, SummaryStatus::Done);
}

#[tokio::test]
async fn no_text_anywhere_is_skipped() {
    let server = article_server(&[("/0", None)]).await;
    let (store, _) = store_with_pending_at(&server.uri(), &[("empty", "")]);
    let fake = FakeSummarizer::ok();

    let report = refresh(&store, &deps_with(fake.clone()), now()).await;

    assert_eq!(report.summarized, 0);
    assert!(fake.calls.lock().unwrap().is_empty());
    assert_eq!(statuses(&store)[0].1, SummaryStatus::Skipped);
}

#[tokio::test]
async fn skipped_articles_do_not_count_toward_cap() {
    // 新しい10件は本文がどこにもない（飛ばす）、古い30件は本文が十分にある
    let server = article_server(&[]).await; // どのパスも 404
    let body = long_body();
    let titles: Vec<String> = (0..40).map(|i| format!("t{i}")).collect();
    let pairs: Vec<(&str, &str)> = titles
        .iter()
        .enumerate()
        .map(|(i, t)| (t.as_str(), if i < 10 { "" } else { body.as_str() }))
        .collect();
    let (store, _) = store_with_pending_at(&server.uri(), &pairs);

    let report = refresh(&store, &deps_with(FakeSummarizer::ok()), now()).await;

    assert_eq!(report.summarized, newsbar_core::MAX_SUMMARIES_PER_REFRESH);
    let all = statuses(&store);
    assert_eq!(
        all.iter()
            .filter(|(_, s)| *s == SummaryStatus::Skipped)
            .count(),
        10
    );
    assert_eq!(
        all.iter()
            .filter(|(_, s)| *s == SummaryStatus::Pending)
            .count(),
        0
    );
}

#[tokio::test]
async fn unauthorized_stops_and_keeps_pending() {
    let body = long_body();
    let (store, _) = store_with_pending(&[("first", &body), ("second", &body)]);
    let fake = FakeSummarizer::failing(|| SummarizeError::Unauthorized);

    let report = refresh(&store, &deps_with(fake.clone()), now()).await;

    assert_eq!(report.summary_error.as_deref(), Some("APIキーが無効です"));
    assert_eq!(fake.calls.lock().unwrap().len(), 1, "1件目で止める");
    assert!(
        statuses(&store)
            .iter()
            .all(|(_, s)| *s == SummaryStatus::Pending)
    );
}

#[tokio::test]
async fn refused_marks_only_that_article_failed() {
    let body = long_body();
    let (store, _) = store_with_pending(&[("refuse me", &body), ("fine", &body)]);

    let report = refresh(&store, &deps_with(FakeSummarizer::ok()), now()).await;

    assert_eq!(report.summarized, 1);
    assert_eq!(report.summary_error, None);
    assert_eq!(
        statuses(&store),
        vec![
            ("refuse me".to_string(), SummaryStatus::Failed),
            ("fine".to_string(), SummaryStatus::Done),
        ]
    );
}

#[tokio::test]
async fn blank_summary_marks_only_that_article_failed() {
    let body = long_body();
    let (store, _) = store_with_pending(&[
        ("blank title", &body),
        ("blank one_line", &body),
        ("blank detail", &body),
        ("fine", &body),
    ]);

    let report = refresh(&store, &deps_with(FakeSummarizer::ok()), now()).await;

    assert_eq!(report.summarized, 1);
    assert_eq!(report.summary_error, None);
    assert_eq!(
        statuses(&store),
        vec![
            ("blank title".to_string(), SummaryStatus::Failed),
            ("blank one_line".to_string(), SummaryStatus::Failed),
            ("blank detail".to_string(), SummaryStatus::Failed),
            ("fine".to_string(), SummaryStatus::Done),
        ]
    );
}

#[tokio::test]
async fn blank_summaries_count_toward_cap() {
    let body = long_body();
    let titles: Vec<String> = (0..35).map(|i| format!("blank detail {i}")).collect();
    let pairs: Vec<(&str, &str)> = titles.iter().map(|t| (t.as_str(), body.as_str())).collect();
    let (store, _) = store_with_pending(&pairs);
    let fake = FakeSummarizer::ok();

    refresh(&store, &deps_with(fake.clone()), now()).await;

    assert_eq!(
        fake.calls.lock().unwrap().len(),
        newsbar_core::MAX_SUMMARIES_PER_REFRESH
    );
    let failed = statuses(&store)
        .iter()
        .filter(|(_, s)| *s == SummaryStatus::Failed)
        .count();
    assert_eq!(failed, newsbar_core::MAX_SUMMARIES_PER_REFRESH);
}

#[tokio::test]
async fn summarization_is_capped_per_refresh() {
    let body = long_body();
    let titles: Vec<String> = (0..35).map(|i| format!("t{i}")).collect();
    let pairs: Vec<(&str, &str)> = titles.iter().map(|t| (t.as_str(), body.as_str())).collect();
    let (store, _) = store_with_pending(&pairs);

    let report = refresh(&store, &deps_with(FakeSummarizer::ok()), now()).await;

    assert_eq!(report.summarized, newsbar_core::MAX_SUMMARIES_PER_REFRESH);
    let pending = statuses(&store)
        .iter()
        .filter(|(_, s)| *s == SummaryStatus::Pending)
        .count();
    assert_eq!(pending, 35 - newsbar_core::MAX_SUMMARIES_PER_REFRESH);
}

#[tokio::test]
async fn no_summarizer_reports_missing_key() {
    let body = long_body();
    let (store, _) = store_with_pending(&[("first", &body)]);

    let report = refresh(&store, &deps(), now()).await;

    assert!(
        report
            .summary_error
            .as_deref()
            .is_some_and(|m| m.starts_with("APIキーが設定されていません")),
        "got {:?}",
        report.summary_error
    );
    assert_eq!(statuses(&store)[0].1, SummaryStatus::Pending);
}
