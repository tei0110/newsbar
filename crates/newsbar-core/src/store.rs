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
//! CREATE TABLE IF NOT EXISTS fetch_log (id INTEGER PRIMARY KEY,
//!   source_id INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
//!   at TEXT NOT NULL, error TEXT, transient INTEGER NOT NULL DEFAULT 0);
//! CREATE INDEX IF NOT EXISTS fetch_log_source_at ON fetch_log(source_id, at DESC);
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
use rusqlite::Connection;
use rusqlite::OptionalExtension;

use crate::model::{
    Article, FetchLogEntry, FetchStats, NewArticle, Source, Summary, SummaryStatus, UnreadCounts,
};

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
    conn: rusqlite::Connection,
}

const SCHEMA: &str = "
    PRAGMA foreign_keys = ON;
    PRAGMA journal_mode = WAL;
    CREATE TABLE IF NOT EXISTS sources (id INTEGER PRIMARY KEY, name TEXT NOT NULL,
      feed_url TEXT NOT NULL UNIQUE, label TEXT NOT NULL, enabled INTEGER NOT NULL DEFAULT 1,
      last_fetched_at TEXT, last_error TEXT);
    CREATE TABLE IF NOT EXISTS articles (id INTEGER PRIMARY KEY,
      source_id INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
      url TEXT NOT NULL UNIQUE, original_title TEXT NOT NULL, published_at TEXT NOT NULL,
      fetched_at TEXT NOT NULL, body_text TEXT, title TEXT, one_line TEXT, detail TEXT,
      summary_status TEXT NOT NULL DEFAULT 'pending', read_at TEXT);
    CREATE INDEX IF NOT EXISTS articles_published ON articles(published_at DESC);
    CREATE TABLE IF NOT EXISTS seen_items (source_id INTEGER NOT NULL REFERENCES sources(id)
      ON DELETE CASCADE, item_key TEXT NOT NULL, seen_at TEXT NOT NULL,
      PRIMARY KEY (source_id, item_key));
    CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS fetch_log (id INTEGER PRIMARY KEY,
      source_id INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
      at TEXT NOT NULL, error TEXT, transient INTEGER NOT NULL DEFAULT 0);
    CREATE INDEX IF NOT EXISTS fetch_log_source_at ON fetch_log(source_id, at DESC);
";

const SOURCES_COLUMNS: &str = "id, name, feed_url, label, enabled, last_fetched_at, last_error";

const SELECT_ARTICLES: &str = " SELECT a.id, a.source_id, s.name, s.label, a.url, a.original_title,
         a.published_at, a.fetched_at, a.title, a.one_line, a.detail,
         a.summary_status, a.read_at, a.body_text
         FROM articles a JOIN sources s ON s.id = a.source_id";

impl Store {
    fn init(conn: Connection) -> Result<Self, StoreError> {
        conn.execute_batch(SCHEMA)?;
        Ok(Store { conn })
    }

    /// ファイルのデータベースを開く（なければ作る）。
    pub fn open(path: &std::path::Path) -> Result<Self, StoreError> {
        Self::init(Connection::open(path)?)
    }

    /// メモリ上のデータベースを開く（テスト用）。
    pub fn open_in_memory() -> Result<Self, StoreError> {
        Self::init(Connection::open_in_memory()?)
    }

    fn row_to_source(row: &rusqlite::Row) -> Result<Source, rusqlite::Error> {
        Ok(Source {
            id: row.get(0)?,
            name: row.get(1)?,
            feed_url: row.get(2)?,
            label: row.get(3)?,
            enabled: row.get(4)?,
            last_fetched_at: row.get(5)?,
            last_error: row.get(6)?,
        })
    }

    fn row_to_article(row: &rusqlite::Row) -> Result<Article, rusqlite::Error> {
        let status: String = row.get(11)?;
        let summary_status: SummaryStatus =
            SummaryStatus::parse(&status).unwrap_or(SummaryStatus::Pending);
        Ok(Article {
            id: row.get(0)?,
            source_id: row.get(1)?,
            source_name: row.get(2)?,
            label: row.get(3)?,
            url: row.get(4)?,
            original_title: row.get(5)?,
            published_at: row.get(6)?,
            fetched_at: row.get(7)?,
            title: row.get(8)?,
            one_line: row.get(9)?,
            detail: row.get(10)?,
            summary_status,
            read_at: row.get(12)?,
            body_text: row.get(13)?,
        })
    }

    /// 情報源を追加する。同じ feed_url があれば
    /// `StoreError::Invalid("この情報源はすでに登録されています")`。
    pub fn add_source(
        &self,
        name: &str,
        feed_url: &str,
        label: &str,
        enabled: bool,
    ) -> Result<Source, StoreError> {
        let changed:usize = self.conn.execute(
            "INSERT OR IGNORE INTO sources  (name, feed_url, label, enabled) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![name, feed_url, label, enabled],
        )?;

        if changed == 0 {
            return Err(StoreError::Invalid(
                "この情報源はすでに登録されています".to_string(),
            ));
        }

        let id: i64 = self.conn.last_insert_rowid();
        Ok(self.conn.query_row(
            &format!("SELECT {SOURCES_COLUMNS} FROM sources WHERE id = ?1"),
            rusqlite::params![id],
            Self::row_to_source,
        )?)
    }

