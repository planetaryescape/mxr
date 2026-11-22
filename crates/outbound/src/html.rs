//! Validation and plain-text fallback for supplied HTML bodies.
//!
//! Two rules govern this module:
//!
//! 1. **It never rewrites.** Validation parses the document only to inspect it.
//!    The string that gets stored and transmitted is always the caller's
//!    original bytes. When something is wrong we report it and refuse; we do
//!    not sanitise, minify, reformat, or "improve" the input. A designed email
//!    depends on markup that a sanitiser would happily destroy — Outlook
//!    conditional comments, `<style>` blocks with media queries, table
//!    layouts.
//! 2. **The text alternative is derived, not substituted.** Generating the
//!    `text/plain` part reads the HTML and leaves it untouched.
//!
//! Known edges, written down rather than left as folklore:
//!
//! - A `data:image/*` payload padded with more than `DATA_URL_SNIFF_BYTES` of
//!   leading whitespace hides its signature from
//!   `data_url_payload_is_markup`. A sniffing client skips whitespace with no
//!   bound at all; this one stops, so that a multi-megabyte inlined logo costs
//!   the same to check as a small one.
//! - A `data:` payload that does not decode is allowed through. See
//!   `FORGIVING_BASE64` for why that is sound rather than lazy.

use std::collections::BTreeSet;
use std::fmt;

use base64::Engine as _;
use mxr_html_worker::{AnyJob, FallbackReason, Job};
use scraper::{Html, Node};
use serde::{Deserialize, Serialize};

/// Schemes a URL-bearing attribute may use.
///
/// `data:` is handled separately: only raster `data:image/*` is allowed. A
/// `data:text/html` payload is a script vector, and so is SVG — see
/// `UnsafeUrl::SvgDataUrl`. The declared media type is only the sender's
/// label, so the payload behind it is checked too; see
/// `data_url_payload_is_markup`.
const ALLOWED_SCHEMES: &[&str] = &["http", "https", "mailto", "cid", "tel"];

/// How much of a `data:` payload to decode before giving up on a signature.
///
/// The longest thing being looked for is a single `<`, so this is almost
/// entirely headroom. It is not sized for the signature; it is sized against
/// evasion. A client that sniffs skips leading whitespace with no bound, so a
/// tight prefix would be defeated by padding the payload with spaces. The bound
/// exists so the work is constant: a two-megabyte inlined logo costs the same
/// to check as a small one.
const DATA_URL_SNIFF_BYTES: usize = 4096;

/// A base64 decoder as forgiving as the one a mail client uses.
///
/// WHATWG "forgiving-base64", which is how a `data:` URL is decoded, strips
/// ASCII whitespace, tolerates missing padding, and ignores non-zero trailing
/// bits. Matching it is what makes it sound to allow an undecodable payload
/// through: a payload *this* cannot read is a payload *no client* can read, so
/// it is not a hidden SVG — it is nothing at all.
///
/// Do not "tighten" this into a stricter decoder. A stricter decoder would
/// start rejecting payloads that clients accept, turning a check that refuses
/// active content into one that refuses legitimate mail.
const FORGIVING_BASE64: base64::engine::GeneralPurpose = base64::engine::GeneralPurpose::new(
    &base64::alphabet::STANDARD,
    base64::engine::GeneralPurposeConfig::new()
        .with_decode_padding_mode(base64::engine::DecodePaddingMode::Indifferent)
        .with_decode_allow_trailing_bits(true),
);

/// Elements that execute, submit, navigate, or embed foreign content.
///
/// The SVG animation elements are here because `<animate
/// attributeName="href" values="javascript:...">` reaches script through
/// indirection that no attribute-by-attribute rule can follow. Nothing in a
/// mail template needs them, so refusing the elements is both simpler and
/// safer than reasoning about what they animate.
///
/// `foreignobject` is here for the same reason: it embeds arbitrary XHTML
/// inside `<svg>`, so allowing it would mean re-deciding every rule in this
/// module for a second document buried in the first. html5ever normalises the
/// name to `foreignObject` whatever case it was written in, and the caller
/// lowercases before the lookup.
const FORBIDDEN_ELEMENTS: &[&str] = &[
    "script",
    "object",
    "embed",
    "applet",
    "iframe",
    "frame",
    "frameset",
    "form",
    "input",
    "button",
    "textarea",
    "select",
    "base",
    "animate",
    "animatemotion",
    "animatetransform",
    "set",
    "foreignobject",
];

/// Constructs that make a fragment of CSS active content.
///
/// `expression()` is legacy IE script-in-CSS; `javascript:` in a `url()` is the
/// same idea; `-moz-binding` attaches XBL. Shared by `<style>` blocks and
/// inline `style` attributes so the two cannot drift apart — most mail clients
/// strip `<style>`, which makes the attribute the common case in real email.
const ACTIVE_CSS_CONSTRUCTS: &[&str] = &["expression(", "javascript:", "-moz-binding"];

/// Elements html5ever discards when they appear with no table ancestor.
///
/// The HTML tree-construction rules drop a stray `<td>` token outright, so the
/// element — and any `onclick` it carries — never reaches the validator. A more
/// forgiving mail client parser may well keep it. Measured, not assumed: see
/// `only_table_scoped_elements_vanish_from_a_document_parse`.
const TABLE_SCOPED_ELEMENTS: &[&str] = &[
    "td", "tr", "th", "tbody", "thead", "tfoot", "caption", "col", "colgroup",
];

/// Attributes whose whole value is a single URL.
///
/// These are local names. html5ever adjusts foreign attributes, and scraper
/// yields only the local part, so SVG's `xlink:href` arrives here as `href` and
/// needs no entry of its own — see
/// `every_url_bearing_attribute_is_checked_not_just_href_and_src`.
///
/// `srcset` is deliberately absent: it holds a list, not a URL, and is handled
/// separately.
const URL_ATTRIBUTES: &[&str] = &[
    "href",
    "src",
    "action",
    "background",
    "poster",
    "formaction",
    "longdesc",
    "cite",
    "usemap",
    "dynsrc",
    "lowsrc",
];

