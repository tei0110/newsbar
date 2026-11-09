use chrono::{TimeZone, Utc};
use newsbar_core::model::{Article, SummaryStatus};
use newsbar_core::search::matches;

fn article(
    original_title: &str,
    title: Option<&str>,
    one_line: Option<&str>,
    detail: Option<&str>,
) -> Article {
    let at = Utc.with_ymd_and_hms(2026, 11, 9, 0, 0, 0).unwrap();
    Article {
        id: 1,
        source_id: 1,
        source_name: "BleepingComputer".into(),
        label: "セキュリティ".into(),
        url: "https://example.com/a".into(),
        original_title: original_title.into(),
        published_at: at,
        fetched_at: at,
        title: title.map(Into::into),
        one_line: one_line.map(Into::into),
        detail: detail.map(Into::into),
        summary_status: SummaryStatus::Done,
        read_at: None,
        body_text: Some("本文だけに出てくる言葉: ひみつ".into()),
    }
}

fn sample() -> Article {
    article(
        "Hackers exploit Linux flaw",
        Some("Linux の脆弱性が悪用される"),
        Some("ランサムウェアの被害"),
        Some("CVE-2026-1234 を使った攻撃が確認された。"),
    )
}

#[test]
fn empty_query_matches_everything() {
    assert!(matches(&sample(), ""));
    assert!(matches(&sample(), "   "));
}

#[test]
fn finds_word_in_each_field() {
    assert!(matches(&sample(), "exploit")); // 元の英語のタイトル
    assert!(matches(&sample(), "脆弱性")); // 日本語のタイトル
    assert!(matches(&sample(), "ランサムウェア")); // 一言要約
    assert!(matches(&sample(), "CVE-2026")); // 詳しい要約
    assert!(!matches(&sample(), "Windows"));
}

#[test]
fn ignores_case() {
    assert!(matches(&sample(), "linux"));
    assert!(matches(&sample(), "HACKERS"));
    assert!(matches(&sample(), "cve"));
}

#[test]
fn full_width_letters_match_half_width() {
    // 日本語入力のまま打った全角英数でも見つかる
    assert!(matches(&sample(), "ＣＶＥ"));
    assert!(matches(&sample(), "ｌｉｎｕｘ"));
}

#[test]
fn all_words_must_match() {
    assert!(matches(&sample(), "CVE Linux"));
    assert!(matches(&sample(), "CVE　ランサムウェア")); // 全角スペースでも区切る
    assert!(!matches(&sample(), "CVE Windows"));
}

#[test]
fn body_text_is_not_searched() {
    // 本文は要約の材料。一覧に出ない文章で引っかかると、なぜ出てきたのか分からないため
    assert!(!matches(&sample(), "ひみつ"));
}

#[test]
fn works_without_summary() {
    let pending = article("Patch Tuesday roundup", None, None, None);
    assert!(matches(&pending, "patch"));
    assert!(!matches(&pending, "脆弱性"));
}
