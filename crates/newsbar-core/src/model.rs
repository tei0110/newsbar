//! アプリ全体で共有するデータの型。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 情報源（RSS/Atom フィード）
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Source {
    pub id: i64,
    pub name: String,
    pub feed_url: String,
    /// 分野ラベル（「AI」「セキュリティ」など）
    pub label: String,
    pub enabled: bool,
    pub last_fetched_at: Option<DateTime<Utc>>,
    /// 直近の取得が失敗したときの理由
    pub last_error: Option<String>,
}

/// フィードから読み取った1件分の記事
#[derive(Debug, Clone, PartialEq)]
pub struct FeedItem {
    pub guid: Option<String>,
    pub url: Option<String>,
    pub title: String,
    pub published: Option<DateTime<Utc>>,
    /// HTML を除いたプレーンテキストの本文（または概要）
    pub text: Option<String>,
}

/// AI 要約の状態
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SummaryStatus {
    /// まだ要約していない
    Pending,
    /// 要約済み
    Done,
    /// 本文が取れなかったので要約しない
    Skipped,
    /// AI が要約できなかった
    Failed,
}

impl SummaryStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SummaryStatus::Pending => "pending",
            SummaryStatus::Done => "done",
            SummaryStatus::Skipped => "skipped",
            SummaryStatus::Failed => "failed",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(SummaryStatus::Pending),
            "done" => Some(SummaryStatus::Done),
            "skipped" => Some(SummaryStatus::Skipped),
            "failed" => Some(SummaryStatus::Failed),
            _ => None,
        }
    }
}

/// これから保存する記事
#[derive(Debug, Clone, PartialEq)]
pub struct NewArticle {
    pub source_id: i64,
    /// 正規化済みの URL
    pub url: String,
    pub original_title: String,
    pub published_at: DateTime<Utc>,
    pub fetched_at: DateTime<Utc>,
    pub body_text: Option<String>,
}

/// 保存済みの記事（情報源の名前・ラベルと結合済み）
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Article {
    pub id: i64,
    pub source_id: i64,
    pub source_name: String,
    pub label: String,
    pub url: String,
    pub original_title: String,
    pub published_at: DateTime<Utc>,
    pub fetched_at: DateTime<Utc>,
    /// AI が作った（翻訳済みの）タイトル
    pub title: Option<String>,
    pub one_line: Option<String>,
    pub detail: Option<String>,
    pub summary_status: SummaryStatus,
    pub read_at: Option<DateTime<Utc>>,
    #[serde(skip)]
    pub body_text: Option<String>,
}

/// AI が作る要約
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub title: String,
    pub one_line: String,
    pub detail: String,
}

/// 未読件数
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UnreadCounts {
    pub total: u32,
    /// ラベルごとの未読件数（ラベル名順）
    pub by_label: Vec<(String, u32)>,
}