/// One reason a document was refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HtmlIssue {
    pub kind: HtmlIssueKind,
    /// 1-based line in the supplied document, when it could be located.
    pub line: Option<usize>,
    /// Short, truncated excerpt of the offending construct.
    pub detail: String,
    /// Which scan found it. Private: it exists to keep issues from different
    /// contexts distinguishable, not to be read by callers.
    origin: IssueOrigin,
}

/// Which scan found an issue.
///
/// Part of an issue's identity, because "same kind, same line, same excerpt" is
/// not the same as "same problem". The same markup written both inside a
/// conditional comment and outside it is two problems and has to be reported
/// twice, while a real table cell seen by both the document parse and the
/// orphan reparse is one problem and must not be. Without this, the
/// duplicate-suppression in `orphan_table_element_issues` cannot tell the two
/// cases apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum IssueOrigin {
    /// The document itself.
    Document,
    /// The body of the nth comment, counted in document order.
    Comment(usize),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HtmlIssueKind {
    /// An element that executes or submits, e.g. `<script>`, `<form>`.
    ForbiddenElement { tag: String },
    /// An inline event handler, e.g. `onclick`.
    EventHandler { attribute: String },
    /// A URL whose scheme is not on the allowlist.
    UnsafeUrlScheme { attribute: String, scheme: String },
    /// A `data:image/svg+xml` payload, refused where raster images are allowed.
    SvgDataUrl { attribute: String },
    /// A `data:image/*` URL whose payload is markup rather than an image, so a
    /// client that sniffs the content gets a document the declared type
    /// promised it would not.
    MislabelledDataUrl { attribute: String, declared: String },
    /// A `<style>` block containing an active-content construct.
    UnsafeStyle,
    /// An inline `style` attribute containing an active-content construct.
    /// The common case in email, where `<style>` blocks are often stripped.
    UnsafeStyleAttribute,
    /// A forbidden element hidden inside a comment. Outlook conditional
    /// comments are real markup to Outlook, so a `<script>` in one is a live
    /// script, not inert text.
    ForbiddenElementInComment { tag: String },
    /// The document is over the parser budget (size, tag count, attribute count
    /// or table shape), so it cannot be checked safely. It is refused, and the
    /// fix is to simplify the HTML.
    TooComplexToCheck,
    /// The HTML helper process could not be started, so nothing was checked.
    /// It is refused like a document that is too complex, but the fix is to
    /// wait and try again, not to change the HTML.
    CheckerUnavailable,
}

impl fmt::Display for HtmlIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let where_ = match self.line {
            Some(line) => format!(" (line {line})"),
            None => String::new(),
        };
        match &self.kind {
            HtmlIssueKind::TooComplexToCheck => write!(
                f,
                "HTML is too complex to check safely, so it cannot be sent{where_}: \
                 simplify it (fewer tags, attributes or table cells) and try again"
            ),
            HtmlIssueKind::CheckerUnavailable => write!(
                f,
                "HTML could not be checked because mxr's HTML helper process did not \
                 start, so it cannot be sent yet: try again in a few seconds"
            ),
            HtmlIssueKind::ForbiddenElement { tag } => {
                write!(
                    f,
                    "<{tag}> is not allowed in email HTML{where_}: {}",
                    self.detail
                )
            }
            HtmlIssueKind::EventHandler { attribute } => write!(
                f,
                "inline event handler `{attribute}` is not allowed{where_}: {}",
                self.detail
            ),
            HtmlIssueKind::UnsafeUrlScheme { attribute, scheme } => write!(
                f,
                "`{attribute}` uses unsafe URL scheme `{scheme}:`{where_}: {}",
                self.detail
            ),
            HtmlIssueKind::SvgDataUrl { attribute } => write!(
                f,
                "`{attribute}` uses a `data:image/svg+xml` URL{where_}: SVG is the one \
                 image format that can carry <script> and event handlers, so it is \
                 refused where data:image/png, jpeg, gif and webp are allowed. This is \
                 deliberate, not a bug — inline the artwork as a raster image or link \
                 it over https. {}",
                self.detail
            ),
            HtmlIssueKind::MislabelledDataUrl {
                attribute,
                declared,
            } => write!(
                f,
                "`{attribute}` declares `data:{declared}` but its payload begins with \
                 markup{where_}: a client that sniffs the content gets a document rather \
                 than an image, and an SVG or HTML document can carry <script>. The \
                 declared type is only a label; the bytes decide. Inline the real raster \
                 image or link the artwork over https. {}",
                self.detail
            ),
            HtmlIssueKind::UnsafeStyle => {
                write!(
                    f,
                    "<style> block contains active content{where_}: {}",
                    self.detail
                )
            }
            HtmlIssueKind::UnsafeStyleAttribute => write!(
                f,
                "inline `style` attribute contains active content{where_}: {}",
                self.detail
            ),
            HtmlIssueKind::ForbiddenElementInComment { tag } => write!(
                f,
                "<{tag}> inside a conditional comment is not allowed{where_}: {}",
                self.detail
            ),
        }
    }
}

/// Every reason a document was refused, in document order where possible.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HtmlValidationError {
    pub issues: Vec<HtmlIssue>,
}

impl fmt::Display for HtmlValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "HTML body rejected ({} issue{}):",
            self.issues.len(),
            if self.issues.len() == 1 { "" } else { "s" }
        )?;
        for issue in &self.issues {
            writeln!(f, "  - {issue}")?;
        }
        write!(
            f,
            "The HTML was not modified. Fix the source and try again."
        )
    }
}

