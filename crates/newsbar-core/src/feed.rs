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
//! - Shift_JIS などの UTF-8 以外のフィードを feed-rs がそのまま読めない場合は、
//!   XML 宣言の encoding を見て `encoding_rs` で UTF-8 に変換してから渡す
//! - 日付は feed-rs の標準の読み方では `2026-10-23T11:37+09:00`（秒がない形。JPCERT/CC などが使う）
//!   を読めない。`feed_rs::parser::Builder::new().timestamp_parser(...)` で自前の読み方を登録する。
//!   登録すると標準の読み方は使われなくなるので、RFC 2822（RSS 2.0）と RFC 3339（Atom）も
//!   自分で読む必要がある（chrono の `DateTime::parse_from_rfc2822` などが使える）
//! - 取得は `response.chunk()` などで少しずつ読み、合計が `MAX_FEED_BYTES` を超えたら
//!   `FeedError::TooLarge` で打ち切る（巨大なレスポンスでメモリを使い果たさないため）

use crate::model::FeedItem;

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
pub fn parse_feed(_bytes: &[u8], _feed_url: &str) -> Result<Vec<FeedItem>, FeedError> {
    todo!("段階1: ユーザーが実装")
}

/// `feed_url` からフィードを取得して解析する。
pub async fn fetch_feed(
    _client: &reqwest::Client,
    _feed_url: &str,
) -> Result<Vec<FeedItem>, FeedError> {
    todo!("段階1: ユーザーが実装")
}
