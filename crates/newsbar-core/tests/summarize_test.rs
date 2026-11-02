use newsbar_core::model::Summary;
use newsbar_core::summarize::claude::{ClaudeSummarizer, DEFAULT_MODEL};
use newsbar_core::summarize::{SummarizeError, Summarizer, SummaryInput, truncate_chars};
use serde_json::json;
use wiremock::matchers::{body_partial_json, body_string_contains, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn input(language: &str) -> SummaryInput {
    SummaryInput {
        title: "A title".into(),
        source_name: "Example".into(),
        url: "https://example.com/a".into(),
        body_text: "Body text of the article.".into(),
        language: language.into(),
    }
}

async fn summarizer_for(server: &MockServer) -> ClaudeSummarizer {
    ClaudeSummarizer::new(newsbar_core::http_client(), "test-key", DEFAULT_MODEL)
        .unwrap()
        .with_base_url(&server.uri())
}

async fn respond_with(status: u16, body: Option<String>) -> MockServer {
    let server = MockServer::start().await;
    let mut response = ResponseTemplate::new(status);
    if let Some(body) = body {
        response = response.set_body_raw(body, "application/json");
    }
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(response)
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn summarize_ok_sends_expected_request() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header("x-api-key", "test-key"))
        .and(header("anthropic-version", "2026-10-21"))
        .and(body_partial_json(json!({
            "model": "claude-haiku-5-5",
            "output_config": {"effort": "low"}
        })))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(fixture("claude_ok.json"), "application/json"),
        )
        .expect(1)
        .mount(&server)
        .await;

    let s = summarizer_for(&server)
        .await
        .summarize(&input("ja"))
        .await
        .unwrap();

    assert_eq!(
        s,
        Summary {
            title: "T".into(),
            one_line: "O".into(),
            detail: "D".into()
        }
    );
}

#[tokio::test]
async fn article_goes_into_user_message() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(body_string_contains("Title: A title"))
        .and(body_string_contains("Source: Example"))
        .and(body_string_contains("URL: https://example.com/a"))
        .and(body_string_contains("Body text of the article."))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(fixture("claude_ok.json"), "application/json"),
        )
        .expect(1)
        .mount(&server)
        .await;
    summarizer_for(&server)
        .await
        .summarize(&input("ja"))
        .await
        .unwrap();
}

#[tokio::test]
async fn japanese_language_goes_into_system_prompt() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(body_string_contains("Write every field in Japanese."))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(fixture("claude_ok.json"), "application/json"),
        )
        .expect(1)
        .mount(&server)
        .await;
    summarizer_for(&server)
        .await
        .summarize(&input("ja"))
        .await
        .unwrap();
}

#[tokio::test]
async fn english_language_goes_into_system_prompt() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(body_string_contains("Write every field in English."))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(fixture("claude_ok.json"), "application/json"),
        )
        .expect(1)
        .mount(&server)
        .await;
    summarizer_for(&server)
        .await
        .summarize(&input("en"))
        .await
        .unwrap();
}

#[tokio::test]
async fn status_codes_map_to_errors() {
    let cases = [
        (401, SummarizeError::Unauthorized),
        (403, SummarizeError::Unauthorized),
        (429, SummarizeError::RateLimited),
    ];
    for (status, expected) in cases {
        let server = respond_with(status, None).await;
        let r = summarizer_for(&server).await.summarize(&input("ja")).await;
        assert_eq!(r, Err(expected), "status {status}");
    }
    let server = respond_with(500, Some("{\"error\":\"overloaded\"}".into())).await;
    let r = summarizer_for(&server).await.summarize(&input("ja")).await;
    assert!(matches!(r, Err(SummarizeError::Http(500, _))), "got {r:?}");
}

#[tokio::test]
async fn refusal_maps_to_refused() {
    let server = respond_with(200, Some(fixture("claude_refusal.json"))).await;
    let r = summarizer_for(&server).await.summarize(&input("ja")).await;
    assert_eq!(r, Err(SummarizeError::Refused));
}

#[tokio::test]
async fn max_tokens_is_invalid_response() {
    let server = respond_with(200, Some(fixture("claude_max_tokens.json"))).await;
    let r = summarizer_for(&server).await.summarize(&input("ja")).await;
    assert_eq!(r, Err(SummarizeError::InvalidResponse("max_tokens".into())));
}

#[tokio::test]
async fn text_without_tags_is_invalid_response() {
    let server = respond_with(200, Some(fixture("claude_no_tags.json"))).await;
    let r = summarizer_for(&server).await.summarize(&input("ja")).await;
    assert!(
        matches!(r, Err(SummarizeError::InvalidResponse(_))),
        "got {r:?}"
    );
}

/// 送ったリクエストの本文（JSON）を1件だけ取り出す
async fn sent_body(language: &str) -> serde_json::Value {
    let server = respond_with(200, Some(fixture("claude_ok.json"))).await;
    summarizer_for(&server)
        .await
        .summarize(&input(language))
        .await
        .unwrap();
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    serde_json::from_slice(&requests[0].body).unwrap()
}

