//! 記事の検索（一覧の絞り込み）。
//!
//! 記事は多くても 30 日分（千数百件）なので、全文検索の仕組みは使わず、1 件ずつ文字列で照らし合わせる。
//! 対象は一覧と詳しい要約に出る文章だけ（元のタイトル・日本語のタイトル・一言要約・詳しい要約）。

use crate::model::Article;

/// 記事が検索語にあてはまるか。
///
/// - 空白（全角スペースも含む）で区切った語を、すべて含む記事だけを残す。空の検索語はすべてにあてはまる
/// - 大文字・小文字と、全角・半角の英数字は区別しない
pub fn matches(article: &Article, query: &str) -> bool {
    let haystack = normalize(
        &[
            Some(article.original_title.as_str()),
            article.title.as_deref(),
            article.one_line.as_deref(),
            article.detail.as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n"),
    );
    query
        .split_whitespace()
        .all(|word| haystack.contains(&normalize(word)))
}

/// 全角の英数字・記号（！〜～）を半角にして、小文字にそろえる
fn normalize(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\u{FF01}'..='\u{FF5E}' => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
            _ => c,
        })
        .flat_map(char::to_lowercase)
        .collect()
}
