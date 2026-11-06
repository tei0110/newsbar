use chrono::{DateTime, Duration, TimeZone, Utc};
use newsbar_core::model::{NewArticle, Summary, SummaryStatus};
use newsbar_core::store::{Store, StoreError};

fn store() -> Store {
    Store::open_in_memory().unwrap()
}

fn t(day: u32, hour: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, day, hour, 0, 0).unwrap()
}

fn new_article(source_id: i64, url: &str, published: DateTime<Utc>) -> NewArticle {
    NewArticle {
        source_id,
        url: url.to_string(),
        original_title: format!("title of {url}"),
        published_at: published,
        fetched_at: published,
        body_text: Some("body".to_string()),
    }
}

#[test]
fn add_and_list_sources() {
    let s = store();
    let a = s
        .add_source("A", "https://a.example/feed", "AI", true)
        .unwrap();
    let b = s
        .add_source("B", "https://b.example/feed", "セキュリティ", false)
        .unwrap();
    assert_eq!(a.name, "A");
    assert!(a.id < b.id);
    let list = s.list_sources().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0], a);
    assert_eq!(list[1].label, "セキュリティ");
    assert!(!list[1].enabled);
    assert_eq!(list[1].last_fetched_at, None);
    assert_eq!(list[1].last_error, None);
}

#[test]
fn duplicate_feed_url_is_rejected() {
    let s = store();
    s.add_source("A", "https://a.example/feed", "AI", true)
        .unwrap();
    let r = s.add_source("A2", "https://a.example/feed", "AI", true);
    match r {
        Err(StoreError::Invalid(msg)) => assert_eq!(msg, "この情報源はすでに登録されています"),
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn record_fetch_sets_and_clears_error() {
    let s = store();
    let a = s
        .add_source("A", "https://a.example/feed", "AI", true)
        .unwrap();
    s.record_fetch(a.id, t(25, 1), Some("HTTP 500")).unwrap();
    let got = &s.list_sources().unwrap()[0];
    assert_eq!(got.last_fetched_at, Some(t(25, 1)));
    assert_eq!(got.last_error.as_deref(), Some("HTTP 500"));
    s.record_fetch(a.id, t(25, 2), None).unwrap();
    let got = &s.list_sources().unwrap()[0];
    assert_eq!(got.last_fetched_at, Some(t(25, 2)));
    assert_eq!(got.last_error, None);
}

#[test]
fn insert_article_dedupes_by_url() {
    let s = store();
    let a = s
        .add_source("A", "https://a.example/feed", "AI", true)
        .unwrap();
    let first = s
        .insert_article(&new_article(a.id, "https://x.example/1", t(25, 1)))
        .unwrap();
    assert!(first.is_some());
    let second = s
        .insert_article(&new_article(a.id, "https://x.example/1", t(25, 2)))
        .unwrap();
    assert_eq!(second, None);
}

#[test]
fn list_articles_newest_first_and_filters_by_label() {
    let s = store();
    let ai = s
        .add_source("AI src", "https://a.example/feed", "AI", true)
        .unwrap();
    let sec = s
        .add_source("Sec src", "https://s.example/feed", "セキュリティ", true)
        .unwrap();
    s.insert_article(&new_article(ai.id, "https://x.example/old", t(24, 1)))
        .unwrap();
    s.insert_article(&new_article(sec.id, "https://x.example/new", t(25, 1)))
        .unwrap();

    let all = s.list_articles(None, false).unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].url, "https://x.example/new");
    assert_eq!(all[0].label, "セキュリティ");
    assert_eq!(all[0].source_name, "Sec src");
    assert_eq!(all[0].summary_status, SummaryStatus::Pending);
    assert_eq!(all[0].body_text.as_deref(), Some("body"));
    assert_eq!(all[1].url, "https://x.example/old");

    let only_ai = s.list_articles(Some("AI"), false).unwrap();
    assert_eq!(only_ai.len(), 1);
    assert_eq!(only_ai[0].source_name, "AI src");
}

