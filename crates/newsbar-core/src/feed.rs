//! 段階1: フィードの取得と、記事のデータ（FeedItem）への変換。
//!
//! ここはユーザーが Rust の勉強として書く部分。
//! `cargo test -p newsbar-core --test feed_test` が全部通れば完成。
//!
//! ヒント:
//! - 解析は feed-rs の `feed_rs::parser::parse` を使う（RSS 2.0 / Atom / RSS 1.0 をまとめて扱える）
//! - 本文は entry の content を優先し、なければ summary を使う。
//!   html2text でプレーンテキストにし、前後の空白を取り除く。空になったら `None`
//! - リンクは `url::Url::parse(feed_url)?.join(href)` で絶対 URL にする（相対 URL 対策）
//! - 日付は feed-rs の標準の読み方では `2026-10-23T11:37+09:00`（秒がない形。JPCERT/CC などが使う）
//!   を読めない。`feed_rs::parser::Builder::new().timestamp_parser(...)` で自前の読み方を登録する。
//!   登録すると標準の読み方は使われなくなるので、RFC 2822（RSS 2.0）と RFC 3339（Atom）も
//!   自分で読む必要がある（chrono の `DateTime::parse_from_rfc2822` などが使える）
//! - 取得は `response.chunk()` などで少しずつ読み、合計が `MAX_FEED_BYTES` を超えたら
//!   `FeedError::TooLarge` で打ち切る（巨大なレスポンスでメモリを使い果たさないため）

use crate::model::FeedItem;
use chrono::{DateTime, Utc};

/// フィードの取得・解析で起きるエラー
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum FeedError {
    #[error("通信エラー: {0}")]
    Network(String),
    #[error("HTTP {0}")]
    Status(u16),
    #[error("フィードが大きすぎます")]
    TooLarge,
    #[error("フィードとして読み込めませんでした: {0}")]
    Parse(String),
}

/// フィードの中身（バイト列）を解析して記事の一覧にする。
///
/// `feed_url` は相対 URL を絶対 URL に直すときの基準。
pub fn parse_feed(bytes: &[u8], feed_url: &str) -> Result<Vec<FeedItem>, FeedError> {
    let feed = feed_rs::parser::Builder::new()
        .timestamp_parser(parse_date)
        .build()
        .parse(bytes)
        .map_err(|e| FeedError::Parse(e.to_string()))?;
    let mut items = Vec::new();
    let base = url::Url::parse(feed_url).ok();
    for entry in feed.entries {
        let a = FeedItem {
            guid: entry.id.into(),
            url: entry.links.first().map(|l| {
                base.as_ref()
                    .and_then(|b| b.join(&l.href).ok().map(|u| u.to_string()))
                    .unwrap_or_else(|| l.href.clone())
            }),
            title: entry
                .title
                .as_ref()
                .map(|t| t.content.clone())
                .unwrap_or_default(),
            published: entry.published.or(entry.updated),
            text: entry
                .content
                .as_ref()
                .and_then(|c| c.body.clone())
                .or_else(|| entry.summary.as_ref().map(|s| s.content.clone()))
                .and_then(|s| {
                    html2text::config::plain_no_decorate()
                        .string_from_read(s.as_bytes(), usize::MAX)
                        .ok()
                        .map(|s| s.trim().to_string())
                })
                .filter(|s| !s.is_empty()),
        };
        items.push(a);
    }
    Ok(items)
}

/// `feed_url` からフィードを取得して解析する。
pub async fn fetch_feed(
    client: &reqwest::Client,
    feed_url: &str,
) -> Result<Vec<FeedItem>, FeedError> {
    let mut res = client
        .get(feed_url)
        .send()
        .await
        .map_err(|e| FeedError::Network(e.to_string()))?;
    let status = res.status();
    if !status.is_success() {
        return Err(FeedError::Status(status.as_u16()));
    }
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = res
        .chunk()
        .await
        .map_err(|e| FeedError::Network(e.to_string()))?
    {
        buf.extend_from_slice(&chunk);
        if buf.len() > crate::MAX_FEED_BYTES {
            return Err(FeedError::TooLarge);
        }
    }
    parse_feed(&buf, feed_url)
}

/// `s` を日付として解析する。RFC 2822 / RFC 3339 / `%Y-%m-%dT%H:%M%:z` のいずれかで読める。
fn parse_date(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc2822(s)
        .or_else(|_| DateTime::parse_from_rfc3339(s))
        .or_else(|_| DateTime::parse_from_str(s, "%Y-%m-%dT%H:%M%:z"))
        .ok()
        .map(|d| d.with_timezone(&Utc))
}
