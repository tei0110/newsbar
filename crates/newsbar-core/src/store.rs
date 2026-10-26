//! 段階2-2: SQLite への保存。
//!
//! `cargo test -p newsbar-core --test store_test` が全部通れば完成。
//!
//! スキーマ（`Store::open` / `open_in_memory` の中で `execute_batch` する）:
//!
//! ```sql
//! CREATE TABLE IF NOT EXISTS sources (id INTEGER PRIMARY KEY, name TEXT NOT NULL,
//!   feed_url TEXT NOT NULL UNIQUE, label TEXT NOT NULL, enabled INTEGER NOT NULL DEFAULT 1,
//!   last_fetched_at TEXT, last_error TEXT);
//! CREATE TABLE IF NOT EXISTS articles (id INTEGER PRIMARY KEY,
//!   source_id INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
//!   url TEXT NOT NULL UNIQUE, original_title TEXT NOT NULL, published_at TEXT NOT NULL,
//!   fetched_at TEXT NOT NULL, body_text TEXT, title TEXT, one_line TEXT, detail TEXT,
//!   summary_status TEXT NOT NULL DEFAULT 'pending', read_at TEXT);
//! CREATE INDEX IF NOT EXISTS articles_published ON articles(published_at DESC);
//! CREATE TABLE IF NOT EXISTS seen_items (source_id INTEGER NOT NULL REFERENCES sources(id)
//!   ON DELETE CASCADE, item_key TEXT NOT NULL, seen_at TEXT NOT NULL,
//!   PRIMARY KEY (source_id, item_key));
//! CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
//! ```
//!
//! 接続を開くたびに `PRAGMA foreign_keys = ON;` と `PRAGMA journal_mode = WAL;` も実行する
//! （foreign_keys を ON にしないと ON DELETE CASCADE が効かない）。
//!
//! ヒント:
//! - 値の受け渡しは `rusqlite::params![...]`、複数行の取得は `prepare` → `query_map`
//! - `DateTime<Utc>` は feature `chrono` のおかげで、そのまま引数にも戻り値にも使える
//! - `Article` は `articles` と `sources` を JOIN して、`source_name`（sources.name）と
//!   `label`（sources.label）を埋める
//! - URL の重複は UNIQUE 制約のエラーで判定できる。
//!   ほかにも `INSERT OR IGNORE` のあとに `changes()` を見る方法がある
//! - `summary_status` は `SummaryStatus::as_str` / `SummaryStatus::parse` で文字列と相互に変換する
//! - `purge` の基準は `crate::RETENTION_DAYS` と `crate::SEEN_RETENTION_DAYS`

use chrono::{DateTime, Utc};

use crate::model::{Article, NewArticle, Source, Summary, SummaryStatus, UnreadCounts};

/// 保存処理のエラー
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error("{0}")]
    Invalid(String),
}

/// SQLite のデータベース
pub struct Store {
    #[allow(dead_code)] // 段階2で使う（実装したらこの行は消す）
    conn: rusqlite::Connection,
}

impl Store {
    /// ファイルのデータベースを開く（なければ作る）。
    pub fn open(_path: &std::path::Path) -> Result<Self, StoreError> {
        todo!("段階2: ユーザーが実装")
    }

    /// メモリ上のデータベースを開く（テスト用）。
    pub fn open_in_memory() -> Result<Self, StoreError> {
        todo!("段階2: ユーザーが実装")
    }

    /// 情報源を追加する。同じ feed_url があれば
    /// `StoreError::Invalid("この情報源はすでに登録されています")`。
    pub fn add_source(
        &self,
        _name: &str,
        _feed_url: &str,
        _label: &str,
        _enabled: bool,
    ) -> Result<Source, StoreError> {
        todo!("段階2: ユーザーが実装")
    }

    /// すべての情報源を id の昇順で返す。
    pub fn list_sources(&self) -> Result<Vec<Source>, StoreError> {
        todo!("段階2: ユーザーが実装")
    }