#[tokio::test]
async fn structured_output_is_not_requested() {
    // 構造化出力（json_schema）は、要約が空文字で返ることがあるため使わない
    let body = sent_body("ja").await;
    assert_eq!(body["output_config"], json!({"effort": "low"}));
}

#[tokio::test]
async fn system_prompt_asks_for_tagged_lines() {
    let body = sent_body("ja").await;
    let system = body["system"].as_str().unwrap();
    assert!(
        system.contains("<title>...</title>\n<one_line>...</one_line>\n<detail>...</detail>"),
        "system: {system}"
    );
}

#[tokio::test]
async fn user_message_ends_with_language_reminder() {
    for (language, reminder) in [
        ("ja", "(Write every field in Japanese.)"),
        ("en", "(Write every field in English.)"),
    ] {
        let body = sent_body(language).await;
        let content = body["messages"][0]["content"].as_str().unwrap();
        assert!(content.ends_with(reminder), "{language}: {content}");
    }
}

/// text ブロックが `text` の、正常終了した応答で要約する
async fn summarize_text(text: &str) -> Result<Summary, SummarizeError> {
    let response = json!({
        "id": "msg_test", "type": "message", "role": "assistant", "model": "claude-haiku-5-5",
        "content": [{"type": "text", "text": text}],
        "stop_reason": "end_turn", "stop_details": null,
        "usage": {"input_tokens": 1200, "output_tokens": 80}
    });
    let server = respond_with(200, Some(response.to_string())).await;
    summarizer_for(&server).await.summarize(&input("ja")).await
}

#[tokio::test]
async fn tag_contents_are_trimmed_but_inner_newlines_kept() {
    let s = summarize_text(
        "<title>\n  T  \n</title>\n<one_line> O </one_line>\n<detail>\nD1\nD2\n</detail>",
    )
    .await
    .unwrap();
    assert_eq!(
        s,
        Summary {
            title: "T".into(),
            one_line: "O".into(),
            detail: "D1\nD2".into()
        }
    );
}

#[tokio::test]
async fn text_around_tags_is_ignored() {
    let s = summarize_text(
        "Here is the summary:\n<title>T</title>\n<one_line>O</one_line>\n<detail>D</detail>\nDone.",
    )
    .await
    .unwrap();
    assert_eq!(
        s,
        Summary {
            title: "T".into(),
            one_line: "O".into(),
            detail: "D".into()
        }
    );
}

#[tokio::test]
async fn missing_or_broken_tags_are_invalid_response() {
    let cases = [
        ("detail がない", "<title>T</title>\n<one_line>O</one_line>"),
        (
            "閉じタグがない",
            "<title>T</title>\n<one_line>O</one_line>\n<detail>D",
        ),
    ];
    for (name, text) in cases {
        let r = summarize_text(text).await;
        assert!(
            matches!(r, Err(SummarizeError::InvalidResponse(_))),
            "{name}: got {r:?}"
        );
    }
}

#[tokio::test]
async fn empty_tag_contents_are_invalid_response() {
    let cases = [
        "<title></title>\n<one_line>O</one_line>\n<detail>D</detail>",
        "<title>T</title>\n<one_line> \n </one_line>\n<detail>D</detail>",
        "<title>T</title>\n<one_line>O</one_line>\n<detail></detail>",
    ];
    for text in cases {
        let r = summarize_text(text).await;
        assert!(
            matches!(r, Err(SummarizeError::InvalidResponse(_))),
            "{text:?}: got {r:?}"
        );
    }
}

#[tokio::test]
async fn unreachable_server_is_network_error() {
    let s = ClaudeSummarizer::new(newsbar_core::http_client(), "test-key", DEFAULT_MODEL)
        .unwrap()
        .with_base_url("http://127.0.0.1:9");
    let r = s.summarize(&input("ja")).await;
    assert!(matches!(r, Err(SummarizeError::Network(_))), "got {r:?}");
}

#[test]
fn blank_api_key_is_missing() {
    let r = ClaudeSummarizer::new(newsbar_core::http_client(), "  \n", DEFAULT_MODEL);
    assert!(matches!(r, Err(SummarizeError::MissingApiKey)));
}

#[tokio::test]
async fn api_key_is_trimmed() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(header("x-api-key", "test-key"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(fixture("claude_ok.json"), "application/json"),
        )
        .expect(1)
        .mount(&server)
        .await;
    ClaudeSummarizer::new(newsbar_core::http_client(), "  test-key\n", DEFAULT_MODEL)
        .unwrap()
        .with_base_url(&server.uri())
        .summarize(&input("ja"))
        .await
        .unwrap();
}

#[test]
fn truncate_counts_chars_not_bytes() {
    assert_eq!(truncate_chars("あいうえお", 3), "あいう");
    assert_eq!(truncate_chars("abc", 10), "abc");
    assert_eq!(truncate_chars("", 3), "");
}