impl std::error::Error for HtmlValidationError {}

/// Inspect `html` for active or unsafe content.
///
/// Returns `Ok(())` when the document is safe to send. The input is never
/// modified; on failure the caller still holds the exact original bytes.
///
/// Deliberately permitted, because email depends on them: tables, inline
/// `style` attributes, `<style>` blocks, `@media` queries, Outlook conditional
/// comments, arbitrary Unicode.
///
/// Best-effort by nature: this parses with html5ever, and a given mail client's
/// parser may disagree at the margins. It reports and refuses; it never
/// silently rewrites to "make safe". Where html5ever is stricter than a mail
/// client is likely to be — it discards stray table cells — a second parse of a
/// throw-away copy covers the difference; see
/// `orphan_table_element_issues`.
pub fn validate_html(html: &str) -> Result<(), HtmlValidationError> {
    VALIDATE
        .try_call(html, &())
        .unwrap_or_else(|reason| Err(unchecked(unchecked_kind(reason))))
}

fn unchecked_kind(reason: FallbackReason) -> HtmlIssueKind {
    match reason {
        FallbackReason::Unavailable => HtmlIssueKind::CheckerUnavailable,
        FallbackReason::Oversize
        | FallbackReason::KnownBad
        | FallbackReason::Timeout
        | FallbackReason::HelperDied
        | FallbackReason::JobFailed => HtmlIssueKind::TooComplexToCheck,
    }
}

fn too_complex_to_check() -> HtmlValidationError {
    unchecked(HtmlIssueKind::TooComplexToCheck)
}

fn unchecked(kind: HtmlIssueKind) -> HtmlValidationError {
    HtmlValidationError {
        issues: vec![HtmlIssue {
            kind,
            line: None,
            detail: String::new(),
            origin: IssueOrigin::Document,
        }],
    }
}

static VALIDATE: Job<(), Result<(), HtmlValidationError>> =
    Job::new("outbound.validate_html", |html, ()| {
        validate_html_in_process(html)
    });

fn validate_html_in_process(html: &str) -> Result<(), HtmlValidationError> {
    if !crate::html_limits::within_budget(html) {
        return Err(too_complex_to_check());
    }
    let document = Html::parse_document(html);
    let mut issues = Vec::new();
    let mut comments_seen = 0;

    for node in document.tree.nodes() {
        match node.value() {
            Node::Element(element) => {
                let tag = element.name().to_ascii_lowercase();

                if FORBIDDEN_ELEMENTS.contains(&tag.as_str()) {
                    issues.push(HtmlIssue {
                        line: locate_line(html, &format!("<{tag}")),
                        detail: truncate(&format!("<{tag}>"), 80),
                        kind: HtmlIssueKind::ForbiddenElement { tag: tag.clone() },
                        origin: IssueOrigin::Document,
                    });
                }

                if tag == "meta" && is_meta_refresh(element) {
                    issues.push(HtmlIssue {
                        line: locate_line(html, "http-equiv"),
                        detail: "<meta http-equiv=\"refresh\">".to_string(),
                        kind: HtmlIssueKind::ForbiddenElement {
                            tag: "meta http-equiv=refresh".to_string(),
                        },
                        origin: IssueOrigin::Document,
                    });
                }

                issues.extend(attribute_issues(element, html));
            }
            Node::Comment(comment) => {
                // Conditional comments are markup to Outlook. Scan them with
                let body = comment.comment.as_ref();

                let mut found = Vec::new();

                for tag in forbidden_tags_in(body) {
                    found.push(HtmlIssue {
                        line: locate_line(html, &format!("<{tag}")),
                        detail: truncate(body, 80),
                        kind: HtmlIssueKind::ForbiddenElementInComment { tag },
                        origin: IssueOrigin::Document,
                    });
                }

                // Outlook runs the handlers, URLs and `<style>` bodies in
                // there too, so they get the same treatment as real markup.
                let inner = Html::parse_fragment(body);
                for node in inner.tree.nodes() {
                    if let Node::Element(element) = node.value() {
                        found.extend(attribute_issues(element, html));
                    }
                }
                found.extend(unsafe_style_blocks(&inner, html));
                let orphans = orphan_table_element_issues(body, html, &found);
                found.extend(orphans);

                for issue in &mut found {
                    issue.origin = IssueOrigin::Comment(comments_seen);
                }
                comments_seen += 1;
                issues.extend(found);
            }
            _ => {}
        }
    }

    issues.extend(unsafe_style_blocks(&document, html));

    let orphans = orphan_table_element_issues(html, html, &issues);
    issues.extend(orphans);

    if issues.is_empty() {
        Ok(())
    } else {
        Err(HtmlValidationError { issues })
    }
}

/// Deterministic `text/plain` alternative generated from an HTML body.
///
/// Uses the same renderer and width as the reader's HTML path
/// (`crates/reader/src/html.rs`), so the text a recipient gets matches what
/// mxr itself would show. Reads the HTML; never mutates it.
pub fn generate_text_alternative(html: &str) -> String {
    TEXT_ALTERNATIVE
        .call(html, &())
        .flatten()
        .unwrap_or_else(|| strip_tags_fallback(html))
}

/// The render, which runs in the HTML helper process. `None` is the tag-strip
/// fallback.
// Guarded like the reader's call (html2text panics on some malformed tables).
// mxr-outbound cannot depend on mxr-reader, so the bounds, the span clamp and
// the fallback are local copies (`html_limits`). Bounded input first: an
// adversarial table costs html2text gigabytes, and an allocation failure
// aborts, so `catch_unwind` cannot protect against it.
static TEXT_ALTERNATIVE: Job<(), Option<String>> =
    Job::new("outbound.text_alternative", |html, ()| {
        let bounded = crate::html_limits::bounded_html(html)?;
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            html2text::config::plain_no_decorate().string_from_read(bounded.as_bytes(), 80)
        }))
        .ok()
        .and_then(Result::ok)
    });

