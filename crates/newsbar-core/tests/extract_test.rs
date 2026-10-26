use newsbar_core::MAX_ARTICLE_BYTES;
use newsbar_core::extract::{ExtractError, extract_main_text, fetch_article_text};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

async fn serve(response: ResponseTemplate) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(response)
        .mount(&server)
        .await;
    server
}

#[test]
fn extracts_article_body_without_chrome() {
    let t = extract_main_text(&fixture("article.html"), "https://example.com/a").unwrap();
    assert!(t.contains("Paragraph one"), "got {t}");
    assert!(t.contains("Paragraph three"), "got {t}");
    assert!(!t.contains("Footer links"), "got {t}");
    assert_eq!(t, t.trim());
}

#[test]
fn too_short_page_is_none() {
    assert_eq!(
        extract_main_text(&fixture("short.html"), "https://example.com/s"),
        None
    );
}

#[tokio::test]
async fn fetch_returns_text_for_html() {
    let server = serve(
        ResponseTemplate::new(200)
            .set_body_raw(fixture("article.html"), "text/html; charset=utf-8"),
    )
    .await;
    let r = fetch_article_text(&newsbar_core::http_client(), &format!("{}/a", server.uri()))
        .await
        .unwrap();
    assert!(r.unwrap().contains("Paragraph two"));
}

#[tokio::test]
async fn fetch_skips_non_html() {
    let server =
        serve(ResponseTemplate::new(200).set_body_raw(b"%PDF-1.7".to_vec(), "application/pdf"))
            .await;
    let r = fetch_article_text(
        &newsbar_core::http_client(),
        &format!("{}/a.pdf", server.uri()),
    )
    .await;
    assert_eq!(r, Ok(None));
}

#[tokio::test]
async fn fetch_reports_status() {
    let server = serve(ResponseTemplate::new(404)).await;
    let r = fetch_article_text(
        &newsbar_core::http_client(),
        &format!("{}/gone", server.uri()),
    )
    .await;
    assert_eq!(r, Err(ExtractError::Status(404)));
}

#[tokio::test]
async fn fetch_rejects_oversized_page() {
    let server = serve(
        ResponseTemplate::new(200).set_body_raw(vec![b'a'; MAX_ARTICLE_BYTES + 1], "text/html"),
    )
    .await;
    let r = fetch_article_text(
        &newsbar_core::http_client(),
        &format!("{}/huge", server.uri()),
    )
    .await;
    assert_eq!(r, Err(ExtractError::TooLarge));
}