#[test]
fn read_articles_are_hidden_unless_included() {
    let s = store();
    let a = s
        .add_source("A", "https://a.example/feed", "AI", true)
        .unwrap();
    let id = s
        .insert_article(&new_article(a.id, "https://x.example/1", t(25, 1)))
        .unwrap()
        .unwrap();
    s.mark_read(id, t(25, 3)).unwrap();
    assert_eq!(s.list_articles(None, false).unwrap().len(), 0);
    let with_read = s.list_articles(None, true).unwrap();
    assert_eq!(with_read.len(), 1);
    assert_eq!(with_read[0].read_at, Some(t(25, 3)));
}

#[test]
fn disabled_source_articles_are_hidden() {
    let s = store();
    let a = s
        .add_source("A", "https://a.example/feed", "AI", true)
        .unwrap();
    s.insert_article(&new_article(a.id, "https://x.example/1", t(25, 1)))
        .unwrap();
    s.set_source_enabled(a.id, false).unwrap();
    assert_eq!(s.list_articles(None, true).unwrap().len(), 0);
    assert_eq!(s.unread_counts().unwrap().total, 0);
    assert_eq!(s.pending_summaries(10).unwrap().len(), 0);
    s.set_source_enabled(a.id, true).unwrap();
    assert_eq!(s.list_articles(None, true).unwrap().len(), 1);
}

#[test]
fn unread_counts_by_label() {
    let s = store();
    let ai = s
        .add_source("A", "https://a.example/feed", "AI", true)
        .unwrap();
    let sec = s
        .add_source("S", "https://s.example/feed", "セキュリティ", true)
        .unwrap();
    let read_id = s
        .insert_article(&new_article(ai.id, "https://x.example/1", t(25, 1)))
        .unwrap()
        .unwrap();
    s.insert_article(&new_article(ai.id, "https://x.example/2", t(25, 2)))
        .unwrap();
    s.insert_article(&new_article(sec.id, "https://x.example/3", t(25, 3)))
        .unwrap();
    s.mark_read(read_id, t(25, 4)).unwrap();

    let c = s.unread_counts().unwrap();
    assert_eq!(c.total, 2);
    assert_eq!(
        c.by_label,
        vec![("AI".to_string(), 1), ("セキュリティ".to_string(), 1)]
    );
}

#[test]
fn mark_all_read_by_label() {
    let s = store();
    let ai = s
        .add_source("A", "https://a.example/feed", "AI", true)
        .unwrap();
    let sec = s
        .add_source("S", "https://s.example/feed", "セキュリティ", true)
        .unwrap();
    s.insert_article(&new_article(ai.id, "https://x.example/1", t(25, 1)))
        .unwrap();
    s.insert_article(&new_article(ai.id, "https://x.example/2", t(25, 2)))
        .unwrap();
    s.insert_article(&new_article(sec.id, "https://x.example/3", t(25, 3)))
        .unwrap();

    assert_eq!(s.mark_all_read(Some("AI"), t(25, 5)).unwrap(), 2);
    assert_eq!(s.unread_counts().unwrap().total, 1);
    assert_eq!(s.mark_all_read(None, t(25, 6)).unwrap(), 1);
    assert_eq!(s.unread_counts().unwrap().total, 0);
}

#[test]
fn summary_lifecycle() {
    let s = store();
    let a = s
        .add_source("A", "https://a.example/feed", "AI", true)
        .unwrap();
    let older = s
        .insert_article(&new_article(a.id, "https://x.example/1", t(25, 1)))
        .unwrap()
        .unwrap();
    let newer = s
        .insert_article(&new_article(a.id, "https://x.example/2", t(25, 2)))
        .unwrap()
        .unwrap();

    let pending = s.pending_summaries(10).unwrap();
    assert_eq!(
        pending.iter().map(|a| a.id).collect::<Vec<_>>(),
        vec![newer, older]
    );
    assert_eq!(s.pending_summaries(1).unwrap().len(), 1);

    let summary = Summary {
        title: "タイトル".into(),
        one_line: "一言".into(),
        detail: "詳しい要約".into(),
    };
    s.save_summary(newer, &summary).unwrap();
    s.set_summary_status(older, SummaryStatus::Failed).unwrap();
    assert_eq!(s.pending_summaries(10).unwrap().len(), 0);

    let all = s.list_articles(None, false).unwrap();
    assert_eq!(all[0].summary_status, SummaryStatus::Done);
    assert_eq!(all[0].title.as_deref(), Some("タイトル"));
    assert_eq!(all[0].one_line.as_deref(), Some("一言"));
    assert_eq!(all[0].detail.as_deref(), Some("詳しい要約"));
    assert_eq!(all[1].summary_status, SummaryStatus::Failed);
}