/// Every parse this crate runs in the HTML helper process.
pub fn html_jobs() -> Vec<&'static dyn AnyJob> {
    vec![&VALIDATE, &TEXT_ALTERNATIVE]
}

/// Tag-stripping fallback for `generate_text_alternative` when html2text
/// panics. Drops `<...>` runs and decodes the common entities.
fn strip_tags_fallback(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => text.push(ch),
            _ => {}
        }
    }
    text.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Inline handlers and unsafe URLs carried by one element's attributes.
///
/// `html` is the whole original document, used only to locate line numbers —
/// including for elements that came out of a comment, which is where they
/// appear in the caller's source.
fn attribute_issues(element: &scraper::node::Element, html: &str) -> Vec<HtmlIssue> {
    let mut issues = Vec::new();

    // html5ever decodes character references in attribute *values*, so an
    // obfuscated `jav&#x0A;ascript:` arrives here already decoded. That is why
    // the document is parsed rather than scanned with a regex. Attribute
    // *names* are not decoded by any HTML parser, so `o&#110;click` is an inert
    // attribute rather than a disguised handler.
    for (name, value) in element.attrs() {
        let attribute = name.to_ascii_lowercase();

        if attribute.starts_with("on") && attribute.len() > 2 {
            issues.push(HtmlIssue {
                line: locate_line(html, &attribute),
                detail: truncate(value, 60),
                kind: HtmlIssueKind::EventHandler { attribute },
                origin: IssueOrigin::Document,
            });
            continue;
        }

        if attribute == "style" {
            issues.extend(css_issues(value, html, HtmlIssueKind::UnsafeStyleAttribute));
            continue;
        }

        let urls: Vec<&str> = if attribute == "srcset" || attribute == "imagesrcset" {
            srcset_candidates(value).collect()
        } else if URL_ATTRIBUTES.contains(&attribute.as_str()) {
            vec![value]
        } else {
            continue;
        };

        for url in urls {
            if let Some(verdict) = unsafe_url(url) {
                issues.push(HtmlIssue {
                    line: locate_line(html, url.trim()),
                    detail: truncate(url, 60),
                    kind: verdict.into_kind(attribute.clone()),
                    origin: IssueOrigin::Document,
                });
            }
        }
    }

    issues
}

/// The URLs in a `srcset` value.
///
/// `srcset` is a comma-separated list of `url [descriptor]` candidates, as in
/// `hero.png 1x, hero@2x.png 2x`. Scheme-checking the whole value finds
/// nothing, because the commas and spaces disqualify it as a scheme.
fn srcset_candidates(value: &str) -> impl Iterator<Item = &str> {
    value
        .split(',')
        .filter_map(|candidate| candidate.split_whitespace().next())
}

/// Every reason a fragment of CSS is refused.
///
/// The single rule set for both `<style>` blocks and inline `style`
/// attributes. Splitting these was the original defect, and CSS `url()` was a
/// way straight round the scheme allowlist that guards `src` and `href`, so
/// both checks live here: the active-content constructs, and every `url()`
/// payload run through the same `unsafe_url` the attributes use.
///
/// `style_kind` is the only thing that differs between the two callers.
fn css_issues(css: &str, html: &str, style_kind: HtmlIssueKind) -> Vec<HtmlIssue> {
    let mut issues = Vec::new();

    if let Some(offender) = active_css_construct(css) {
        issues.push(HtmlIssue {
            line: locate_line(html, offender),
            detail: truncate(offender, 40),
            kind: style_kind,
            origin: IssueOrigin::Document,
        });
    }

    for url in css_url_payloads(css) {
        if let Some(verdict) = unsafe_url(url) {
            issues.push(HtmlIssue {
                line: locate_line(html, url.trim()),
                detail: truncate(url, 60),
                kind: verdict.into_kind("style".to_string()),
                origin: IssueOrigin::Document,
            });
        }
    }

    issues
}

/// The payload of every `url()` in a fragment of CSS.
///
/// Covers `url(x)`, `url('x')`, `url("x")`, whitespace inside the parens, and
/// several `url()` in one declaration. A `)` inside the payload ends it early,
/// which can only truncate a URL — never hide its scheme, which is what the
/// caller looks at.
fn css_url_payloads(css: &str) -> Vec<&str> {
    const OPENER: &str = "url(";

    let lowered = css.to_ascii_lowercase();
    let mut payloads = Vec::new();
    let mut cursor = 0;

    while let Some(offset) = lowered[cursor..].find(OPENER) {
        let open = cursor + offset + OPENER.len();
        let Some(close) = lowered[open..].find(')').map(|at| open + at) else {
            break;
        };
        payloads.push(unquote(css[open..close].trim()));
        cursor = close + 1;
    }

    payloads
}

/// `value` without one surrounding pair of matching quotes.
fn unquote(value: &str) -> &str {
    ['\'', '"']
        .into_iter()
        .find_map(|quote| {
            value
                .strip_prefix(quote)
                .and_then(|inner| inner.strip_suffix(quote))
        })
        .unwrap_or(value)
}

/// The first active-content construct in `css`, when there is one.
///
/// One function for `<style>` blocks and inline `style` attributes alike; the
/// two having separate rules was the defect.
fn active_css_construct(css: &str) -> Option<&'static str> {
    let lowered = css.to_ascii_lowercase();
    ACTIVE_CSS_CONSTRUCTS
        .iter()
        .copied()
        .find(|needle| lowered.contains(needle))
}

