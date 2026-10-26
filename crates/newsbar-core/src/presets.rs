//! 最初から入っている情報源と、おすすめの情報源（設計書 7章、2026-10-26 に動作確認済み）。

use crate::store::{Store, StoreError};

/// あらかじめ用意した情報源
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PresetSource {
    pub name: &'static str,
    pub feed_url: &'static str,
    pub label: &'static str,
}

const fn p(name: &'static str, feed_url: &'static str, label: &'static str) -> PresetSource {
    PresetSource {
        name,
        feed_url,
        label,
    }
}

/// 最初から有効にする情報源（設計書 7.1）
pub const DEFAULT_SOURCES: &[PresetSource] = &[
    p(
        "Simon Willison",
        "https://simonwillison.net/atom/everything/",
        "AI",
    ),
    p("OpenAI News", "https://openai.com/news/rss.xml", "AI"),
    p(
        "Google DeepMind Blog",
        "https://deepmind.google/blog/rss.xml",
        "AI",
    ),
    p(
        "Hugging Face Blog",
        "https://huggingface.co/blog/feed.xml",
        "AI",
    ),
    p("Latent Space", "https://www.latent.space/feed", "AI"),
    p("Interconnects", "https://www.interconnects.ai/feed", "AI"),
    p("Import AI", "https://importai.substack.com/feed", "AI"),
    p(
        "One Useful Thing",
        "https://www.oneusefulthing.org/feed",
        "AI",
    ),
    p(
        "Hacker News（AI・LLM関連、100ポイント以上）",
        "https://hnrss.org/newest?q=LLM+OR+AI&points=100",
        "AI",
    ),
    p(
        "Krebs on Security",
        "https://krebsonsecurity.com/feed/",
        "セキュリティ",
    ),
    p(
        "Schneier on Security",
        "https://www.schneier.com/feed/atom/",
        "セキュリティ",
    ),
    p(
        "BleepingComputer",
        "https://www.bleepingcomputer.com/feed/",
        "セキュリティ",
    ),
    p(
        "piyolog",
        "https://piyolog.hatenadiary.jp/rss",
        "セキュリティ",
    ),
    p(
        "Google Project Zero",
        "https://googleprojectzero.blogspot.com/feeds/posts/summary?max-results=10",
        "セキュリティ",
    ),
    p(
        "Embrace The Red",
        "https://embracethered.com/blog/index.xml",
        "セキュリティ",
    ),
];

/// おすすめ（初期状態では入れない。設定画面から追加できる。設計書 7.2）
pub const RECOMMENDED_SOURCES: &[PresetSource] = &[
    p(
        "Google Research Blog",
        "https://research.google/blog/rss/",
        "AI",
    ),
    p(
        "Google Blog（AI）",
        "https://blog.google/technology/ai/rss/",
        "AI",
    ),
    p(
        "GitHub Blog（AI & ML）",
        "https://github.blog/ai-and-ml/feed/",
        "AI",
    ),
    p(
        "AWS Machine Learning Blog",
        "https://aws.amazon.com/blogs/machine-learning/feed/",
        "AI",
    ),
    p(
        "ITmedia AI+",
        "https://rss.itmedia.co.jp/rss/2.0/aiplus.xml",
        "AI",
    ),
    p(
        "Zenn（LLMトピック）",
        "https://zenn.dev/topics/llm/feed",
        "AI",
    ),
    p(
        "The Hacker News",
        "https://feeds.feedburner.com/TheHackersNews",
        "セキュリティ",
    ),
    p(
        "CISA Advisories",
        "https://www.cisa.gov/cybersecurity-advisories/all.xml",
        "セキュリティ",
    ),
    p(
        "JPCERT/CC",
        "https://www.jpcert.or.jp/rss/jpcert.rdf",
        "セキュリティ",
    ),
    p("JVN iPedia", "https://jvn.jp/rss/jvn.rdf", "セキュリティ"),
    p(
        "IPA セキュリティ情報",
        "https://www.ipa.go.jp/security/alert-rss.rdf",
        "セキュリティ",
    ),
    p(
        "Security NEXT",
        "https://www.security-next.com/feed",
        "セキュリティ",
    ),
    p(
        "Publickey",
        "https://www.publickey1.jp/atom.xml",
        "技術全般",
    ),
    p(
        "Hacker News（トップページ）",
        "https://news.ycombinator.com/rss",
        "技術全般",
    ),
    p(
        "Hacker News（すべて、100ポイント以上）",
        "https://hnrss.org/newest?points=100",
        "技術全般",
    ),
];

const SEEDED_KEY: &str = "presets_seeded";

/// 初回だけ、最初の情報源を登録する（削除したものを次回起動時に復活させない）。
pub fn seed_defaults(store: &Store) -> Result<(), StoreError> {
    if store.get_setting(SEEDED_KEY)?.is_some() {
        return Ok(());
    }
    for preset in DEFAULT_SOURCES {
        match store.add_source(preset.name, preset.feed_url, preset.label, true) {
            Ok(_) | Err(StoreError::Invalid(_)) => {}
            Err(e) => return Err(e),
        }
    }
    store.set_setting(SEEDED_KEY, "1")
}