#[test]
fn set_body_text_replaces_body() {
    let s = store();
    let a = s
        .add_source("A", "https://a.example/feed", "AI", true)
        .unwrap();
    let id = s
        .insert_article(&new_article(a.id, "https://x.example/1", t(25, 1)))
        .unwrap()
        .unwrap();
    s.set_body_text(id, "extracted body").unwrap();
    let got = &s.list_articles(None, false).unwrap()[0];
    assert_eq!(got.body_text.as_deref(), Some("extracted body"));
}

#[test]
fn seen_keys_are_per_source() {
    let s = store();
    let a = s
        .add_source("A", "https://a.example/feed", "AI", true)
        .unwrap();
    let b = s
        .add_source("B", "https://b.example/feed", "AI", true)
        .unwrap();
    s.mark_seen(a.id, "k", t(25, 1)).unwrap();
    s.mark_seen(a.id, "k", t(25, 2)).unwrap(); // 2回目もエラーにならない
    assert!(s.is_seen(a.id, "k").unwrap());
    assert!(!s.is_seen(b.id, "k").unwrap());
}

#[test]
fn purge_removes_old_articles_and_seen_keys() {
    let s = store();
    let a = s
        .add_source("A", "https://a.example/feed", "AI", true)
        .unwrap();
    let now = t(26, 12);
    let mut old = new_article(a.id, "https://x.example/old", now - Duration::days(31));
    old.fetched_at = now - Duration::days(31);
    let mut recent = new_article(a.id, "https://x.example/recent", now - Duration::days(29));
    recent.fetched_at = now - Duration::days(29);
    s.insert_article(&old).unwrap();
    s.insert_article(&recent).unwrap();
    s.mark_seen(a.id, "old-key", now - Duration::days(91))
        .unwrap();
    s.mark_seen(a.id, "recent-key", now - Duration::days(89))
        .unwrap();

    s.purge(now).unwrap();

    let left = s.list_articles(None, true).unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].url, "https://x.example/recent");
    assert!(!s.is_seen(a.id, "old-key").unwrap());
    assert!(s.is_seen(a.id, "recent-key").unwrap());
}

#[test]
fn delete_source_cascades() {
    let s = store();
    let a = s
        .add_source("A", "https://a.example/feed", "AI", true)
        .unwrap();
    s.insert_article(&new_article(a.id, "https://x.example/1", t(25, 1)))
        .unwrap();
    s.mark_seen(a.id, "k", t(25, 1)).unwrap();
    s.delete_source(a.id).unwrap();
    assert_eq!(s.list_sources().unwrap().len(), 0);
    assert_eq!(s.list_articles(None, true).unwrap().len(), 0);
    assert!(!s.is_seen(a.id, "k").unwrap());
    // 同じURLの記事を、別の情報源で入れ直せる（記事が本当に消えている）
    let b = s
        .add_source("B", "https://b.example/feed", "AI", true)
        .unwrap();
    assert!(
        s.insert_article(&new_article(b.id, "https://x.example/1", t(25, 1)))
            .unwrap()
            .is_some()
    );
}

#[test]
fn settings_round_trip() {
    let s = store();
    assert_eq!(s.get_setting("model").unwrap(), None);
    s.set_setting("model", "claude-haiku-5-5").unwrap();
    assert_eq!(
        s.get_setting("model").unwrap().as_deref(),
        Some("claude-haiku-5-5")
    );
    s.set_setting("model", "other").unwrap();
    assert_eq!(s.get_setting("model").unwrap().as_deref(), Some("other"));
}

#[test]
fn open_creates_file_and_reopens() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("newsbar.db");
    {
        let s = Store::open(&path).unwrap();
        s.add_source("A", "https://a.example/feed", "AI", true)
            .unwrap();
    }
    assert!(path.exists());
    let s = Store::open(&path).unwrap();
    assert_eq!(s.list_sources().unwrap().len(), 1);
}

