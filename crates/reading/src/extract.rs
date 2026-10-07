//! One newsletter email as readable items, by rules and no model (blueprint
//! 22, Reading; research reading.md §4).
//!
//! The HTML is walked once into paragraphs and links. Hidden preheaders,
//! the masthead and the footer are dropped. What is left decides the shape:
//!
//! - `digest`: three or more headline-like links, each with little text of
//!   its own. Every link becomes an item with its blurb.
//! - `teaser`: a short body pointing at one link ("Read more"): the item is
//!   that article.
//! - `notice`: a few lines and nothing to follow.
//! - `single`: an essay; the issue is the article.
//!
//! An in-text link inside a long paragraph is never a digest item: a link
//! counts only when it is the paragraph's headline (in a heading, or most
//! of its block's text).

use crate::headline::clean_headline;
use crate::urls::{clean_url, display_domain, is_click_tracker};
use once_cell::sync::Lazy;
use regex::Regex;
use scraper::{ElementRef, Html, Node};
use serde::{Deserialize, Serialize};

/// Bumped whenever extraction changes, so cached rows are rebuilt.
pub const EXTRACTOR_VERSION: u32 = 1;

/// A standfirst longer than this is cut at a word boundary.
const STANDFIRST_MAX_CHARS: usize = 300;
const BLURB_MAX_CHARS: usize = 240;
/// A paragraph shorter than this is not a standfirst.
const STANDFIRST_MIN_WORDS: usize = 12;
/// Digest: at least this many headline links...
const DIGEST_MIN_LINKS: usize = 3;
/// ...with at most this many words of body per link.
const DIGEST_MAX_WORDS_PER_LINK: usize = 160;
const TEASER_MAX_WORDS: usize = 300;
const NOTICE_MAX_WORDS: usize = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    /// One essay: the issue is the article.
    Single,
    /// Many links with blurbs: each link is an item.
    Digest,
    /// A short body pointing at one article.
    Teaser,
    /// A few lines with nothing to follow.
    Notice,
}

impl Shape {
    pub const fn id(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Digest => "digest",
            Self::Teaser => "teaser",
            Self::Notice => "notice",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "single" => Some(Self::Single),
            "digest" => Some(Self::Digest),
            "teaser" => Some(Self::Teaser),
            "notice" => Some(Self::Notice),
            _ => None,
        }
    }
}

/// A link an issue points to: a digest item or a teaser's article.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkItem {
    pub title: String,
    pub blurb: Option<String>,
    /// Tracking parameters stripped; a redirect that names its target is
    /// unwrapped.
    pub url: String,
    /// `sqlite.org`; for a click tracker, the tracker's domain.
    pub domain: String,
    /// The link goes through a click tracker, so `domain` is not the
    /// article's site.
    pub tracked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParagraphKind {
    Heading,
    Text,
    ListItem,
    Quote,
}

/// One block of the issue's text, for the reader and highlights.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Paragraph {
    pub kind: ParagraphKind,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Extraction {
    pub shape: Shape,
    pub headline: String,
    /// The first paragraph of real text, cut to a few lines.
    pub standfirst: Option<String>,
    /// Words in the issue's body, masthead and footer excluded.
    pub words: u32,
    /// Teaser: the article it points at.
    pub main_link: Option<LinkItem>,
    /// Digest: each link item, in order.
    pub links: Vec<LinkItem>,
    /// The body as paragraphs, masthead and footer removed.
    pub paragraphs: Vec<Paragraph>,
}

/// What extraction reads from one message.
#[derive(Debug, Clone, Copy)]
pub struct IssueInput<'a> {
    pub subject: &'a str,
    /// The publication's name, when known (the sender's display name).
    pub source: Option<&'a str>,
    pub html: Option<&'a str>,
    pub text: Option<&'a str>,
    /// Used when no body is stored.
    pub snippet: &'a str,
}

