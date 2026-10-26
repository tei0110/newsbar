use std::collections::HashSet;

use chrono::{DateTime, Duration, TimeZone, Utc};
use newsbar_core::ingest::{IngestReport, ingest, item_key, select_new_items};
use newsbar_core::model::FeedItem;
use newsbar_core::store::Store;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 26, 12, 0, 0).unwrap()
}

fn item(guid: Option<&str>, url: Option<&str>, published: Option<DateTime<Utc>>) -> FeedItem {
    FeedItem {
        guid: guid.map(str::to_string),
        url: url.map(str::to_string),
        title: format!("title {guid:?}"),
        published,
        text: Some("text".to_string()),
    }
}

fn at(day: u32, hour: u32) -> Option<DateTime<Utc>> {
    Some(Utc.with_ymd_and_hms(2026, 10, day, hour, 0, 0).unwrap())
}

/// a〜f の6件（コメントのとおり、対象は a・c・d）
fn sample_items() -> Vec<FeedItem> {
    vec![
        item(Some("a"), Some("https://x.example/a"), at(26, 0)), // 対象
        item(Some("b"), Some("https://x.example/b"), at(23, 0)), // 3.5日前 → 対象外
        item(Some("c"), Some("https://x.example/c"), None),      // 日付なし → 対象
        item(Some("d"), Some("https://x.example/d"), at(27, 0)), // 未来 → 対象
        item(Some("e"), None, at(26, 1)),                        // URLなし → 対象外
        item(Some("f"), Some("https://x.example/f"), at(26, 2)), // 取り込み済み → 対象外
    ]
}

#[test]
fn item_key_prefers_guid_then_normalized_url() {
    assert_eq!(
        item_key(&item(Some("g"), Some("https://x.example/1"), None)).as_deref(),
        Some("g")
    );
    assert_eq!(
        item_key(&item(None, Some("HTTPS://E.com/x/"), None)).as_deref(),
        Some("https://e.com/x")
    );
    assert_eq!(item_key(&item(None, None, None)), None);
}

#[test]
fn select_applies_window_seen_and_url_rules() {
    let picked = select_new_items(&sample_items(), now(), |k| k == "f");
    let keys: HashSet<String> = picked.iter().filter_map(item_key).collect();
    assert_eq!(
        keys,
        HashSet::from(["a".to_string(), "c".to_string(), "d".to_string()])
    );
}

#[test]
fn select_caps_per_source_keeping_newest() {
    let items: Vec<FeedItem> = (0..25)
        .map(|i| {
            item(
                Some(&format!("k{i}")),
                Some(&format!("https://x.example/{i}")),
                Some(now() - Duration::hours(i)),
            )
        })
        .collect();
    let picked = select_new_items(&items, now(), |_| false);
    assert_eq!(picked.len(), 20);
    assert_eq!(picked[0].guid.as_deref(), Some("k0"));
    assert_eq!(picked[19].guid.as_deref(), Some("k19"));
}

#[test]
fn ingest_inserts_then_skips_on_second_run() {
    let store = Store::open_in_memory().unwrap();
    let src = store
        .add_source("A", "https://a.example/feed", "AI", true)
        .unwrap();
    store.mark_seen(src.id, "f", now()).unwrap();

    let first = ingest(&store, &src, &sample_items(), now()).unwrap();
    assert_eq!(
        first,
        IngestReport {
            inserted: 3,
            duplicates: 0
        }
    );
    // 古い b も取り込み済みになっている
    assert!(store.is_seen(src.id, "b").unwrap());

    let articles = store.list_articles(None, false).unwrap();
    assert_eq!(articles.len(), 3);
    // 日付なし（c）と未来（d）は now として保存される
    let c = articles
        .iter()
        .find(|a| a.url == "https://x.example/c")
        .unwrap();
    assert_eq!(c.published_at, now());
    assert_eq!(c.fetched_at, now());
    assert_eq!(c.body_text.as_deref(), Some("text"));

    let second = ingest(&store, &src, &sample_items(), now()).unwrap();
    assert_eq!(second, IngestReport::default());
}

#[test]
fn ingest_stores_normalized_url() {
    let store = Store::open_in_memory().unwrap();
    let src = store
        .add_source("A", "https://a.example/feed", "AI", true)
        .unwrap();
    let items = vec![item(
        Some("n"),
        Some("https://X.example/post/?utm_source=rss"),
        at(26, 1),
    )];
    ingest(&store, &src, &items, now()).unwrap();
    assert_eq!(
        store.list_articles(None, false).unwrap()[0].url,
        "https://x.example/post"
    );
}

#[test]
fn same_url_from_two_sources_is_one_article() {
    let store = Store::open_in_memory().unwrap();
    let s1 = store
        .add_source("HN", "https://hn.example/feed", "AI", true)
        .unwrap();
    let s2 = store
        .add_source("Blog", "https://blog.example/feed", "AI", true)
        .unwrap();
    let via_hn = vec![item(
        Some("hn-1"),
        Some("https://x.example/shared"),
        at(26, 1),
    )];
    let via_blog = vec![item(
        Some("blog-1"),
        Some("https://x.example/shared/?utm_medium=feed"),
        at(26, 1),
    )];

    ingest(&store, &s1, &via_hn, now()).unwrap();
    let r = ingest(&store, &s2, &via_blog, now()).unwrap();
    assert_eq!(
        r,
        IngestReport {
            inserted: 0,
            duplicates: 1
        }
    );
    assert_eq!(store.list_articles(None, false).unwrap().len(), 1);
}
