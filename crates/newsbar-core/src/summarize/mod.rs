//! 段階3: AI 要約（差し替えられる形の trait と、入力の整形）。
//!
//! `cargo test -p newsbar-core --test summarize_test` が全部通れば完成。
//!
//! `Summarizer` は trait にしてあるので、将来ほかの AI サービス（や、運営側のサーバー）を
//! 同じ形で足せる。初版で実装するのは `claude::ClaudeSummarizer` だけ。

pub mod claude;

use crate::model::Summary;

/// 要約の材料
#[derive(Debug, Clone, PartialEq)]
pub struct SummaryInput {
    pub title: String,
    pub source_name: String,
    pub url: String,
    pub body_text: String,
    /// 要約の言語。"ja" か "en"
    pub language: String,
}

/// 要約で起きるエラー
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum SummarizeError {
    #[error("APIキーが設定されていません")]
    MissingApiKey,
    #[error("APIキーが無効です")]
    Unauthorized,
    #[error("APIの利用上限に達しました。しばらく待ってから再試行します")]
    RateLimited,
    #[error("この記事は要約できませんでした")]
    Refused,
    #[error("APIエラー（HTTP {0}）: {1}")]
    Http(u16, String),
    #[error("AIの応答を読み取れませんでした: {0}")]
    InvalidResponse(String),
    #[error("通信エラー: {0}")]
    Network(String),
}

/// 記事を要約するもの
#[async_trait::async_trait]
pub trait Summarizer: Send + Sync {
    async fn summarize(&self, input: &SummaryInput) -> Result<Summary, SummarizeError>;
}

/// 文字数（バイト数ではない）で切り詰める。日本語をバイト単位で切ると文字が壊れるため。
pub fn truncate_chars(_s: &str, _max: usize) -> String {
    todo!("段階3: ユーザーが実装")
}