// ---- 取得の履歴（fetch_log） ----

#[test]
fn fetch_log_keeps_success_and_failure_newest_first() {
    let s = store();
    let a = s
        .add_source("A", "https://a.example/feed", "AI", true)
        .unwrap();
    s.log_fetch(a.id, t(25, 10), None, false).unwrap();
    s.log_fetch(a.id, t(25, 11), Some("HTTP 502"), true)
        .unwrap();

    let log = s.recent_fetches(a.id, 10).unwrap();
    assert_eq!(log.len(), 2);
    assert_eq!(log[0].at, t(25, 11));
    assert_eq!(log[0].error.as_deref(), Some("HTTP 502"));
    assert!(log[0].transient);
    assert_eq!(log[1].at, t(25, 10));
    assert_eq!(log[1].error, None);
}

#[test]
fn recent_fetches_is_per_source_and_limited() {
    let s = store();
    let a = s
        .add_source("A", "https://a.example/feed", "AI", true)
        .unwrap();
    let b = s
        .add_source("B", "https://b.example/feed", "AI", true)
        .unwrap();
    for hour in 1..=3 {
        s.log_fetch(a.id, t(25, hour), None, false).unwrap();
    }
    s.log_fetch(b.id, t(25, 4), Some("HTTP 404"), false)
        .unwrap();

    let log = s.recent_fetches(a.id, 2).unwrap();
    assert_eq!(
        log.iter().map(|e| e.at).collect::<Vec<_>>(),
        vec![t(25, 3), t(25, 2)]
    );
}

#[test]
fn fetch_stats_counts_since_and_last_failure() {
    let s = store();
    let a = s
        .add_source("A", "https://a.example/feed", "AI", true)
        .unwrap();
    s.log_fetch(a.id, t(20, 9), Some("HTTP 500"), true).unwrap(); // 集計期間より前
    s.log_fetch(a.id, t(25, 9), Some("HTTP 502"), true).unwrap();
    s.log_fetch(a.id, t(25, 10), None, false).unwrap();
    s.log_fetch(a.id, t(25, 11), Some("HTTP 503"), true)
        .unwrap();
    s.log_fetch(a.id, t(25, 12), None, false).unwrap();

    let stats = s.fetch_stats(a.id, t(24, 0)).unwrap();
    assert_eq!(stats.total, 4);
    assert_eq!(stats.failures, 2);
    assert_eq!(stats.last_failure_at, Some(t(25, 11)));
    assert_eq!(stats.last_failure_error.as_deref(), Some("HTTP 503"));
}

#[test]
fn fetch_stats_without_log_is_empty() {
    let s = store();
    let a = s
        .add_source("A", "https://a.example/feed", "AI", true)
        .unwrap();
    let stats = s.fetch_stats(a.id, t(24, 0)).unwrap();
    assert_eq!(
        (stats.total, stats.failures, stats.last_failure_at),
        (0, 0, None)
    );
}

#[test]
fn purge_removes_old_fetch_log() {
    let s = store();
    let a = s
        .add_source("A", "https://a.example/feed", "AI", true)
        .unwrap();
    let now = t(31, 0);
    s.log_fetch(a.id, now - Duration::days(31), None, false)
        .unwrap();
    s.log_fetch(a.id, now - Duration::days(1), None, false)
        .unwrap();
    s.purge(now).unwrap();
    assert_eq!(s.recent_fetches(a.id, 10).unwrap().len(), 1);
}

#[test]
fn deleting_source_removes_its_fetch_log() {
    let s = store();
    let a = s
        .add_source("A", "https://a.example/feed", "AI", true)
        .unwrap();
    s.log_fetch(a.id, t(25, 10), None, false).unwrap();
    s.delete_source(a.id).unwrap();
    let b = s
        .add_source("B", "https://b.example/feed", "AI", true)
        .unwrap();
    // id が使い回されても、前の情報源の履歴が混ざらない
    assert!(s.recent_fetches(a.id, 10).unwrap().is_empty());
    assert!(s.recent_fetches(b.id, 10).unwrap().is_empty());
}