/// Attribute issues on table-scoped elements the parse of `markup` threw away.
///
/// html5ever obeys the HTML tree-construction rules, so a `<td onclick="...">`
/// with no table ancestor is dropped before the validator ever sees it. Parse a
/// throw-away copy wrapped in `<table>`, where those tokens are legal, and take
/// only the table-scoped elements from it. Everything else in the copy has
/// already been inspected in its proper place, and `already_reported` keeps a
/// real table's cells from being counted twice.
///
/// `html` is the caller's original document, used only for line numbers. The
/// copy is never handed back.
///
/// The suppression subtracts *one copy per copy* already reported, rather than
/// every copy that looks alike. Treating it as a set was a miss, not merely an
/// under-report: in
/// `<table><tr><td onclick></table><td onclick>` the real cell and the orphan
/// produce identical issues, so the real one swallowed the orphan and a
/// genuinely unreported problem went unreported.
fn orphan_table_element_issues(
    markup: &str,
    html: &str,
    already_reported: &[HtmlIssue],
) -> Vec<HtmlIssue> {
    let wrapped = format!("<table>{}", without_table_end_tags(markup));
    let fragment = Html::parse_fragment(&wrapped);
    let mut unclaimed: Vec<&HtmlIssue> = already_reported.iter().collect();

    fragment
        .tree
        .nodes()
        .filter_map(|node| node.value().as_element())
        .filter(|element| {
            TABLE_SCOPED_ELEMENTS
                .iter()
                .any(|tag| element.name().eq_ignore_ascii_case(tag))
        })
        .flat_map(|element| attribute_issues(element, html))
        .filter(
            |issue| match unclaimed.iter().position(|seen| *seen == issue) {
                Some(at) => {
                    unclaimed.swap_remove(at);
                    false
                }
                None => true,
            },
        )
        .collect()
}

/// `markup` with every `</table>` end tag removed.
///
/// The wrapper in `orphan_table_element_issues` is the only thing keeping
/// stray table tokens alive, and an unmatched `</table>` in the source would
/// close it — hiding exactly the markup being looked for. Dropping end tags
/// from the throw-away copy cannot invent an attribute, so it cannot invent an
/// issue either.
fn without_table_end_tags(markup: &str) -> String {
    const END_TAG: &str = "</table";

    let lowered = markup.to_ascii_lowercase();
    let mut kept = String::with_capacity(markup.len());
    let mut cursor = 0;

    while let Some(offset) = lowered[cursor..].find(END_TAG) {
        let start = cursor + offset;
        let after_name = start + END_TAG.len();
        match lowered[after_name..].find('>').map(|at| after_name + at) {
            Some(close) if markup[after_name..close].trim().is_empty() => {
                kept.push_str(&markup[cursor..start]);
                cursor = close + 1;
            }
            _ => {
                kept.push_str(&markup[cursor..after_name]);
                cursor = after_name;
            }
        }
    }
    kept.push_str(&markup[cursor..]);
    kept
}

fn is_meta_refresh(element: &scraper::node::Element) -> bool {
    element
        .attr("http-equiv")
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("refresh"))
}

/// Why a URL is refused.
enum UnsafeUrl {
    /// A scheme outside `ALLOWED_SCHEMES`.
    Scheme(String),
    /// `data:image/svg+xml`. SVG is the one image format that carries
    /// `<script>` and event handlers, so it is refused where raster images are
    /// not. `<img>` will not run it and `<object>`/`<embed>` are forbidden
    /// outright, but a mail client's behaviour is not something to bet on.
    SvgDataUrl,
    /// A `data:image/*` URL whose payload turned out to be markup, whatever
    /// media type it declared.
    MislabelledDataUrl { declared: String },
}

impl UnsafeUrl {
    /// The reported issue for this verdict on `attribute`.
    fn into_kind(self, attribute: String) -> HtmlIssueKind {
        match self {
            Self::Scheme(scheme) => HtmlIssueKind::UnsafeUrlScheme { attribute, scheme },
            Self::SvgDataUrl => HtmlIssueKind::SvgDataUrl { attribute },
            Self::MislabelledDataUrl { declared } => HtmlIssueKind::MislabelledDataUrl {
                attribute,
                declared,
            },
        }
    }
}

/// What is wrong with `url`, when something is.
///
/// Returns `None` for allowed schemes, raster `data:image/*`, and
/// scheme-relative or relative URLs.
fn unsafe_url(url: &str) -> Option<UnsafeUrl> {
    // Browsers and mail clients strip whitespace and control characters before
    // resolving a scheme, so `java\tscript:` is live. Do the same here.
    let cleaned: String = url
        .chars()
        .filter(|c| !c.is_whitespace() && !c.is_control())
        .collect();

    let colon = cleaned.find(':')?;
    let scheme = &cleaned[..colon];

    if scheme.is_empty() || !scheme.starts_with(|c: char| c.is_ascii_alphabetic()) {
        return None;
    }
    if !scheme
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    {
        return None;
    }
    if cleaned[..colon].contains(['/', '?', '#']) {
        return None;
    }

    let scheme = scheme.to_ascii_lowercase();

    if ALLOWED_SCHEMES.contains(&scheme.as_str()) {
        return None;
    }
    if scheme == "data" {
        let rest = &cleaned[colon + 1..];
        let declared = rest
            .split([';', ','])
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();

        if declared.starts_with("image/svg") {
            return Some(UnsafeUrl::SvgDataUrl);
        }
        if declared.starts_with("image/") {
            return data_url_payload_is_markup(rest)
                .then_some(UnsafeUrl::MislabelledDataUrl { declared });
        }
    }

    Some(UnsafeUrl::Scheme(scheme))
}

