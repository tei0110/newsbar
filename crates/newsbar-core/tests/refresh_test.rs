use std::sync::Mutex;

use chrono::{DateTime, TimeZone, Utc};
use newsbar_core::model::NewArticle;
use newsbar_core::presets::{DEFAULT_SOURCES, RECOMMENDED_SOURCES, seed_defaults};
use newsbar_core::refresh::{RefreshDeps, refresh, validate_feed_url};
use newsbar_core::store::Store;
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
    }
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
