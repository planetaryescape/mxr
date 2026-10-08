//! Reader text for a fetched web page or an issue's own HTML, through
//! `dom_smoothie`, a Rust port of Readability.js (what Firefox's Reader
//! View uses). It was picked over `readability`, `readabilityrs` and
//! `readability-rust` because it is maintained, MIT licensed, pure Rust and
//! builds on the same html5ever and selectors versions `scraper` already
//! brings, so it adds no second HTML stack.
//!
//! The HTML it returns is the page's own markup, cleaned: clients still
//! sanitize it before showing it, as they do any email.

use crate::extract::{paragraphs_from_html, word_count, Paragraph};

/// Readability on fewer words than this is a paywall, a login wall or a
/// page that only renders in a browser.
const MIN_ARTICLE_WORDS: usize = 120;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Article {
    pub title: String,
    pub byline: Option<String>,
    pub site_name: Option<String>,
    pub excerpt: Option<String>,
    /// Cleaned article HTML, still to be sanitized by the client.
    pub html: String,
    /// The same text as paragraphs, for the terminal and highlights.
    pub paragraphs: Vec<Paragraph>,
    pub words: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ArticleError {
    #[error("no article found on the page")]
    NotFound,
    #[error("only {0} words came through: probably a paywall or a page that needs a browser")]
    TooShort(usize),
}

/// The article in `html`, fetched from `url`.
pub fn extract_article(html: &str, url: Option<&str>) -> Result<Article, ArticleError> {
    let mut readability =
        dom_smoothie::Readability::new(html, url, None).map_err(|_| ArticleError::NotFound)?;
    let article = readability.parse().map_err(|_| ArticleError::NotFound)?;
    let words = word_count(&article.text_content);
    if words < MIN_ARTICLE_WORDS {
        return Err(ArticleError::TooShort(words));
    }
    let html = article.content.to_string();
    Ok(Article {
        title: article.title.trim().to_string(),
        byline: article.byline.filter(|b| !b.trim().is_empty()),
        site_name: article.site_name.filter(|s| !s.trim().is_empty()),
        excerpt: article.excerpt.filter(|e| !e.trim().is_empty()),
        paragraphs: paragraphs_from_html(&html),
        html,
        words: u32::try_from(words).unwrap_or(u32::MAX),
    })
}

/// An issue's own HTML as reader HTML, when Readability keeps at least
/// half of the words the extractor found; table layouts sometimes defeat
/// it, and then the reader shows the extractor's paragraphs instead.
pub fn issue_reader_html(html: &str, issue_words: u32) -> Option<String> {
    let mut readability = dom_smoothie::Readability::new(html, None, None).ok()?;
    let article = readability.parse().ok()?;
    let words = word_count(&article.text_content);
    (words * 2 >= issue_words as usize && words > 0).then(|| article.content.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARTICLE: &str = r#"<!doctype html><html><head><title>Shipping a sync engine in 2026 | Platform Weekly</title>
<meta property="og:site_name" content="Platform Weekly"><meta name="author" content="Sam Okafor"></head>
<body><nav><a href="/">Home</a> <a href="/about">About</a></nav>
<article><h1>Shipping a sync engine in 2026</h1>
<p>We rebuilt replication on top of one SQLite file per device and stopped losing writes. This is the long version of what changed, what broke on the way and the one test that caught it before our users did.</p>
<p>The first decision was the easiest to make and the hardest to live with: every device keeps its own file and its own clock, and nothing is ever merged in place. A change log records every write with the device that made it and the clock reading at the time.</p>
<p>The second decision was about deletes. A delete is the absence of a row, and an absence does not travel on its own, so we keep tombstones for three weeks, which is longer than our slowest device stays offline.</p>
<p>The third decision took the longest. Merges happen on read, not on write, so a device that has been offline for a fortnight pays the cost of catching up the first time it opens the app instead of every device paying for it all the time.</p>
</article><footer>© 2026 Platform Weekly. <a href="/privacy">Privacy</a></footer></body></html>"#;

    #[test]
    fn an_article_comes_out_as_text_without_the_chrome() {
        let article = extract_article(ARTICLE, Some("https://platformweekly.example.com/sync"))
            .expect("article");
        assert!(article.title.starts_with("Shipping a sync engine in 2026"));
        assert!(article.words > 120, "{}", article.words);
        assert!(article
            .paragraphs
            .iter()
            .any(|p| p.text.starts_with("The second decision was about deletes")));
        assert!(!article.html.contains("Privacy"));
        assert!(!article.html.contains("About"));
    }

    #[test]
    fn a_paywall_stub_is_too_short() {
        let stub = "<html><body><article><h1>Members only</h1><p>Subscribe to keep reading this story.</p></article></body></html>";
        assert!(matches!(
            extract_article(stub, None),
            Err(ArticleError::TooShort(_) | ArticleError::NotFound)
        ));
    }
}