/// Whether a `data:image/*` payload is markup rather than an image.
///
/// The declared media type is a label the sender chose; a client that sniffs
/// reads the bytes. `data:image/png;base64,<an SVG document>` is an SVG to
/// anything that sniffs, and SVG carries `<script>`.
///
/// The test is deliberately "is this markup", not "does this match the raster
/// format it claims". Magic-byte matching would have to enumerate every format
/// a mail client renders — png, jpeg, gif, webp, avif, heic, bmp, ico, tiff —
/// and would refuse a legitimate image the moment that list fell behind the
/// formats in use. Nothing that begins with `<` is a raster image in any
/// format, so the narrower test gives up no coverage while risking far fewer
/// false positives.
///
/// An unreadable payload is allowed through; see `FORGIVING_BASE64`.
fn data_url_payload_is_markup(rest: &str) -> bool {
    let Some((parameters, payload)) = rest.split_once(',') else {
        return false;
    };

    let decoded = if parameters.to_ascii_lowercase().ends_with(";base64") {
        decode_base64_prefix(payload)
    } else {
        percent_decode_prefix(payload)
    };

    // A sniffing client skips leading whitespace before matching a signature.
    decoded
        .iter()
        .find(|byte| !byte.is_ascii_whitespace())
        .is_some_and(|byte| *byte == b'<')
}

/// At most `DATA_URL_SNIFF_BYTES` bytes from the front of a base64 payload.
///
/// Empty when the prefix does not decode, which the caller treats as "not
/// markup" rather than as a refusal.
fn decode_base64_prefix(payload: &str) -> Vec<u8> {
    let wanted = DATA_URL_SNIFF_BYTES.div_ceil(3) * 4;
    let bytes = payload.as_bytes();
    let mut prefix = &bytes[..bytes.len().min(wanted)];

    if let Some(pad) = prefix.iter().position(|byte| *byte == b'=') {
        prefix = &prefix[..pad];
    }
    if prefix.len() % 4 == 1 {
        prefix = &prefix[..prefix.len() - 1];
    }

    FORGIVING_BASE64.decode(prefix).unwrap_or_default()
}

/// At most `DATA_URL_SNIFF_BYTES` bytes from the front of a percent-encoded
/// payload.
///
/// An invalid escape is kept verbatim, which is what URL percent-decoding does
/// with one.
fn percent_decode_prefix(payload: &str) -> Vec<u8> {
    let bytes = payload.as_bytes();
    let mut decoded = Vec::new();
    let mut cursor = 0;

    while cursor < bytes.len() && decoded.len() < DATA_URL_SNIFF_BYTES {
        let escaped = (bytes[cursor] == b'%')
            .then(|| bytes.get(cursor + 1..cursor + 3))
            .flatten()
            .filter(|pair| pair.iter().all(u8::is_ascii_hexdigit))
            .and_then(|pair| std::str::from_utf8(pair).ok())
            .and_then(|pair| u8::from_str_radix(pair, 16).ok());

        match escaped {
            Some(byte) => {
                decoded.push(byte);
                cursor += 3;
            }
            None => {
                decoded.push(bytes[cursor]);
                cursor += 1;
            }
        }
    }

    decoded
}

fn unsafe_style_blocks(document: &Html, html: &str) -> Vec<HtmlIssue> {
    let selector = match scraper::Selector::parse("style") {
        Ok(selector) => selector,
        Err(_) => return Vec::new(),
    };

    document
        .select(&selector)
        .flat_map(|element| {
            let css = element.text().collect::<String>();
            css_issues(&css, html, HtmlIssueKind::UnsafeStyle)
        })
        .collect()
}

/// Forbidden tag names appearing inside a comment body.
fn forbidden_tags_in(comment: &str) -> BTreeSet<String> {
    let lowered = comment.to_ascii_lowercase();
    FORBIDDEN_ELEMENTS
        .iter()
        .filter(|tag| {
            lowered
                .match_indices(&format!("<{tag}"))
                .any(|(index, matched)| {
                    lowered[index + matched.len()..]
                        .chars()
                        .next()
                        .is_none_or(|c| !c.is_ascii_alphanumeric() && c != '-')
                })
        })
        .map(|tag| (*tag).to_string())
        .collect()
}

/// 1-based line of the first case-insensitive occurrence of `needle`.
fn locate_line(haystack: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return None;
    }
    let lowered_haystack = haystack.to_ascii_lowercase();
    let index = lowered_haystack.find(&needle.to_ascii_lowercase())?;
    Some(haystack[..index].bytes().filter(|b| *b == b'\n').count() + 1)
}

fn truncate(value: &str, max_chars: usize) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let kept: String = trimmed.chars().take(max_chars).collect();
    format!("{kept}…")
}

#[cfg(test)]
mod tests {
    #[test]
    fn text_alternative_survives_a_malformed_table_span() {
        // html2text panics on this colspan; the alternative must still carry the text.
        let text = generate_text_alternative(
            r#"<table><tr><td colspan="18446744073709551615">A</td><td>B</td></tr></table>"#,
        );
        assert!(text.contains('A') && text.contains('B'), "got {text:?}");
    }

    use super::*;

    /// Refuse `markup`, or fail with a message naming what got through.
    fn refusal(markup: &str) -> HtmlValidationError {
        match validate_html(markup) {
            Err(error) => error,
            Ok(()) => panic!("should have been refused: {markup}"),
        }
    }

