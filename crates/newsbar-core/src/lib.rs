//! newsbar の中心部分。Tauri に依存しない。

use std::time::Duration;

pub mod feed;
pub mod ingest;
pub mod model;
pub mod store;
pub mod url_norm;

/// 新着として取り込む期間（公開からの時間）
pub const NEW_ITEM_WINDOW_HOURS: i64 = 48;
/// 記事を保存しておく日数
pub const RETENTION_DAYS: i64 = 30;
/// 取り込み済みの記事IDを覚えておく日数
pub const SEEN_RETENTION_DAYS: i64 = 90;
/// 1つの情報源から1回に取り込む新着の上限
pub const MAX_NEW_PER_SOURCE: usize = 20;
/// 1回の更新で要約する上限
pub const MAX_SUMMARIES_PER_REFRESH: usize = 30;
/// AI に渡す本文の上限（文字数）
pub const MAX_BODY_CHARS: usize = 12_000;
/// フィードの本文がこれ以上あれば、記事ページを取りに行かない（文字数）
pub const ENOUGH_FEED_TEXT_CHARS: usize = 500;
/// 記事ページから抽出した本文の最低長（文字数）
pub const MIN_EXTRACTED_CHARS: usize = 200;
/// フィードの最大サイズ
pub const MAX_FEED_BYTES: usize = 5 * 1024 * 1024;
/// 記事ページの最大サイズ
pub const MAX_ARTICLE_BYTES: usize = 2 * 1024 * 1024;

const USER_AGENT: &str = "newsbar/0.1 (+https://github.com/tei0110/newsbar)";

/// アプリ共通の HTTP クライアント
pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent(USER_AGENT)
        .build()
        .expect("HTTP クライアントを作れませんでした")
}
