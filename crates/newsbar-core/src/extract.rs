//! 段階4: 記事ページから本文を取り出す。
//!
//! フィードに本文がほとんどない記事（Hacker News など）は、記事のページを取りに行って
//! 本文を取り出し、それを要約の材料にする。サイトごとの個別ルールは作らず、汎用的な抽出だけを行う。
//!
//! `cargo test -p newsbar-core --test extract_test` が全部通れば完成。
//!
//! ヒント:
//! - 本文の抽出は dom_smoothie を使う:
//!   `dom_smoothie::Readability::new(html, Some(url), None)?.parse()?` で `Article` が返り、
//!   その `text_content` がプレーンテキストの本文
//! - 抽出した本文の前後の空白を除き、`crate::MIN_EXTRACTED_CHARS` 文字未満なら `None`
//!   （ナビゲーションだけのページなどを本文とみなさないため）
//! - 取得では、レスポンスの Content-Type が text/html で始まらなければ `Ok(None)`（PDF などは対象外）
//! - フィードの取得（段階1）と同じく、少しずつ読みながら `crate::MAX_ARTICLE_BYTES` を超えたら
//!   `ExtractError::TooLarge` で打ち切る

/// 記事ページの取得で起きるエラー
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ExtractError {
    #[error("通信エラー: {0}")]
    Network(String),
    #[error("HTTP {0}")]
    Status(u16),
    #[error("ページが大きすぎます")]
    TooLarge,
}

/// HTML から本文を取り出す。本文が短すぎれば `None`。
pub fn extract_main_text(_html: &str, _url: &str) -> Option<String> {
    todo!("段階4: ユーザーが実装")
}

/// 記事ページを取得して本文を取り出す。HTML でなければ `Ok(None)`。
pub async fn fetch_article_text(
    _client: &reqwest::Client,
    _url: &str,
) -> Result<Option<String>, ExtractError> {
    todo!("段階4: ユーザーが実装")
}