    #[test]
    fn the_markup_a_designed_email_depends_on_is_never_flagged() {
        for markup in [
            r#"<table role="presentation" cellpadding="0"><tr><td style="padding:24px;color:#1a1a1a">x</td></tr></table>"#,
            "<style>@media only screen and (max-width:600px){.c{width:100%!important}}</style>",
            "<!--[if mso]><style>.f{font-family:Arial,sans-serif}</style><![endif]-->",
            "<!--[if gte mso 9]><xml><o:OfficeDocumentSettings/></xml><![endif]-->",
            "<!-- an ordinary authoring note -->",
            r#"<a href="https://example.com/read">x</a>"#,
            r#"<a href="http://example.com/read">x</a>"#,
            r#"<a href="mailto:a@example.com?subject=Hi%20there">x</a>"#,
            r#"<a href="tel:+15550100">x</a>"#,
            r#"<img src="cid:notto-logo" alt="Notto®" width="120">"#,
            r#"<img src="data:image/png;base64,iVBORw0KGgo=">"#,
            r#"<img src="data:image/gif;base64,R0lGOD">"#,
            r#"<a href="/relative/path">x</a>"#,
            r#"<a href="page.html?range=1:2">x</a>"#,
            r#"<a href="//example.com/scheme-relative">x</a>"#,
            r##"<a href="#anchor">x</a>"##,
            r#"<meta charset="utf-8">"#,
            r#"<meta name="viewport" content="width=device-width">"#,
            r#"<meta http-equiv="content-type" content="text/html; charset=utf-8">"#,
            "<p>Notto® — café, naïve, 日本語, 🎉</p>",
            "",
            "   \n\t  ",
        ] {
            assert_eq!(validate_html(markup), Ok(()), "wrongly refused: {markup}");
        }
    }

    #[test]
    fn every_executing_or_submitting_element_is_rejected_by_name() {
        for (markup, tag) in [
            ("<script>alert(1)</script>", "script"),
            ("<object data='x.swf'></object>", "object"),
            ("<embed src='x.swf'>", "embed"),
            ("<applet code='x'></applet>", "applet"),
            ("<iframe src='https://evil.example'></iframe>", "iframe"),
            ("<form action='https://evil.example'></form>", "form"),
            ("<input name='p'>", "input"),
            ("<button>go</button>", "button"),
            ("<textarea>x</textarea>", "textarea"),
            ("<select><option>x</option></select>", "select"),
            ("<base href='https://evil.example'>", "base"),
        ] {
            let error = refusal(markup);
            assert!(
                error.issues.iter().any(|issue| issue.kind
                    == HtmlIssueKind::ForbiddenElement {
                        tag: tag.to_string()
                    }),
                "expected <{tag}> to be named in the refusal for {markup}, got {:?}",
                error.issues
            );
        }
    }

    #[test]
    fn inline_event_handlers_are_rejected() {
        for markup in [
            r#"<p onclick="steal()">x</p>"#,
            r#"<img src="cid:a" onerror="steal()">"#,
            r#"<body onload="steal()"><p>x</p></body>"#,
            r#"<p ONMOUSEOVER="steal()">x</p>"#,
            "<p onclick=steal()>x</p>",
            r#"<table><tr><td onmouseenter="steal()">x</td></tr></table>"#,
        ] {
            let error = refusal(markup);
            assert!(
                error
                    .issues
                    .iter()
                    .any(|issue| matches!(issue.kind, HtmlIssueKind::EventHandler { .. })),
                "expected an EventHandler issue for {markup}, got {:?}",
                error.issues
            );
        }
    }

    #[test]
    fn unsafe_url_schemes_are_rejected_wherever_a_url_can_live() {
        for markup in [
            r#"<a href="javascript:steal()">x</a>"#,
            r#"<a href="vbscript:steal()">x</a>"#,
            r#"<a href="JaVaScRiPt:steal()">x</a>"#,
            "<a href=\"java\tscript:steal()\">x</a>",
            "<a href=\"   javascript:steal()\">x</a>",
            r#"<a href="jav&#x0A;ascript:steal()">x</a>"#,
            r#"<img src="javascript:steal()">"#,
            r#"<table><tr><td background="javascript:steal()">x</td></tr></table>"#,
            r#"<a href="file:///etc/passwd">x</a>"#,
        ] {
            let error = refusal(markup);
            assert!(
                error
                    .issues
                    .iter()
                    .any(|issue| matches!(issue.kind, HtmlIssueKind::UnsafeUrlScheme { .. })),
                "expected an UnsafeUrlScheme issue for {markup}, got {:?}",
                error.issues
            );
        }
    }

    #[test]
    fn style_blocks_survive_but_active_css_inside_them_does_not() {
        assert_eq!(
            validate_html("<style>@media (max-width:600px){.a{width:100%}}</style>"),
            Ok(())
        );
        for markup in [
            "<style>.a{width:expression(alert(1))}</style>",
            "<style>.a{width:EXPRESSION(alert(1))}</style>",
            "<style>.a{background:url(javascript:alert(1))}</style>",
            "<style>@import url(\"javascript:alert(1)\");</style>",
            "<style>.a{-moz-binding:url(evil.xml#x)}</style>",
        ] {
            let error = refusal(markup);
            assert!(
                error
                    .issues
                    .iter()
                    .any(|issue| issue.kind == HtmlIssueKind::UnsafeStyle),
                "expected an UnsafeStyle issue for {markup}, got {:?}",
                error.issues
            );
        }
    }

    #[test]
    fn a_script_hidden_in_a_conditional_comment_is_not_a_blind_spot() {
        for markup in [
            "<!--[if mso]><script>alert(1)</script><![endif]-->",
            "<!--[if gte mso 9]><SCRIPT>alert(1)</SCRIPT><![endif]-->",
            "<!--[if mso]><iframe src=\"https://evil.example\"></iframe><![endif]-->",
            "<!--[if mso]><form action=\"https://evil.example\"><input name=\"p\"></form><![endif]-->",
        ] {
            let error = refusal(markup);
            assert!(
                error.issues.iter().any(|issue| matches!(
                    issue.kind,
                    HtmlIssueKind::ForbiddenElementInComment { .. }
                )),
                "expected a ForbiddenElementInComment issue for {markup}, got {:?}",
                error.issues
            );
        }
    }

