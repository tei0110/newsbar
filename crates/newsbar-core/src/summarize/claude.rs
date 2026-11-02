//! 段階3: Claude（Anthropic Messages API）で要約する。
//!
//! リクエスト: `POST {base_url}/v1/messages`
//! ヘッダー: `x-api-key: <APIキー>`、`anthropic-version: 2026-10-21`、`content-type: application/json`
//!
//! 本文（この形どおりに組み立てる。serde_json の `json!` マクロが便利）:
//!
//! ```json
//! {"model": "<model>", "max_tokens": 2048,
//!  "system": "<下の SYSTEM_PROMPT の {LANG} を置き換えたもの>",
//!  "messages": [{"role": "user", "content": "Title: <title>\nSource: <source_name>\nURL: <url>\n\n<body_text>"}],
//!  "output_config": {"effort": "low", "format": {"type": "json_schema", "schema": {
//!    "type": "object", "additionalProperties": false, "required": ["title", "one_line", "detail"],
//!    "properties": {"title": {"type": "string"}, "one_line": {"type": "string"}, "detail": {"type": "string"}}}}}}
//! ```
//!
//! `{LANG}` は language が "ja" なら `Japanese`、"en" なら `English`。
//!
//! 応答の扱い:
//! - HTTP 401・403 → `Unauthorized`、429 → `RateLimited`、そのほかの 200 以外 → `Http(status, 本文)`
//! - `stop_reason` が `"refusal"` → `Refused`、`"max_tokens"` → `InvalidResponse("max_tokens")`
//! - `content` 配列から `"type": "text"` のブロックを探し、その `text` を JSON として `Summary` に読む。
//!   `"type": "thinking"` のブロックは無視する
//! - 送信自体の失敗（つながらない等）→ `Network`

use serde_json::json;

use super::{SummarizeError, Summarizer, SummaryInput};
use crate::model::Summary;

pub const DEFAULT_MODEL: &str = "claude-haiku-5-5";
pub const API_BASE: &str = "https://api.anthropic.com";
pub const API_VERSION: &str = "2026-10-21";

/// システムプロンプト（`{LANG}` を置き換えて使う）
pub const SYSTEM_PROMPT: &str = "\
You summarize news articles for a busy engineer who follows AI and security news.
Write every field in {LANG}.
Use only facts stated in the article text. Never add facts, numbers, names, or opinions that are not in the text.
If the text is short or unclear, summarize only what it actually says.
Fields:
- title: the article title in {LANG} (keep it unchanged if it is already in {LANG})
- one_line: the key point in at most 30 Japanese characters (or about 60 characters in English)
- detail: 2 to 3 sentences with the most important facts";

/// Claude で要約する
pub struct ClaudeSummarizer {
    http: reqwest::Client,
    api_key: String,
    model: String,
    base_url: String,
}

impl ClaudeSummarizer {
    /// APIキーの前後の空白を除き、空なら `MissingApiKey`。base_url は `API_BASE`。
    pub fn new(http: reqwest::Client, api_key: &str, model: &str) -> Result<Self, SummarizeError> {
        let api_key = api_key.trim();
        if api_key.is_empty() {
            return Err(SummarizeError::MissingApiKey);
        }
        Ok(Self {
            http,
            api_key: api_key.to_string(),
            model: model.to_string(),
            base_url: API_BASE.to_string(),
        })
    }

    /// 送り先を変える（テストで偽のサーバーに向けるため）。
    pub fn with_base_url(mut self, base_url: &str) -> Self {
        self.base_url = base_url.to_string();
        self
    }
}

#[async_trait::async_trait]
impl Summarizer for ClaudeSummarizer {
    async fn summarize(&self, input: &SummaryInput) -> Result<Summary, SummarizeError> {
        // ① 本文の JSON を組み立てる
        let lang = if input.language == "en" {
            "English"
        } else {
            "Japanese"
        };
        let body = json!({
            "model": self.model,
            "max_tokens": 2048,
            "system": SYSTEM_PROMPT.replace("{LANG}", lang),
            "messages": [{
                "role": "user",
                "content": format!("Title: {}\nSource: {}\nURL: {}\n\n{}", input.title, input.source_name, input.url, input.body_text)
            }],
            "output_config": {
                "effort": "low",
                "format": {
                    "type": "json_schema",
                    "schema": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["title", "one_line", "detail"],
                        "properties": {
                            "title": {"type": "string"},
                            "one_line": {"type": "string"},
                            "detail": {"type": "string"}
                        }
                    }
                }
            }
        });

        // ② 送る（失敗したら Network）
        let resp = self
            .http
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .json(&body)
            .send()
            .await
            .map_err(|e| SummarizeError::Network(e.to_string()))?;

        // ③ ステータスで分ける（401・403 / 429 / それ以外の 200 以外）
        //    200 以外で Http を返すときは、resp.text().await で本文も入れる
        let status = resp.status().as_u16();
        match status {
            200 => {} // OK
            401 | 403 => return Err(SummarizeError::Unauthorized),
            429 => return Err(SummarizeError::RateLimited),
            other => {
                let text = resp.text().await.unwrap_or_default();
                return Err(SummarizeError::Http(other, text));
            }
        }

        // ④ 中身を読む
        //    resp.json::<ApiResponse>().await で構造体にする（失敗したら InvalidResponse）
        //    stop_reason を見て Refused / InvalidResponse("max_tokens") を返す
        //    content から kind == "text" のブロックを探して、その text を
        //    serde_json::from_str::<Summary>(...) で Summary にする
        // 4-1. JSON を構造体に（読めなければ InvalidResponse）
        let api: ApiResponse = resp
            .json()
            .await
            .map_err(|e| SummarizeError::InvalidResponse(e.to_string()))?;

        // 4-2. stop_reason を見る
        match api.stop_reason.as_deref() {
            Some("refusal") => return Err(SummarizeError::Refused),
            Some("max_tokens") => {
                return Err(SummarizeError::InvalidResponse("max_tokens".to_string()));
            }
            _ => {}
        }

        // 4-3. text ブロックを探して、Summary として読む
        let text = api
            .content
            .iter()
            .find(|b| b.kind == "text")
            .and_then(|b| b.text.as_deref())
            .ok_or_else(|| SummarizeError::InvalidResponse("No text block found".to_string()))?;
        serde_json::from_str::<Summary>(text)
            .map_err(|e| SummarizeError::InvalidResponse(e.to_string()))
    }
}
#[derive(serde::Deserialize)]
struct ApiResponse {
    content: Vec<Block>,
    stop_reason: Option<String>,
}

#[derive(serde::Deserialize)]
struct Block {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>, // thinking ブロックには text がないので Option
}
