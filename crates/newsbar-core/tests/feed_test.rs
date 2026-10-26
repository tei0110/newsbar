use chrono::{TimeZone, Utc};
use newsbar_core::MAX_FEED_BYTES;
use newsbar_core::feed::{FeedError, fetch_feed, parse_feed};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

#[test]
fn rss2_items_are_parsed() {
    let items = parse_feed(&fixture("rss2.xml"), "https://example.com/feed.xml").unwrap();
    assert_eq!(items.len(), 3);
    assert_eq!(items[0].guid.as_deref(), Some("https://example.com/p/1"));
    assert_eq!(items[0].url.as_deref(), Some("https://example.com/p/1"));
    assert_eq!(items[0].title, "First post");
    assert_eq!(
        items[0].published,
        Some(Utc.with_ymd_and_hms(2026, 10, 25, 10, 0, 0).unwrap())
    );
    assert_eq!(items[0].text.as_deref(), Some("Hello world"));
}

#[test]
fn item_without_link_has_no_url() {
    let items = parse_feed(&fixture("rss2.xml"), "https://example.com/feed.xml").unwrap();
    assert_eq!(items[2].url, None);
}

#[test]
fn atom_relative_link_is_resolved() {
    let items = parse_feed(
        &fixture("atom.xml"),
        "https://blog.example.com/atom/everything/",
    )
    .unwrap();
    assert_eq!(
        items[0].url.as_deref(),
        Some("https://blog.example.com/2026/oct/25/post/")
    );
    assert_eq!(
        items[0].guid.as_deref(),
        Some("tag:blog.example.com,2026:1")
    );
}

#[test]
fn rdf_dc_date_is_parsed() {
    let items = parse_feed(&fixture("rdf.xml"), "https://jp.example.org/rss.rdf").unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(
        items[0].published,
        Some(Utc.with_ymd_and_hms(2026, 10, 23, 2, 37, 0).unwrap())
    );
}

#[test]
fn shift_jis_feed_is_decoded() {
    let items = parse_feed(&fixture("rss2_sjis.xml"), "https://jp.example.org/rss").unwrap();
    assert_eq!(items[0].title, "日本語のタイトル");
}

#[test]
fn html_page_is_a_parse_error() {
    let r = parse_feed(
        b"<!DOCTYPE html><html><body>hi</body></html>",
        "https://example.com/",
    );
    assert!(matches!(r, Err(FeedError::Parse(_))), "got {r:?}");
}

#[tokio::test]
async fn fetch_returns_items() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/feed.xml"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/rss+xml")
                .set_body_bytes(fixture("rss2.xml")),
        )
        .mount(&server)
        .await;
    let url = format!("{}/feed.xml", server.uri());
    let items = fetch_feed(&newsbar_core::http_client(), &url)
        .await
        .unwrap();
    assert_eq!(items.len(), 3);
}

#[tokio::test]
async fn fetch_reports_http_status() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    let url = format!("{}/missing.xml", server.uri());
    let r = fetch_feed(&newsbar_core::http_client(), &url).await;
    assert_eq!(r, Err(FeedError::Status(404)));
}

#[tokio::test]
async fn fetch_rejects_oversized_feed() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![b' '; MAX_FEED_BYTES + 1]))
        .mount(&server)
        .await;
    let url = format!("{}/huge.xml", server.uri());
    let r = fetch_feed(&newsbar_core::http_client(), &url).await;
    assert_eq!(r, Err(FeedError::TooLarge));
}