    #[test]
    fn meta_refresh_is_rejected_however_it_is_spelled() {
        for markup in [
            r#"<meta http-equiv="refresh" content="0;url=https://evil.example">"#,
            r#"<meta HTTP-EQUIV="REFRESH" content="0">"#,
            r#"<meta http-equiv=" refresh " content="0">"#,
        ] {
            refusal(markup);
        }
    }

    #[test]
    fn every_reason_is_reported_not_just_the_first() {
        let markup = concat!(
            "<p onclick=\"a()\">x</p>\n",
            "<script>b()</script>\n",
            "<a href=\"javascript:c()\">y</a>\n",
            "<style>.d{width:expression(e())}</style>\n",
        );

        let error = refusal(markup);

        for expected in [
            "EventHandler",
            "ForbiddenElement",
            "UnsafeUrlScheme",
            "UnsafeStyle",
        ] {
            assert!(
                error
                    .issues
                    .iter()
                    .any(|issue| format!("{:?}", issue.kind).starts_with(expected)),
                "no {expected} issue among {:?}",
                error.issues
            );
        }
        assert_eq!(
            error.to_string().matches("\n  - ").count(),
            error.issues.len()
        );
    }

    /// Spelled out rather than read from `TABLE_SCOPED_ELEMENTS`, so that
    /// shrinking that list fails a test instead of quietly narrowing one.
    const DROPPED_WITHOUT_A_TABLE: &[&str] = &[
        "td", "tr", "th", "tbody", "thead", "tfoot", "caption", "col", "colgroup",
    ];

    #[test]
    fn an_event_handler_on_an_orphan_table_element_is_not_a_blind_spot() {
        for tag in DROPPED_WITHOUT_A_TABLE {
            let error = refusal(&format!(r#"<{tag} onclick="steal()">x"#));
            assert!(
                error.issues.iter().any(|issue| issue.kind
                    == HtmlIssueKind::EventHandler {
                        attribute: "onclick".to_string()
                    }),
                "orphan <{tag}> hid its handler: {error:?}"
            );
        }
    }

    #[test]
    fn an_svg_data_url_is_rejected_although_raster_data_urls_are_not() {
        for markup in [
            r#"<img src="data:image/svg+xml;base64,PHN2Zz48c2NyaXB0Pjwvc2NyaXB0Pjwvc3ZnPg==">"#,
            r#"<img src="data:image/svg+xml,%3Csvg%3E%3C/svg%3E">"#,
            r#"<img src="DATA:IMAGE/SVG+XML;base64,PHN2Zz4=">"#,
        ] {
            let rendered = refusal(markup).to_string();
            assert!(rendered.contains("svg"), "no mention of svg: {rendered}");
            assert!(
                rendered.contains("script"),
                "must say why SVG differs from other images: {rendered}"
            );
        }

        for markup in [
            r#"<img src="data:image/png;base64,iVBORw0KGgo=">"#,
            r#"<img src="data:image/jpeg;base64,/9j/4AAQ">"#,
            r#"<img src="data:image/gif;base64,R0lGOD">"#,
            r#"<img src="data:image/webp;base64,UklGRg==">"#,
        ] {
            assert_eq!(validate_html(markup), Ok(()), "wrongly refused: {markup}");
        }
    }

    #[test]
    fn svg_animation_elements_are_refused_outright() {
        for (markup, tag) in [
            (
                r#"<svg><animate attributeName="href" values="javascript:steal()"></svg>"#,
                "animate",
            ),
            (
                r#"<svg><set attributeName="href" to="javascript:steal()"></svg>"#,
                "set",
            ),
            (
                r#"<svg><animateTransform attributeName="transform"></svg>"#,
                "animatetransform",
            ),
            (r#"<svg><animateMotion></svg>"#, "animatemotion"),
        ] {
            let error = refusal(markup);
            assert!(
                error.issues.iter().any(|issue| issue.kind
                    == HtmlIssueKind::ForbiddenElement {
                        tag: tag.to_string()
                    }),
                "expected <{tag}> to be refused, got {:?}",
                error.issues
            );
        }
    }
}

#[cfg(test)]
mod budget_validation_tests {
    use super::*;

    fn refused_as_too_complex(html: &str) -> bool {
        validate_html(html).is_err_and(|error| {
            error
                .issues
                .iter()
                .any(|issue| issue.kind == HtmlIssueKind::TooComplexToCheck)
        })
    }

    fn sixty_five_inert_attributes() -> String {
        (0..65).map(|n| format!(" a{n}")).collect()
    }

    #[test]
    fn an_image_with_an_onerror_hidden_among_65_attributes_is_refused() {
        let html = format!(
            r#"<img{} src=x onerror="alert(1)">"#,
            sixty_five_inert_attributes()
        );
        assert!(refused_as_too_complex(&html));
    }
}

#[cfg(test)]
mod lost_helper_tests {
    use super::*;
    use mxr_html_worker::{with_supervisor, Config, Supervisor};
    use std::sync::Arc;

    fn lost_helper() -> Arc<Supervisor> {
        Arc::new(Supervisor::new(Config::new(
            "/nonexistent/mxr-helper",
            Vec::new(),
        )))
    }

    #[test]
    fn html_no_helper_could_check_is_refused_with_a_retry_not_a_rewrite() {
        let error = with_supervisor(lost_helper(), || validate_html("<p>fine</p>"))
            .expect_err("an unchecked document must not be sent");
        assert_eq!(
            error
                .issues
                .iter()
                .map(|issue| &issue.kind)
                .collect::<Vec<_>>(),
            vec![&HtmlIssueKind::CheckerUnavailable]
        );
        let message = error.to_string();
        assert!(message.contains("try again"), "{message}");
        assert!(!message.contains("simplify"), "{message}");
    }
}