    pub fn set_source_enabled(&self, _id: i64, _enabled: bool) -> Result<(), StoreError> {
        todo!("段階2: ユーザーが実装")
    }

    /// 情報源を削除する。その記事と取り込み済みIDも消える。
    pub fn delete_source(&self, _id: i64) -> Result<(), StoreError> {
        todo!("段階2: ユーザーが実装")
    }

    /// 取得の結果を記録する。成功なら `error` は `None`（last_error も消す）。
    pub fn record_fetch(
        &self,
        _id: i64,
        _at: DateTime<Utc>,
        _error: Option<&str>,
    ) -> Result<(), StoreError> {
        todo!("段階2: ユーザーが実装")
    }

    pub fn is_seen(&self, _source_id: i64, _key: &str) -> Result<bool, StoreError> {
        todo!("段階2: ユーザーが実装")
    }

    /// 取り込み済みとして記録する。すでにあれば seen_at を更新する。
    pub fn mark_seen(
        &self,
        _source_id: i64,
        _key: &str,
        _at: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        todo!("段階2: ユーザーが実装")
    }

    /// 記事を保存して id を返す。同じ URL がすでにあれば `None`。
    pub fn insert_article(&self, _a: &NewArticle) -> Result<Option<i64>, StoreError> {
        todo!("段階2: ユーザーが実装")
    }

    /// 記事を公開日時の新しい順に返す。`label` を指定するとその分野だけ。
    /// `include_read` が false なら未読だけ。無効な情報源の記事は含めない。
    pub fn list_articles(
        &self,
        _label: Option<&str>,
        _include_read: bool,
    ) -> Result<Vec<Article>, StoreError> {
        todo!("段階2: ユーザーが実装")
    }

    pub fn mark_read(&self, _id: i64, _at: DateTime<Utc>) -> Result<(), StoreError> {
        todo!("段階2: ユーザーが実装")
    }

    /// 未読を既読にして、既読にした件数を返す。`label` を指定するとその分野だけ。
    pub fn mark_all_read(
        &self,
        _label: Option<&str>,
        _at: DateTime<Utc>,
    ) -> Result<usize, StoreError> {
        todo!("段階2: ユーザーが実装")
    }

    /// 未読件数（無効な情報源は除く。by_label はラベル名順）。
    pub fn unread_counts(&self) -> Result<UnreadCounts, StoreError> {
        todo!("段階2: ユーザーが実装")
    }

    /// 要約待ちの記事を新しい順に `limit` 件まで返す（無効な情報源は除く）。
    pub fn pending_summaries(&self, _limit: usize) -> Result<Vec<Article>, StoreError> {
        todo!("段階2: ユーザーが実装")
    }

    /// 要約を保存し、状態を Done にする。
    pub fn save_summary(&self, _id: i64, _s: &Summary) -> Result<(), StoreError> {
        todo!("段階2: ユーザーが実装")
    }

    pub fn set_summary_status(&self, _id: i64, _status: SummaryStatus) -> Result<(), StoreError> {
        todo!("段階2: ユーザーが実装")
    }

    pub fn set_body_text(&self, _id: i64, _text: &str) -> Result<(), StoreError> {
        todo!("段階2: ユーザーが実装")
    }

    /// fetched_at が 30日より前の記事と、seen_at が 90日より前の取り込み済みIDを消す。
    pub fn purge(&self, _now: DateTime<Utc>) -> Result<(), StoreError> {
        todo!("段階2: ユーザーが実装")
    }

    pub fn get_setting(&self, _key: &str) -> Result<Option<String>, StoreError> {
        todo!("段階2: ユーザーが実装")
    }

    /// 設定を保存する（あれば上書き）。
    pub fn set_setting(&self, _key: &str, _value: &str) -> Result<(), StoreError> {
        todo!("段階2: ユーザーが実装")
    }
}
