//! Highlights as Markdown, for `mxr reading export --markdown`: grouped by
//! the item they came from, oldest first, each with its source and link so
//! a note in another tool can find its way back.

use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct HighlightExport {
    pub quote: String,
    pub note: Option<String>,
    /// The item's headline.
    pub title: String,
    /// The publication.
    pub source: String,
    /// The article or the issue's web link, when there is one.
    pub url: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Every highlight as one Markdown document.
pub fn to_markdown(highlights: &[HighlightExport]) -> String {
    let mut sorted: Vec<&HighlightExport> = highlights.iter().collect();
    sorted.sort_by_key(|h| h.created_at);
    let mut groups: Vec<(&str, &str, Option<&str>, Vec<&HighlightExport>)> = Vec::new();
    for highlight in sorted {
        match groups
            .iter_mut()
            .find(|(title, source, _, _)| *title == highlight.title && *source == highlight.source)
        {
            Some((_, _, _, items)) => items.push(highlight),
            None => groups.push((
                &highlight.title,
                &highlight.source,
                highlight.url.as_deref(),
                vec![highlight],
            )),
        }
    }
    let mut out = String::from("# Reading highlights\n");
    for (title, source, url, items) in groups {
        out.push_str(&format!("\n## {}\n\n", one_line(title)));
        let first = items.first().map(|h| h.created_at.format("%Y-%m-%d").to_string());
        let mut meta = vec![one_line(source)];
        meta.extend(first);
        if let Some(url) = url {
            meta.push(format!("<{url}>"));
        }
        out.push_str(&meta.join(" · "));
        out.push('\n');
        for highlight in items {
            out.push('\n');
            for line in highlight.quote.lines().filter(|l| !l.trim().is_empty()) {
                out.push_str(&format!("> {}\n", line.trim()));
            }
            if let Some(note) = highlight.note.as_deref().filter(|n| !n.trim().is_empty()) {
                out.push_str(&format!("\nNote: {}\n", one_line(note)));
            }
        }
    }
    out
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn at(days: i64) -> DateTime<Utc> {
        DateTime::UNIX_EPOCH + Duration::days(20_300 + days)
    }

    #[test]
    fn highlights_group_by_item_with_source_date_and_link() {
        let highlights = [
            HighlightExport {
                quote: "A delete is the absence of a row,\nand an absence does not travel on its own.".into(),
                note: Some("Use this in the sync talk".into()),
                title: "Shipping a sync engine in 2026".into(),
                source: "Platform Weekly".into(),
                url: Some("https://blog.example.com/sync".into()),
                created_at: at(1),
            },
            HighlightExport {
                quote: "Start with the tablet.".into(),
                note: None,
                title: "One SQLite file per device".into(),
                source: "Sam's Essays".into(),
                url: None,
                created_at: at(0),
            },
            HighlightExport {
                quote: "Merges happen on read, not on write.".into(),
                note: Some("  ".into()),
                title: "Shipping a sync engine in 2026".into(),
                source: "Platform Weekly".into(),
                url: Some("https://blog.example.com/sync".into()),
                created_at: at(2),
            },
        ];
        let markdown = to_markdown(&highlights);
        let expected = "# Reading highlights

## One SQLite file per device

Sam's Essays · 2025-07-31

> Start with the tablet.

## Shipping a sync engine in 2026

Platform Weekly · 2025-08-01 · <https://blog.example.com/sync>

> A delete is the absence of a row,
> and an absence does not travel on its own.

Note: Use this in the sync talk

> Merges happen on read, not on write.
";
        assert_eq!(markdown, expected);
    }

    #[test]
    fn no_highlights_is_just_the_heading() {
        assert_eq!(to_markdown(&[]), "# Reading highlights\n");
    }
}
