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
    #[allow(dead_code)] // 段階3で使う（実装したらこの行は消す）
    http: reqwest::Client,
    #[allow(dead_code)] // 段階3で使う（実装したらこの行は消す）
    api_key: String,
    #[allow(dead_code)] // 段階3で使う（実装したらこの行は消す）
    model: String,
    #[allow(dead_code)] // 段階3で使う（実装したらこの行は消す）
    base_url: String,
}

impl ClaudeSummarizer {
    /// APIキーの前後の空白を除き、空なら `MissingApiKey`。base_url は `API_BASE`。
    pub fn new(
        _http: reqwest::Client,
        _api_key: &str,
        _model: &str,
    ) -> Result<Self, SummarizeError> {
        todo!("段階3: ユーザーが実装")
    }

    /// 送り先を変える（テストで偽のサーバーに向けるため）。
    pub fn with_base_url(self, _base_url: &str) -> Self {
        todo!("段階3: ユーザーが実装")
    }
}

#[async_trait::async_trait]
impl Summarizer for ClaudeSummarizer {
    async fn summarize(&self, _input: &SummaryInput) -> Result<Summary, SummarizeError> {
        todo!("段階3: ユーザーが実装")
    }
}