    /// すべての情報源を id の昇順で返す。
    pub fn list_sources(&self) -> Result<Vec<Source>, StoreError> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {SOURCES_COLUMNS} FROM sources ORDER BY id ASC"
        ))?;
        let sources = stmt
            .query_map([], Self::row_to_source)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(sources)
    }

    pub fn set_source_enabled(&self, id: i64, enabled: bool) -> Result<(), StoreError> {
        self.conn.execute(
            "UPDATE sources SET enabled = ?1 WHERE id = ?2",
            rusqlite::params![enabled, id],
        )?;
        Ok(())
    }

    /// 情報源を削除する。その記事と取り込み済みIDも消える。
    pub fn delete_source(&self, id: i64) -> Result<(), StoreError> {
        self.conn
            .execute("DELETE FROM sources WHERE id = ?1", rusqlite::params![id])?;
        Ok(())
    }

    /// 取得の結果を記録する。成功なら `error` は `None`（last_error も消す）。
    pub fn record_fetch(
        &self,
        id: i64,
        at: DateTime<Utc>,
        error: Option<&str>,
    ) -> Result<(), StoreError> {
        self.conn.execute(
            "UPDATE sources SET last_fetched_at = ?1, last_error = ?2 WHERE id = ?3",
            rusqlite::params![at, error, id],
        )?;
        Ok(())
    }

    /// 取得の履歴に1行足す。成功なら `error` は `None`。
    pub fn log_fetch(
        &self,
        source_id: i64,
        at: DateTime<Utc>,
        error: Option<&str>,
        transient: bool,
    ) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO fetch_log (source_id, at, error, transient) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![source_id, at, error, transient],
        )?;
        Ok(())
    }

    /// 情報源の取得の履歴を、新しい順に `limit` 件まで返す。
    pub fn recent_fetches(
        &self,
        source_id: i64,
        limit: usize,
    ) -> Result<Vec<FetchLogEntry>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT at, error, transient FROM fetch_log WHERE source_id = ?1
             ORDER BY at DESC, id DESC LIMIT ?2",
        )?;
        let entries = stmt
            .query_map(rusqlite::params![source_id, limit as i64], |row| {
                Ok(FetchLogEntry {
                    at: row.get(0)?,
                    error: row.get(1)?,
                    transient: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(entries)
    }

    /// `since` 以降の取得の回数・失敗の回数と、最後の失敗を返す。
    pub fn fetch_stats(
        &self,
        source_id: i64,
        since: DateTime<Utc>,
    ) -> Result<FetchStats, StoreError> {
        let (total, failures): (i64, i64) = self.conn.query_row(
            "SELECT COUNT(*), COUNT(error) FROM fetch_log WHERE source_id = ?1 AND at >= ?2",
            rusqlite::params![source_id, since],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let last_failure: Option<(DateTime<Utc>, String)> = self
            .conn
            .query_row(
                "SELECT at, error FROM fetch_log
                 WHERE source_id = ?1 AND at >= ?2 AND error IS NOT NULL
                 ORDER BY at DESC, id DESC LIMIT 1",
                rusqlite::params![source_id, since],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let (last_failure_at, last_failure_error) = match last_failure {
            Some((at, error)) => (Some(at), Some(error)),
            None => (None, None),
        };
        Ok(FetchStats {
            total: total as usize,
            failures: failures as usize,
            last_failure_at,
            last_failure_error,
        })
    }

    pub fn is_seen(&self, source_id: i64, key: &str) -> Result<bool, StoreError> {
        self.conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM seen_items WHERE source_id = ?1 AND item_key = ?2)",
                rusqlite::params![source_id, key],
                |row| row.get(0),
            )
            .map_err(StoreError::from)
    }

    /// 取り込み済みとして記録する。すでにあれば seen_at を更新する。
    pub fn mark_seen(
        &self,
        source_id: i64,
        key: &str,
        at: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO seen_items (source_id, item_key, seen_at) VALUES (?1, ?2, ?3) \
             ON CONFLICT(source_id, item_key) DO UPDATE SET seen_at = excluded.seen_at",
            rusqlite::params![source_id, key, at],
        )?;
        Ok(())
    }

    /// 記事を保存して id を返す。同じ URL がすでにあれば `None`。
    pub fn insert_article(&self, a: &NewArticle) -> Result<Option<i64>, StoreError> {
        let changed:usize = self.conn.execute(
            "INSERT OR IGNORE INTO articles (source_id, url, original_title, body_text, published_at, fetched_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                a.source_id,
                a.url,
                a.original_title,
                a.body_text,
                a.published_at,
                a.fetched_at
            ],
        )?;
        Ok(if changed > 0 {
            Some(self.conn.last_insert_rowid())
        } else {
            None
        })
    }

    /// 記事を公開日時の新しい順に返す。`label` を指定するとその分野だけ。
    /// `include_read` が false なら未読だけ。無効な情報源の記事は含めない。
    pub fn list_articles(
        &self,
        label: Option<&str>,
        include_read: bool,
    ) -> Result<Vec<Article>, StoreError> {
        let params = rusqlite::params![label, include_read];

        let mut stmt = self.conn.prepare(&format!(
            "{SELECT_ARTICLES}
            WHERE s.enabled = 1
            AND (?1 IS NULL OR s.label = ?1)
            AND (?2 OR a.read_at IS NULL)
            ORDER BY a.published_at DESC
            "
        ))?;
        let articles = stmt
            .query_map(params, Self::row_to_article)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(articles)
    }

    pub fn mark_read(&self, id: i64, at: DateTime<Utc>) -> Result<(), StoreError> {
        self.conn.execute(
            "UPDATE articles SET read_at = ?2 WHERE id = ?1",
            rusqlite::params![id, at],
        )?;
        Ok(())
    }

    /// 未読を既読にして、既読にした件数を返す。`label` を指定するとその分野だけ。
    pub fn mark_all_read(
        &self,
        label: Option<&str>,
        at: DateTime<Utc>,
    ) -> Result<usize, StoreError> {
        self.conn.execute(
            "UPDATE articles SET read_at = ?2
                    WHERE read_at IS NULL
                    AND source_id IN (SELECT id FROM sources WHERE enabled = 1 AND (?1 IS NULL OR label = ?1))",
            rusqlite::params![label, at],
        ).map_err(StoreError::from)
    }

    /// 未読件数（無効な情報源は除く。by_label はラベル名順）。
    pub fn unread_counts(&self) -> Result<UnreadCounts, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT label, COUNT(*) FROM articles a JOIN sources s ON s.id = a.source_id
             WHERE a.read_at IS NULL AND s.enabled = 1
             GROUP BY label ORDER BY label ASC",
        )?;
        let by_label = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        let total: u32 = by_label.iter().map(|(_, c)| c).sum();
        Ok(UnreadCounts { total, by_label })
    }

    /// 要約待ちの記事を新しい順に `limit` 件まで返す（無効な情報源は除く）。
    pub fn pending_summaries(&self, limit: usize) -> Result<Vec<Article>, StoreError> {
        self.conn
            .prepare(&format!(
                "{SELECT_ARTICLES}
            WHERE s.enabled = 1 AND a.summary_status = 'pending'
            ORDER BY a.published_at DESC
            LIMIT ?1"
            ))?
            .query_map(rusqlite::params![limit as i64], Self::row_to_article)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    /// 要約を保存し、状態を Done にする。
    pub fn save_summary(&self, id: i64, s: &Summary) -> Result<(), StoreError> {
        self.conn.execute(
            "UPDATE articles SET title = ?2, one_line = ?3, detail = ?4, summary_status = 'done' WHERE id = ?1",
            rusqlite::params![id, s.title, s.one_line, s.detail],
        )?;
        Ok(())
    }

    pub fn set_summary_status(&self, id: i64, status: SummaryStatus) -> Result<(), StoreError> {
        self.conn.execute(
            "UPDATE articles SET summary_status = ?2 WHERE id = ?1",
            rusqlite::params![id, status.as_str()],
        )?;
        Ok(())
    }

    pub fn set_body_text(&self, id: i64, text: &str) -> Result<(), StoreError> {
        self.conn.execute(
            "UPDATE articles SET body_text = ?2 WHERE id = ?1",
            rusqlite::params![id, text],
        )?;
        Ok(())
    }

    /// fetched_at が 30日より前の記事、seen_at が 90日より前の取り込み済みID、
    /// 30日より前の取得の履歴を消す。
    pub fn purge(&self, now: DateTime<Utc>) -> Result<(), StoreError> {
        let article_cutoff = now - chrono::Duration::days(crate::RETENTION_DAYS);
        let seen_cutoff = now - chrono::Duration::days(crate::SEEN_RETENTION_DAYS);
        self.conn.execute(
            "DELETE FROM articles WHERE fetched_at < ?1",
            rusqlite::params![article_cutoff],
        )?;
        self.conn.execute(
            "DELETE FROM seen_items WHERE seen_at < ?1",
            rusqlite::params![seen_cutoff],
        )?;
        self.conn.execute(
            "DELETE FROM fetch_log WHERE at < ?1",
            rusqlite::params![article_cutoff],
        )?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>, StoreError> {
        let body: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                rusqlite::params![key],
                |row| row.get(0),
            )
            .optional()?;
        Ok(body)
    }

    /// 設定を保存する（あれば上書き）。
    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![key, value],
        )?;
        Ok(())
    }
}
