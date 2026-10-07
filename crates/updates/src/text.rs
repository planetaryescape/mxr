//! Text helpers: clipping, the first informative line of a body, and the
//! one link worth opening.

use once_cell::sync::Lazy;
use regex::Regex;

/// At most `max` characters, whitespace collapsed, with an ellipsis when
/// cut.
pub fn clip(value: &str, max: usize) -> String {
    let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max {
        return collapsed;
    }
    let cut: String = collapsed.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", cut.trim_end())
}

/// Lines that say nothing about the update itself: greetings, view in
/// browser, unsubscribe and legal footers, sign-offs.
static BOILERPLATE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?ix)^(
            (hi|hello|hey|dear|greetings)\b[^.!?]{0,40}[,!:]?$
          | .*\b(view|read|open)\s+(this|it|the\s+email|in\s+(your\s+)?(web\s+)?browser|online)\b.*
          | .*\bunsubscribe\b.*
          | .*\b(manage|update)\s+(your\s+)?(email\s+)?(preferences|notification\s+settings|settings)\b.*
          | .*\b(do\s+not|don'?t|please\s+do\s+not)\s+reply\b.*
          | .*\bthis\s+(is\s+an?\s+)?(automated|automatic)\b.*
          | .*\bhaving\s+trouble\s+(viewing|reading)\b.*
          | .*\b(privacy\s+policy|terms\s+of\s+(service|use)|all\s+rights\s+reserved)\b.*
          | (thanks|thank\s+you|cheers|regards|best|sincerely)\b[^.]{0,30}[,.!]?$
          | the\s+[a-z0-9\ .&-]{1,30}\s+team[.!]?$
          | https?://\S+$
          | [^\p{L}\p{N}]*$
        )",
    )
    .expect("valid boilerplate regex")
});

/// The first line of `body` that says something: not a greeting, footer,
/// bare link or a fragment of fewer than three words. Clipped to `max`
/// characters.
pub fn first_informative_line(body: &str, max: usize) -> Option<String> {
    body.lines()
        .map(str::trim)
        .map(|line| line.trim_start_matches(['>', '*', '-', '#', '|']).trim())
        .filter(|line| !line.is_empty())
        .take(40)
        .find(|line| line.split_whitespace().count() >= 3 && !BOILERPLATE.is_match(line))
        .map(|line| clip(line, max))
}

static URL: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"https?://[^\s<>"')\]]+"#).expect("valid url regex"));

/// Words around a link that make it the place to go deeper.
static LINK_CUE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(view|open|see|track|details|review|check|read|status|report|dashboard|order|build|run|incident|activity)\b")
        .expect("valid link cue regex")
});

/// Links that are never the update: unsubscribing, settings, legal and
/// help pages, images and tracking pixels.
static LINK_NOISE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(unsubscribe|preferences|opt-?out|privacy|terms|/help\b|support\.|/settings|\.(png|jpe?g|gif|svg)(\?|$)|/pixel|/open\?|/o/|mailto:)")
        .expect("valid link noise regex")
});

/// The one link a source line opens: the first in the body on a line that
/// names what it opens ("View order", "Open build"), else the first link
/// that isn't noise. Never a pay link: the caller drops links on money
/// lines.
pub fn pick_link(body: &str) -> Option<String> {
    let mut fallback: Option<String> = None;
    let mut previous = "";
    for line in body.lines().take(200) {
        for found in URL.find_iter(line) {
            let url = found.as_str().trim_end_matches(['.', ',', ';', ':']);
            if LINK_NOISE.is_match(url) {
                continue;
            }
            if LINK_CUE.is_match(&line[..found.start()]) || LINK_CUE.is_match(previous) {
                return Some(url.to_string());
            }
            fallback.get_or_insert_with(|| url.to_string());
        }
        if !line.trim().is_empty() {
            previous = line;
        }
    }
    fallback
}

/// The registrable domain of a link's host, for "opens github.com".
pub fn link_domain(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let host = rest
        .split(['/', '?', '#'])
        .next()?
        .rsplit('@')
        .next()?
        .split(':')
        .next()?
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    Some(psl::domain_str(&host).map_or(host.clone(), str::to_string))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_greetings_footers_and_fragments() {
        let body = "View this email in your browser\n\nHi Bhekani,\n\nYour week:\n\
                    You ran 3 times this week, 21.3 km in total.\nUnsubscribe";
        assert_eq!(
            first_informative_line(body, 140).as_deref(),
            Some("You ran 3 times this week, 21.3 km in total.")
        );
        assert_eq!(
            first_informative_line("Hi there,\n\nThanks,\nThe Acme team", 80),
            None
        );
        assert_eq!(clip("a  b   c", 10), "a b c");
        assert_eq!(clip("abcdef", 4), "abc…");
    }

    #[test]
    fn picks_the_link_named_by_its_line_and_skips_noise() {
        let body = "Logo https://cdn.example.com/logo.png\n\
                    Manage preferences https://example.com/preferences\n\
                    https://example.com/about\n\
                    View order\nhttps://shop.example.com/orders/123\n";
        assert_eq!(
            pick_link(body).as_deref(),
            Some("https://shop.example.com/orders/123")
        );
        assert_eq!(
            pick_link("see https://ci.example.com/run/9.").as_deref(),
            Some("https://ci.example.com/run/9")
        );
        assert_eq!(
            pick_link("hello https://example.com/x").as_deref(),
            Some("https://example.com/x")
        );
        assert_eq!(pick_link("unsubscribe https://x.example/unsubscribe"), None);
        assert_eq!(
            link_domain("https://www.github.com:443/acme/api").as_deref(),
            Some("github.com")
        );
    }
}