fn regex(pattern: &str) -> Regex {
    Regex::new(pattern).expect("extraction regex literals compile")
}

static HIDDEN_STYLE: Lazy<Regex> = Lazy::new(|| {
    regex(
        r"(?i)display\s*:\s*none|visibility\s*:\s*hidden|max-height\s*:\s*0(?:px)?\s*(?:;|$|!)|font-size\s*:\s*[01](?:px)?\s*(?:;|$|!)|opacity\s*:\s*0(?:\.0+)?\s*(?:;|$|!)|mso-hide\s*:\s*all",
    )
});
/// Masthead lines: never a standfirst.
static MASTHEAD: Lazy<Regex> = Lazy::new(|| {
    regex(
        r"(?i)^(?:view (?:this )?(?:email|message|newsletter|post)?\s*(?:in|on) (?:your |a )?(?:browser|web)|view online|read (?:it )?(?:online|in (?:the )?app|on the web)|open in app|forwarded this email\??|was this (?:email )?forwarded to you\??|(?:sponsored|presented|brought to you) by\b|in partnership with\b|together with\b|listen (?:now|to this)|watch now|share this (?:post|email)|subscribe\b|upgrade to paid)",
    )
});
/// Footer lines: everything from the first one on is dropped.
static FOOTER: Lazy<Regex> = Lazy::new(|| {
    regex(
        r"(?i)\bunsubscribe\b|you(?: are| were|'re|’re)? receiving this|you received this|update (?:your )?preferences|manage (?:your )?(?:email )?(?:preferences|subscription)|to stop receiving|no longer (?:wish|want) to receive|©\s*\d{4}|all rights reserved|why did i get this",
    )
});
static UNSUBSCRIBE_WORD: Lazy<Regex> = Lazy::new(|| regex(r"(?i)\bunsubscribe\b"));
/// A paragraph that is only a date: "Oct 6, 2026", "6 October 2026".
static DATE_ONLY: Lazy<Regex> = Lazy::new(|| {
    regex(r"(?i)^(?:(?:mon|tue|wed|thu|fri|sat|sun)[a-z]*,?\s+)?(?:[a-z]{3,9}\.?\s+\d{1,2},?\s+\d{4}|\d{1,2}\s+[a-z]{3,9}\.?,?\s+\d{4}|\d{4}-\d{2}-\d{2})$")
});
static GREETING: Lazy<Regex> = Lazy::new(|| {
    regex(r"(?i)^(?:hi|hey|hello|dear|good (?:morning|afternoon|evening)|morning|greetings|welcome back)\b(?:[^.!?]{0,40}[.!?]|[^,.!?]{0,30}[,:])\s*")
});
static READ_MORE: Lazy<Regex> = Lazy::new(|| {
    regex(
        r"(?i)^(?:read more|continue reading|keep reading|read the (?:full|rest|whole)\b.*|read (?:the )?(?:full )?(?:post|article|story|essay|piece)|full (?:story|article|post)|read now|read it here|read on)\W*$",
    )
});
/// Links that are about the email, not in it.
static BOILERPLATE_LINK: Lazy<Regex> = Lazy::new(|| {
    regex(
        r"(?i)^(?:unsubscribe|manage|preferences|update (?:your )?preferences|view (?:in|on)|view online|read online|read in app|open in app|share|like|comment|restack|subscribe|upgrade|sign up|log ?in|forward|refer\b|privacy|terms|help|contact us|follow|twitter|x|facebook|linkedin|instagram|youtube|tiktok|threads|bluesky|mastodon|download (?:on|the)|get it on|app store|google play|advertise|sponsor|website|home|here|link|click here|this)\b",
    )
});
static BOILERPLATE_HREF: Lazy<Regex> = Lazy::new(|| {
    regex(
        r"(?i)unsubscribe|/subscribe|manage[-_]?preferences|/preferences|/account|/settings|/app-link/|/share\b|action=share|twitter\.com|x\.com/|facebook\.com|linkedin\.com/(?:company|in|share)|instagram\.com|youtube\.com/(?:@|c/|channel|user)|apps\.apple\.com|play\.google\.com|/comments?\b|/sign-?in|/login|/refer",
    )
});
/// "1. ", "2) ", "- " before a list item's title.
static LIST_MARKER: Lazy<Regex> = Lazy::new(|| regex(r"^\s*(?:\d{1,3}[.)]|[-*•·])\s+"));
/// The verbs buttons start with.
static CTA_VERB: Lazy<Regex> = Lazy::new(|| {
    regex(
        r"(?i)^(?:view|watch|read|get|save|shop|buy|book|claim|learn|discover|explore|try|start|join|register|download|see|check|order|reserve|apply|listen|unlock|grab|redeem|continue|go to|visit|open|reply|rsvp|vote|donate)\b",
    )
});
static URL_IN_TEXT: Lazy<Regex> = Lazy::new(|| regex(r#"https?://[^\s<>()"']+[^\s<>()"'.,;:!?\]]"#));

fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn word_count(text: &str) -> usize {
    text.split_whitespace()
        .filter(|w| w.chars().any(char::is_alphanumeric))
        .count()
}

/// Cut at a word boundary, with an ellipsis when cut.
pub fn clip(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out = String::new();
    for word in text.split_whitespace() {
        if out.chars().count() + word.chars().count() + 1 > max_chars.saturating_sub(1) {
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    let trimmed = out.trim_end_matches([',', ';', ':', '-', '–', '—', '.']);
    format!("{trimmed}…")
}

/// Extract the readable items of one issue.
pub fn extract(input: &IssueInput<'_>) -> Extraction {
    let headline = clean_headline(input.subject, input.source);
    let walked = match input.html.filter(|html| !html.trim().is_empty()) {
        Some(html) => walk_html(html),
        None => match input.text.filter(|text| !text.trim().is_empty()) {
            Some(text) => walk_text(text),
            None => Walked {
                paragraphs: vec![Paragraph {
                    kind: ParagraphKind::Text,
                    text: collapse(input.snippet),
                }],
                anchors: Vec::new(),
            },
        },
    };
    classify(walked, headline, input.subject)
}

/// A link as the walk found it.
#[derive(Debug, Clone)]
struct Anchor {
    text: String,
    href: String,
    /// Index into the walk's paragraphs.
    paragraph: usize,
    in_heading: bool,
    in_strong: bool,
}

struct Walked {
    paragraphs: Vec<Paragraph>,
    anchors: Vec<Anchor>,
}

const BLOCK_TAGS: &[&str] = &[
    "p", "div", "td", "th", "tr", "table", "tbody", "thead", "tfoot", "li", "ul", "ol", "h1", "h2",
    "h3", "h4", "h5", "h6", "blockquote", "section", "article", "header", "footer", "main", "pre",
    "dd", "dt", "dl", "center", "figure", "figcaption", "hr", "aside", "nav", "address",
];
const SKIP_TAGS: &[&str] = &[
    "script", "style", "head", "title", "noscript", "template", "svg", "button", "form", "select",
    "input", "textarea", "iframe", "object",
];

fn is_hidden(element: &scraper::node::Element) -> bool {
    element.attr("hidden").is_some()
        || element
            .attr("style")
            .is_some_and(|style| HIDDEN_STYLE.is_match(style))
        || element
            .attr("class")
            .is_some_and(|class| class.split_whitespace().any(|c| c.eq_ignore_ascii_case("preheader")))
}

#[derive(Default)]
struct Walker {
    paragraphs: Vec<Paragraph>,
    anchors: Vec<Anchor>,
    buffer: String,
    /// Anchors found in the paragraph being built.
    pending: Vec<Anchor>,
    block_stack: Vec<&'static str>,
    strong_depth: usize,
}

impl Walker {
    fn kind(&self) -> ParagraphKind {
        if self.in_heading() {
            return ParagraphKind::Heading;
        }
        if self.block_stack.contains(&"li") {
            return ParagraphKind::ListItem;
        }
        for tag in self.block_stack.iter().rev() {
            match *tag {
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => return ParagraphKind::Heading,
                "li" => return ParagraphKind::ListItem,
                "blockquote" => return ParagraphKind::Quote,
                "p" | "td" | "div" | "th" => return ParagraphKind::Text,
                _ => {}
            }
        }
        ParagraphKind::Text
    }

    fn in_heading(&self) -> bool {
        self.block_stack
            .iter()
            .any(|tag| matches!(*tag, "h1" | "h2" | "h3" | "h4" | "h5" | "h6"))
    }

    fn flush(&mut self) {
        let kind = self.kind();
        // A double line break inside one block starts a new paragraph.
        let raw = std::mem::take(&mut self.buffer);
        let mut pieces: Vec<String> = raw
            .split("\n\n")
            .map(|piece| {
                collapse(piece)
                    .trim_start_matches(['§', '¶', '#'])
                    .trim()
                    .to_string()
            })
            .filter(|piece| !piece.is_empty())
            .collect();
        if pieces.is_empty() {
            // Anchors with no text of their own (an image link) still count.
            for mut anchor in self.pending.drain(..) {
                anchor.paragraph = self.paragraphs.len();
                self.paragraphs.push(Paragraph {
                    kind,
                    text: String::new(),
                });
                self.anchors.push(anchor);
            }
            return;
        }
        let first = self.paragraphs.len();
        for piece in pieces.drain(..) {
            self.paragraphs.push(Paragraph { kind, text: piece });
        }
        for mut anchor in self.pending.drain(..) {
            // The piece that holds the anchor's text, else the first.
            anchor.paragraph = (first..self.paragraphs.len())
                .find(|&i| !anchor.text.is_empty() && self.paragraphs[i].text.contains(&anchor.text))
                .unwrap_or(first);
            self.anchors.push(anchor);
        }
    }

    fn walk(&mut self, node: ego_tree::NodeRef<'_, Node>) {
        match node.value() {
            Node::Text(text) => {
                self.buffer.push_str(text);
            }
            Node::Element(element) => {
                let name = element.name();
                if SKIP_TAGS.contains(&name) || is_hidden(element) {
                    return;
                }
                if name == "br" {
                    self.buffer.push('\n');
                    return;
                }
                if name == "img" {
                    return;
                }
                let block = BLOCK_TAGS.iter().find(|tag| **tag == name).copied();
                if let Some(tag) = block {
                    self.flush();
                    self.block_stack.push(tag);
                }
                let strong = matches!(name, "strong" | "b");
                if strong {
                    self.strong_depth += 1;
                }
                if name == "a" {
                    if let Some(element_ref) = ElementRef::wrap(node) {
                        self.anchor(element_ref);
                    }
                }
                for child in node.children() {
                    self.walk(child);
                }
                if strong {
                    self.strong_depth -= 1;
                }
                if block.is_some() {
                    self.flush();
                    self.block_stack.pop();
                }
            }
            _ => {
                for child in node.children() {
                    self.walk(child);
                }
            }
        }
    }

    fn anchor(&mut self, element: ElementRef<'_>) {
        let Some(href) = element.value().attr("href") else {
            return;
        };
        let mut text = collapse(&element.text().collect::<String>());
        if text.is_empty() {
            text = element
                .select(&IMG)
                .find_map(|img| img.value().attr("alt"))
                .map(collapse)
                .unwrap_or_default();
        }
        self.pending.push(Anchor {
            text,
            href: href.to_string(),
            paragraph: 0,
            in_heading: self.in_heading(),
            in_strong: self.strong_depth > 0,
        });
    }
}

/// Any HTML (a fetched article's cleaned markup) as reader paragraphs.
pub fn paragraphs_from_html(html: &str) -> Vec<Paragraph> {
    walk_html(html)
        .paragraphs
        .into_iter()
        .filter(|p| !p.text.is_empty())
        .collect()
}

static IMG: Lazy<scraper::Selector> =
    Lazy::new(|| scraper::Selector::parse("img").expect("img selector parses"));

fn walk_html(html: &str) -> Walked {
    let document = Html::parse_document(html);
    let mut walker = Walker::default();
    walker.walk(document.tree.root());
    walker.flush();
    Walked {
        paragraphs: walker.paragraphs,
        anchors: walker.anchors,
    }
}

/// Plain-text issues: paragraphs split on blank lines; a URL's title is the
/// text before it on its line, else the line above.
fn walk_text(text: &str) -> Walked {
    let mut paragraphs = Vec::new();
    let mut anchors = Vec::new();
    for block in text.replace("\r\n", "\n").split("\n\n") {
        let lines: Vec<&str> = block.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
        if lines.is_empty() {
            continue;
        }
        let index = paragraphs.len();
        let mut kept: Vec<String> = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            let urls: Vec<&str> = URL_IN_TEXT.find_iter(line).map(|m| m.as_str()).collect();
            let without = collapse(&URL_IN_TEXT.replace_all(line, " "));
            let without = without
                .trim_matches(|c: char| c.is_whitespace() || "-–—:*•·()[]<>".contains(c))
                .to_string();
            for url in &urls {
                let title = if word_count(&without) >= 2 {
                    without.clone()
                } else if i > 0 && !URL_IN_TEXT.is_match(lines[i - 1]) {
                    lines[i - 1]
                        .trim_matches(|c: char| c.is_whitespace() || "-–—:*•·".contains(c))
                        .to_string()
                } else {
                    String::new()
                };
                anchors.push(Anchor {
                    text: title,
                    href: (*url).to_string(),
                    paragraph: index,
                    in_heading: true,
                    in_strong: false,
                });
            }
            if !without.is_empty() {
                kept.push(without);
            }
        }
        let kind = if lines[0].starts_with(['-', '*', '•']) {
            ParagraphKind::ListItem
        } else {
            ParagraphKind::Text
        };
        paragraphs.push(Paragraph {
            kind,
            text: collapse(&kept.join(" ")),
        });
    }
    Walked {
        paragraphs,
        anchors,
    }
}

/// A button, not a story: "GET THE DISCOUNT →", "View this role ↗",
/// "Watch the keynote".
fn call_to_action(text: &str) -> bool {
    let trimmed = text.trim();
    let letters: Vec<char> = trimmed.chars().filter(|c| c.is_alphabetic()).collect();
    let shouting = letters.len() >= 6 && letters.iter().all(|c| c.is_uppercase());
    trimmed.ends_with(['→', '↗', '›', '»', '>', '➔', '➜'])
        || shouting
        || (word_count(trimmed) <= 4 && CTA_VERB.is_match(trimmed))
}

fn boilerplate_link(anchor: &Anchor) -> bool {
    anchor.text.is_empty()
        || call_to_action(&anchor.text)
        || BOILERPLATE_LINK.is_match(&anchor.text)
        || MASTHEAD.is_match(&anchor.text)
        || BOILERPLATE_HREF.is_match(&anchor.href)
        || anchor.text.starts_with("http")
}

fn link_item(anchor: &Anchor, blurb: Option<String>) -> Option<LinkItem> {
    let url = clean_url(&anchor.href)?;
    Some(LinkItem {
        title: clip(&LIST_MARKER.replace(&anchor.text, ""), 160),
        blurb,
        domain: display_domain(&url),
        tracked: is_click_tracker(&url),
        url: url.to_string(),
    })
}

fn classify(walked: Walked, headline: String, subject: &str) -> Extraction {
    let Walked {
        paragraphs,
        anchors,
    } = walked;
    // The footer starts at the first footer line in the last half; a short
    // issue's whole body can be its last half.
    let half = paragraphs.len() / 2;
    let footer_start = paragraphs
        .iter()
        .enumerate()
        .skip(half.saturating_sub(1))
        .find(|(_, p)| FOOTER.is_match(&p.text))
        .map_or(paragraphs.len(), |(i, _)| i);
    let footer_start = anchors
        .iter()
        .filter(|a| a.paragraph >= half.saturating_sub(1))
        .filter(|a| UNSUBSCRIBE_WORD.is_match(&a.text) || a.href.to_lowercase().contains("unsubscribe"))
        .map(|a| a.paragraph)
        .min()
        .map_or(footer_start, |at| at.min(footer_start));

    let subject_key = collapse(subject).to_lowercase();
    let headline_key = headline.to_lowercase();
    // A paragraph that is nothing but one boilerplate link is a button.
    // So is a row of them (Twitter LinkedIn).
    let mut leftover: std::collections::HashMap<usize, String> = std::collections::HashMap::new();
    for anchor in anchors.iter().filter(|a| boilerplate_link(a) && !a.text.is_empty()) {
        let text = leftover
            .entry(anchor.paragraph)
            .or_insert_with(|| paragraphs[anchor.paragraph].text.clone());
        *text = text.replacen(&anchor.text, " ", 1);
    }
    let buttons: std::collections::HashSet<usize> = leftover
        .into_iter()
        .filter(|(_, rest)| !rest.chars().any(char::is_alphanumeric))
        .map(|(paragraph, _)| paragraph)
        .collect();
    let is_masthead = |(i, p): &(usize, &Paragraph)| {
        let key = p.text.to_lowercase();
        MASTHEAD.is_match(&p.text)
            || DATE_ONLY.is_match(&p.text)
            || buttons.contains(i)
            || key == subject_key
            || key == headline_key
    };
    let body: Vec<(usize, &Paragraph)> = paragraphs
        .iter()
        .enumerate()
        .take(footer_start)
        .filter(|entry| !entry.1.text.is_empty() && !is_masthead(entry))
        .collect();
    let words: usize = body.iter().map(|(_, p)| word_count(&p.text)).sum();

    let standfirst = body.iter().find_map(|(_, p)| {
        if !matches!(p.kind, ParagraphKind::Text | ParagraphKind::Quote) {
            return None;
        }
        let stripped = GREETING.replace(&p.text, "");
        let text = if stripped == p.text {
            stripped.into_owned()
        } else {
            crate::headline::capitalize_first(&stripped)
        };
        (word_count(&text) >= STANDFIRST_MIN_WORDS && !URL_IN_TEXT.is_match(&text))
            .then(|| clip(&text, STANDFIRST_MAX_CHARS))
    });

    let live: Vec<&Anchor> = anchors
        .iter()
        .filter(|a| a.paragraph < footer_start)
        .collect();
    let read_more = live
        .iter()
        .find(|a| READ_MORE.is_match(&a.text) && !BOILERPLATE_HREF.is_match(&a.href));

    let mut seen_urls: Vec<String> = Vec::new();
    let mut seen_titles: Vec<String> = Vec::new();
    let mut links = Vec::new();
    for anchor in &live {
        if boilerplate_link(anchor) || READ_MORE.is_match(&anchor.text) {
            continue;
        }
        let title_words = word_count(&anchor.text);
        if title_words < 2 || title_words > 30 || (title_words < 3 && !anchor.in_heading) {
            continue;
        }
        let paragraph = &paragraphs[anchor.paragraph];
        let para_len = paragraph.text.chars().count().max(1);
        let share = anchor.text.chars().count() as f64 / para_len as f64;
        let leads = paragraph
            .text
            .strip_prefix(&anchor.text)
            .is_some_and(|rest| rest.trim_start().starts_with(['—', '–', '-', ':', '|', '·']) || rest.is_empty());
        let headline_like = anchor.in_heading
            || share >= 0.6
            || (anchor.in_strong && share >= 0.2)
            || (leads && (anchor.in_strong || paragraph.kind == ParagraphKind::ListItem || share >= 0.15));
        if !headline_like {
            continue;
        }
        let blurb = blurb_for(anchor, &paragraphs, &live, footer_start);
        let Some(item) = link_item(anchor, blurb) else {
            continue;
        };
        let title_key = item.title.to_lowercase();
        if seen_urls.contains(&item.url) || seen_titles.contains(&title_key) {
            continue;
        }
        seen_urls.push(item.url.clone());
        seen_titles.push(title_key);
        links.push(item);
    }

    let shape = if links.len() >= DIGEST_MIN_LINKS
        && words / links.len() <= DIGEST_MAX_WORDS_PER_LINK
    {
        Shape::Digest
    } else if words <= TEASER_MAX_WORDS
        && (read_more.is_some()
            || (links.len() == 1 && same_story(&links[0].title, &headline)))
    {
        Shape::Teaser
    } else if words <= NOTICE_MAX_WORDS {
        Shape::Notice
    } else {
        Shape::Single
    };
    let main_link = match shape {
        Shape::Teaser => read_more
            .and_then(|anchor| link_item(anchor, None))
            .or_else(|| links.first().cloned())
            .map(|mut item| {
                if READ_MORE.is_match(&item.title) {
                    item.title.clone_from(&headline);
                }
                item
            }),
        _ => None,
    };
    let body_paragraphs = body.into_iter().map(|(_, p)| p.clone()).collect();
    Extraction {
        shape,
        headline,
        standfirst,
        words: u32::try_from(words).unwrap_or(u32::MAX),
        main_link,
        links: if shape == Shape::Digest { links } else { Vec::new() },
        paragraphs: body_paragraphs,
    }
}

/// Whether a link's title and the issue's headline name the same story:
/// at least half of the shorter one's words appear in the other.
fn same_story(title: &str, headline: &str) -> bool {
    let words = |text: &str| -> Vec<String> {
        text.split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.len() > 2)
            .map(str::to_lowercase)
            .collect()
    };
    let (a, b) = (words(title), words(headline));
    let (short, long) = if a.len() <= b.len() { (&a, &b) } else { (&b, &a) };
    !short.is_empty() && short.iter().filter(|w| long.contains(w)).count() * 2 >= short.len()
}

/// A link's blurb: the rest of its own block, else the block after it when
/// that is plain text with no headline link of its own.
fn blurb_for(
    anchor: &Anchor,
    paragraphs: &[Paragraph],
    anchors: &[&Anchor],
    footer_start: usize,
) -> Option<String> {
    let own = &paragraphs[anchor.paragraph].text;
    let rest = collapse(&own.replacen(&anchor.text, " ", 1));
    let rest = rest
        .trim_matches(|c: char| c.is_whitespace() || "-–—:|·•()".contains(c))
        .to_string();
    if word_count(&rest) >= 6 {
        return Some(clip(&rest, BLURB_MAX_CHARS));
    }
    let next_index = anchor.paragraph + 1;
    let next = paragraphs.get(next_index)?;
    // The next block is the next item's headline, not this one's blurb.
    let next_is_headline = anchors.iter().any(|other| {
        other.paragraph == next_index
            && (other.in_heading
                || other.text.chars().count() * 2 >= next.text.chars().count())
    });
    (next_index < footer_start
        && !next_is_headline
        && matches!(next.kind, ParagraphKind::Text | ParagraphKind::Quote)
        && word_count(&next.text) >= 6
        && !MASTHEAD.is_match(&next.text))
    .then(|| clip(&next.text, BLURB_MAX_CHARS))
}

#[cfg(test)]
mod tests;
