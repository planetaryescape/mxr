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
use scraper::{Html, Node};

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
#[derive(Debug, Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IssueOrigin {
    /// The document itself.
    Document,
    /// The body of the nth comment, counted in document order.
    Comment(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
}

impl fmt::Display for HtmlIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let where_ = match self.line {
            Some(line) => format!(" (line {line})"),
            None => String::new(),
        };
        match &self.kind {
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
                // the same rules rather than trusting the `<!--`.
                let body = comment.comment.as_ref();

                // Collected apart from `issues` so the orphan scan below
                // deduplicates against this comment's own findings and nothing
                // else. Deduplicating against the whole document would let an
                // identical offence elsewhere suppress this one.
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
                // Comments inside the fragment are ignored rather than
                // recursed into — HTML comments do not nest.
                let inner = Html::parse_fragment(body);
                for node in inner.tree.nodes() {
                    if let Node::Element(element) = node.value() {
                        found.extend(attribute_issues(element, html));
                    }
                }
                found.extend(unsafe_style_blocks(&inner, html));
                let orphans = orphan_table_element_issues(body, html, &found);
                found.extend(orphans);

                // Stamped last, so the scans above compared like with like.
                for issue in &mut found {
                    issue.origin = IssueOrigin::Comment(comments_seen);
                }
                comments_seen += 1;
                issues.extend(found);
            }
            _ => {}
        }
    }

    // `<style>` is allowed — media queries are the whole point of a responsive
    // template — but its contents still get checked for active constructs.
    issues.extend(unsafe_style_blocks(&document, html));

    // Last, because it needs to know what the document parse already found.
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
    html2text::config::plain_no_decorate()
        .string_from_read(html.as_bytes(), 80)
        .unwrap_or_default()
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
                // Callers scanning a comment body overwrite this with the
                // comment's index; the document scan leaves it as-is.
                origin: IssueOrigin::Document,
            });
            continue;
        }

        if attribute == "style" {
            issues.extend(css_issues(value, html, HtmlIssueKind::UnsafeStyleAttribute));
            continue;
        }

        // `srcset` and `imagesrcset` hold a list of candidates, so each one is
        // a URL and the value as a whole is not.
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
            // Only whitespace may sit between the name and the `>`, so
            // `</tablet>` is a different tag and stays.
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
        // Not a scheme at all — a colon inside a relative path or query.
        return None;
    }
    // A `/`, `?` or `#` before the colon means the colon belongs to the path.
    if cleaned[..colon].contains(['/', '?', '#']) {
        return None;
    }

    let scheme = scheme.to_ascii_lowercase();

    if ALLOWED_SCHEMES.contains(&scheme.as_str()) {
        return None;
    }
    if scheme == "data" {
        // Not lowercased: base64 is case-sensitive, and this is the payload.
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
        // No comma means no payload: not a usable data URL, nothing to sniff.
        return false;
    };

    // The base64 flag is the last parameter. Whitespace is already gone.
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
    // Four base64 characters carry three bytes.
    let wanted = DATA_URL_SNIFF_BYTES.div_ceil(3) * 4;
    // Sliced as bytes: a payload may hold arbitrary UTF-8, and a character
    // boundary is not something to trip over while looking at a prefix.
    let bytes = payload.as_bytes();
    let mut prefix = &bytes[..bytes.len().min(wanted)];

    // Padding is only ever final, so anything from the first `=` is the tail.
    if let Some(pad) = prefix.iter().position(|byte| *byte == b'=') {
        prefix = &prefix[..pad];
    }
    // A lone trailing character is not a whole quantum. Dropping it costs
    // nothing: the byte it half-encodes lies beyond the prefix anyway.
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
            // `<script` or `<script ` / `<script>`, not `<scripted`.
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
    use super::*;

    /// The whole point of the feature: a designed email survives untouched.
    const BRANDED: &str = r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  @media only screen and (max-width: 600px) {
    .container { width: 100% !important; }
  }
</style>
<!--[if mso]>
<style>.fallback { font-family: Arial, sans-serif; }</style>
<![endif]-->
</head>
<body>
<table class="container" role="presentation" cellpadding="0" cellspacing="0" width="600">
  <tr><td style="padding:24px;font-family:Georgia,serif;color:#1a1a1a;">
    <img src="cid:notto-logo" alt="Notto®" width="120">
    <p>Hi Dumi — the Notto® digest is ready.</p>
    <a href="https://example.com/read">Read it</a>
  </td></tr>
</table>
</body>
</html>"#;

    /// Refuse `markup`, or fail with a message naming what got through.
    fn refusal(markup: &str) -> HtmlValidationError {
        match validate_html(markup) {
            Err(error) => error,
            Ok(()) => panic!("should have been refused: {markup}"),
        }
    }

    #[test]
    fn a_branded_responsive_template_passes() {
        assert_eq!(validate_html(BRANDED), Ok(()));
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
    fn a_refused_document_is_handed_back_intact_for_the_caller_to_fix() {
        // Refusing rather than sanitising is the contract: the caller still
        // holds their exact source and can go fix it.
        let source = String::from("<p>keep me</p>\n<script>alert(1)</script>\n<p>®</p>");
        let before = source.clone();

        let error = validate_html(&source).unwrap_err();

        assert!(!error.issues.is_empty());
        assert_eq!(source, before);
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
    fn a_frameset_document_is_rejected() {
        refusal("<html><frameset><frame src=\"https://evil.example\"></frameset></html>");
    }

    #[test]
    fn tag_case_does_not_hide_a_forbidden_element() {
        for markup in [
            "<SCRIPT>alert(1)</SCRIPT>",
            "<ScRiPt>alert(1)</ScRiPt>",
            "<IFRAME src='https://evil.example'></IFRAME>",
        ] {
            refusal(markup);
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
    fn data_urls_are_allowed_only_for_images() {
        assert_eq!(
            validate_html(r#"<img src="data:image/jpeg;base64,/9j/4AA">"#),
            Ok(())
        );
        for markup in [
            r#"<a href="data:text/html,<b>x</b>">y</a>"#,
            r#"<a href="DATA:TEXT/HTML,x">y</a>"#,
            r#"<a href="data:application/javascript,steal()">y</a>"#,
            r#"<a href="data:,plain">y</a>"#,
        ] {
            refusal(markup);
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
    fn an_event_handler_hidden_in_a_conditional_comment_is_rejected() {
        // Outlook renders what is inside `[if mso]`, so a handler in there is
        // a live handler and not inert comment text.
        refusal(r#"<!--[if mso]><table><tr><td onclick="steal()">x</td></tr></table><![endif]-->"#);
    }

    #[test]
    fn an_unsafe_url_hidden_in_a_conditional_comment_is_rejected() {
        refusal(r#"<!--[if mso]><a href="javascript:steal()">x</a><![endif]-->"#);
    }

    #[test]
    fn active_css_hidden_in_a_conditional_comment_is_rejected() {
        refusal("<!--[if mso]><style>.a{width:expression(alert(1))}</style><![endif]-->");
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
        // Every issue reaches the message the user actually sees.
        assert_eq!(
            error.to_string().matches("\n  - ").count(),
            error.issues.len()
        );
    }

    #[test]
    fn an_issue_points_at_a_line_and_does_not_echo_the_whole_document() {
        let filler = "<p>padding</p>\n".repeat(20);
        let markup = format!("<html>\n<body>\n{filler}<script>alert(1)</script>\n</body>\n</html>");

        let error = refusal(&markup);

        assert_eq!(error.issues[0].line, Some(23));
        assert!(
            !error.issues[0].detail.contains("padding"),
            "the excerpt leaked the surrounding document: {:?}",
            error.issues[0].detail
        );
    }

    #[test]
    fn a_long_offending_value_is_truncated_in_the_excerpt() {
        let href = format!("javascript:{}", "a".repeat(500));

        let error = refusal(&format!(r#"<a href="{href}">x</a>"#));

        let detail = &error.issues[0].detail;
        assert!(
            detail.chars().count() <= 64,
            "excerpt was not truncated: {} chars",
            detail.chars().count()
        );
        assert!(
            detail.starts_with("javascript:"),
            "excerpt lost the useful part: {detail}"
        );
    }

    #[test]
    fn the_error_says_the_html_was_left_alone() {
        let rendered = refusal("<script>x</script>").to_string();
        assert!(rendered.contains("was not modified"), "{rendered}");
        assert!(rendered.contains("script"), "{rendered}");
    }

    #[test]
    fn the_generated_text_alternative_is_deterministic_readable_and_markup_free() {
        let html = r#"<h1>Digest</h1><p>Hi Dumi — the report is ready.</p><a href="https://x">Read it</a>"#;

        let text = generate_text_alternative(html);

        assert_eq!(
            text,
            generate_text_alternative(html),
            "generation is not deterministic"
        );
        assert!(text.contains("Digest"), "{text:?}");
        assert!(text.contains("Hi Dumi"), "{text:?}");
        assert!(text.contains("Read it"), "{text:?}");
        assert!(
            !text.contains('<'),
            "markup leaked into the text alternative: {text:?}"
        );
    }

    #[test]
    fn generating_a_text_alternative_from_nothing_yields_nothing() {
        for html in ["", "<p></p>", "<style>.a{color:red}</style>"] {
            assert!(
                generate_text_alternative(html).trim().is_empty(),
                "expected no visible text from {html:?}"
            );
        }
    }

    #[test]
    fn an_entity_in_an_attribute_name_is_inert_but_a_real_handler_is_not() {
        // No HTML parser decodes character references in attribute *names*
        // (html5ever's attribute-name state has no character-reference path),
        // so `o&#110;click` is an inert attribute rather than a disguised
        // handler, and refusing it would refuse valid markup. Values are a
        // different matter, and are covered above.
        assert_eq!(validate_html(r#"<p o&#110;click="steal()">x</p>"#), Ok(()));
        refusal(r#"<p onclick="steal()">x</p>"#);
    }

    /// Spelled out rather than read from `TABLE_SCOPED_ELEMENTS`, so that
    /// shrinking that list fails a test instead of quietly narrowing one.
    const DROPPED_WITHOUT_A_TABLE: &[&str] = &[
        "td", "tr", "th", "tbody", "thead", "tfoot", "caption", "col", "colgroup",
    ];

    /// Why the second parse exists at all. If html5ever ever starts keeping
    /// these, or starts dropping something new, this is the test that says so.
    #[test]
    fn only_table_scoped_elements_vanish_from_a_document_parse() {
        fn survives(tag: &str) -> bool {
            let document = Html::parse_document(&format!("<{tag} data-probe=\"1\">x"));
            document
                .tree
                .nodes()
                .filter_map(|node| node.value().as_element())
                .any(|element| element.name() == tag)
        }

        assert_eq!(
            TABLE_SCOPED_ELEMENTS, DROPPED_WITHOUT_A_TABLE,
            "the validator's list drifted from what html5ever actually drops"
        );
        for tag in DROPPED_WITHOUT_A_TABLE {
            assert!(!survives(tag), "<{tag}> unexpectedly survived");
        }
        for tag in [
            "li", "option", "optgroup", "dt", "dd", "legend", "rt", "source",
        ] {
            assert!(survives(tag), "<{tag}> is dropped too and needs covering");
        }
    }

    /// A realistic branded template whose cells sit in a real table. Nothing
    /// here may ever be refused.
    const BRANDED_TABLE_CELLS: &str = r#"<!DOCTYPE html>
<html>
<body>
<table role="presentation" cellpadding="0" cellspacing="0" width="600">
  <caption style="caption-side:top;font-size:11px;">Notto® weekly</caption>
  <!-- The url() forms a real branded template actually uses. -->
  <colgroup><col style="background-image:url(&quot;https://cdn.example.com/rule.gif&quot;)"></colgroup>
  <colgroup><col style="width:600px"></colgroup>
  <thead><tr><th style="text-align:left;font-family:Georgia,serif;">Notto® digest</th></tr></thead>
  <tbody>
    <tr><td style="padding:24px;color:#1a1a1a;background:#fff;mso-line-height-rule:exactly;">
      <a href="https://example.com/read" style="color:#0b57d0;">Read it</a>
    </td></tr>
    <tr><td style="background:url(https://cdn.example.com/hero.png) no-repeat center,url('cid:notto-texture');list-style-image:url(data:image/png;base64,iVBORw0KGgo=);">
      <img src="cid:notto-logo" alt="Notto®" width="120"
           srcset="https://cdn.example.com/logo.png 1x, https://cdn.example.com/logo@2x.png 2x">
    </td></tr>
    <!-- An inlined raster logo, base64 wrapped across lines as a real
         template writes it. -->
    <tr><td><img alt="Notto®" width="1" src="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAAB
      CAYAAAAfFcSJAAAADUlEQVR4nGNgYGBgAAAABQABV81f1QAAAABJRU5ErkJggg=="></td></tr>
  </tbody>
  <tfoot><tr><td style="font-size:12px;color:#666;">Unsubscribe any time.</td></tr></tfoot>
</table>
</body>
</html>"#;

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
    fn an_unsafe_url_on_an_orphan_table_element_is_not_a_blind_spot() {
        let error = refusal(r#"<td background="javascript:steal()">x</td>"#);
        assert!(
            error.issues.iter().any(|issue| matches!(
                &issue.kind,
                HtmlIssueKind::UnsafeUrlScheme { scheme, .. } if scheme == "javascript"
            )),
            "{error:?}"
        );
    }

    /// Held before the orphan scan existed and must keep holding: only the
    /// `<td>` token is dropped, so the `<a>` beneath it is reparented into the
    /// body and inspected there.
    #[test]
    fn an_unsafe_url_below_an_orphan_table_element_is_caught() {
        let error = refusal(r#"<td><a href="javascript:steal()">x</a></td>"#);
        assert!(
            error.issues.iter().any(|issue| matches!(
                &issue.kind,
                HtmlIssueKind::UnsafeUrlScheme { scheme, .. } if scheme == "javascript"
            )),
            "{error:?}"
        );
    }

    #[test]
    fn a_stray_table_end_tag_does_not_hide_the_orphan_that_follows_it() {
        // The second parse wraps a copy in `<table>`; an unmatched `</table>`
        // in the source would otherwise close that wrapper.
        for markup in [
            r#"<div></table><td onclick="steal()">x"#,
            r#"<div></TABLE><td onclick="steal()">x"#,
            r#"<div></table  ><td onclick="steal()">x"#,
        ] {
            refusal(markup);
        }
    }

    #[test]
    fn a_handler_on_an_orphan_cell_inside_a_conditional_comment_is_rejected() {
        refusal(r#"<!--[if mso]><td onclick="steal()">x<![endif]-->"#);
    }

    #[test]
    fn a_real_table_cell_is_not_mistaken_for_an_orphan() {
        assert_eq!(validate_html(BRANDED_TABLE_CELLS), Ok(()));
    }

    #[test]
    fn a_real_table_cell_with_a_handler_is_reported_once_not_twice() {
        let error =
            refusal(r#"<table><tr><td onclick="steal()" style="padding:8px">x</td></tr></table>"#);
        assert_eq!(error.issues.len(), 1, "{error:?}");
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

    /// Inline CSS is the dominant form in email, because most mail clients
    /// strip `<style>` blocks. Checking blocks but not attributes left the
    /// rules absent from exactly the place email puts its CSS.
    #[test]
    fn active_css_in_an_inline_style_attribute_is_rejected() {
        for markup in [
            r#"<td style="width:expression(steal())">x</td>"#,
            r#"<p style="background:url(javascript:steal())">x</p>"#,
            r#"<p style="-moz-binding:url(evil.xml#x)">x</p>"#,
            r#"<p STYLE="WIDTH:EXPRESSION(steal())">x</p>"#,
            r#"<table><tr><td style="width:expression(steal())">x</td></tr></table>"#,
            r#"<!--[if mso]><p style="width:expression(steal())">x</p><![endif]-->"#,
        ] {
            refusal(markup);
        }
    }

    /// The defect was drift between the block rules and the attribute rules,
    /// so assert the two agree rather than that each works alone.
    #[test]
    fn a_block_and_an_attribute_reach_the_same_verdict_on_the_same_css() {
        for css in [
            "width:expression(steal())",
            "background:url(javascript:steal())",
            "-moz-binding:url(evil.xml#x)",
        ] {
            assert!(
                validate_html(&format!("<style>.a{{{css}}}</style>")).is_err(),
                "block allowed {css}"
            );
            assert!(
                validate_html(&format!(r#"<p style="{css}">x</p>"#)).is_err(),
                "attribute allowed {css}"
            );
        }
        for css in [
            "background:url(https://cdn.example.com/bg.png)",
            "mso-line-height-rule:exactly",
            "padding:24px;color:#1a1a1a",
        ] {
            assert_eq!(
                validate_html(&format!("<style>.a{{{css}}}</style>")),
                Ok(()),
                "block refused {css}"
            );
            assert_eq!(
                validate_html(&format!(r#"<p style="{css}">x</p>"#)),
                Ok(()),
                "attribute refused {css}"
            );
        }
    }

    /// The round-1 SVG refusal covered `src` and `href` only, so CSS was a way
    /// straight round it. Blocks and attributes are asserted together, because
    /// a rule living in one and not the other is the defect being fixed.
    #[test]
    fn a_url_in_css_is_held_to_the_same_scheme_allowlist_as_an_attribute() {
        for css in [
            "background:url(data:image/svg+xml;base64,PHN2Zz4=)",
            "background:url('data:image/svg+xml;base64,PHN2Zz4=')",
            "background:url( data:image/svg+xml;base64,PHN2Zz4= )",
            "background:url(vbscript:steal())",
            "background:url(file:///etc/passwd)",
            "background:url(a.png),url(data:image/svg+xml;base64,PHN2Zz4=)",
        ] {
            assert!(
                validate_html(&format!("<style>.a{{{css}}}</style>")).is_err(),
                "block allowed {css}"
            );
            assert!(
                validate_html(&format!(r#"<p style="{css}">x</p>"#)).is_err(),
                "attribute allowed {css}"
            );
        }

        // Double quotes cannot survive inside a double-quoted attribute, so the
        // two forms are checked where each is actually written.
        refusal(r#"<style>.a{background:url("data:image/svg+xml;base64,PHN2Zz4=")}</style>"#);
        refusal(
            r#"<p style="background:url(&quot;data:image/svg+xml;base64,PHN2Zz4=&quot;)">x</p>"#,
        );

        for css in [
            "background:url(https://cdn.example.com/bg.png)",
            "background:url(cid:notto-logo)",
            "background:url('cid:notto-logo')",
            "background:url(data:image/png;base64,iVBORw0KGgo=)",
            "background:url(images/bg.png),url(images/bg@2x.png)",
            "background:url()",
            "list-style:none",
        ] {
            assert_eq!(
                validate_html(&format!("<style>.a{{{css}}}</style>")),
                Ok(()),
                "block refused {css}"
            );
            assert_eq!(
                validate_html(&format!(r#"<p style="{css}">x</p>"#)),
                Ok(()),
                "attribute refused {css}"
            );
        }
    }

    #[test]
    fn an_svg_data_url_in_css_gets_the_same_explanation_as_one_in_an_attribute() {
        for markup in [
            r#"<p style="background:url(data:image/svg+xml;base64,PHN2Zz4=)">x</p>"#,
            r#"<style>.a{background:url(data:image/svg+xml;base64,PHN2Zz4=)}</style>"#,
        ] {
            let rendered = refusal(markup).to_string();
            assert!(rendered.contains("svg"), "no mention of svg: {rendered}");
            assert!(
                rendered.contains("script"),
                "must say why SVG differs from other images: {rendered}"
            );
        }
    }

    #[test]
    fn an_inline_style_in_a_real_table_cell_is_reported_once_not_twice() {
        let error =
            refusal(r#"<table><tr><td style="width:expression(steal())">x</td></tr></table>"#);
        assert_eq!(error.issues.len(), 1, "{error:?}");
    }

    #[test]
    fn every_url_bearing_attribute_is_checked_not_just_href_and_src() {
        for markup in [
            r#"<img longdesc="javascript:steal()">"#,
            r#"<blockquote cite="javascript:steal()">x</blockquote>"#,
            r#"<img usemap="javascript:steal()">"#,
            r#"<img dynsrc="javascript:steal()">"#,
            r#"<img lowsrc="javascript:steal()">"#,
            r#"<svg><a xlink:href="javascript:steal()">x</a></svg>"#,
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
    fn each_candidate_in_a_srcset_is_checked_not_the_whole_value() {
        // `srcset` is a comma-separated list of `url [descriptor]` candidates.
        // Scheme-checking the whole value finds nothing at all in the second
        // case: the comma disqualifies everything before the colon.
        for markup in [
            r#"<img srcset="javascript:steal() 1x">"#,
            r#"<img srcset="hero.png 1x, javascript:steal() 2x">"#,
            r#"<link imagesrcset="hero.png 1x, javascript:steal() 2x">"#,
        ] {
            let error = refusal(markup);
            assert!(
                error.issues.iter().any(|issue| matches!(
                    &issue.kind,
                    HtmlIssueKind::UnsafeUrlScheme { scheme, .. } if scheme == "javascript"
                )),
                "{error:?}"
            );
        }
        assert_eq!(
            validate_html(
                r#"<img srcset="hero.png 1x, hero@2x.png 2x, https://cdn.example.com/h.png 3x">"#
            ),
            Ok(())
        );
    }

    #[test]
    fn svg_animation_elements_are_refused_outright() {
        // `<animate attributeName="href" values="javascript:...">` reaches
        // script through indirection no attribute-by-attribute rule can follow,
        // and animation has no legitimate use in a mail template.
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

    #[test]
    fn foreign_object_is_refused_outright() {
        // `<foreignObject>` embeds arbitrary XHTML inside `<svg>`. Refusing the
        // element beats reasoning about what it carries, exactly as with the
        // SVG animation elements.
        for markup in [
            "<svg><foreignObject><p>x</p></foreignObject></svg>",
            "<svg><foreignobject><p>x</p></foreignobject></svg>",
            "<svg><FOREIGNOBJECT></FOREIGNOBJECT></svg>",
        ] {
            let error = refusal(markup);
            assert!(
                error.issues.iter().any(|issue| issue.kind
                    == HtmlIssueKind::ForbiddenElement {
                        tag: "foreignobject".to_string()
                    }),
                "expected <foreignObject> to be refused, got {:?}",
                error.issues
            );
        }
    }

    #[test]
    fn foreign_object_inside_a_conditional_comment_is_refused_too() {
        let error =
            refusal("<!--[if mso]><svg><foreignObject><p>x</p></foreignObject></svg><![endif]-->");
        assert!(
            error.issues.iter().any(|issue| issue.kind
                == HtmlIssueKind::ForbiddenElementInComment {
                    tag: "foreignobject".to_string()
                }),
            "{error:?}"
        );
    }

    /// Claimed already covered, because html5ever adjusts foreign attributes
    /// and scraper yields only the local name, so SVG's `xlink:href` arrives as
    /// `href`. Asserted rather than inherited.
    #[test]
    fn an_svg_use_reference_is_held_to_the_scheme_allowlist() {
        for markup in [
            r#"<svg><use href="javascript:steal()"></use></svg>"#,
            r#"<svg><use xlink:href="javascript:steal()"></use></svg>"#,
        ] {
            let error = refusal(markup);
            assert!(
                error.issues.iter().any(|issue| matches!(
                    &issue.kind,
                    HtmlIssueKind::UnsafeUrlScheme { attribute, scheme }
                        if attribute == "href" && scheme == "javascript"
                )),
                "expected <use> to be checked, got {:?}",
                error.issues
            );
        }
    }

    /// The scheme check reads the declared media type and stops. A client that
    /// content-sniffs reads the payload, so the payload is what decides:
    /// `data:image/png;base64,<an SVG document>` is an SVG, and SVG carries
    /// `<script>`.
    #[test]
    fn a_data_image_url_whose_payload_is_markup_is_refused_whatever_it_claims_to_be() {
        for markup in [
            // <svg xmlns="..."><script>steal()</script></svg>
            r#"<img src="data:image/png;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciPjxzY3JpcHQ+c3RlYWwoKTwvc2NyaXB0Pjwvc3ZnPg==">"#,
            // <?xml version="1.0"?><svg/>
            r#"<img src="data:image/gif;base64,PD94bWwgdmVyc2lvbj0iMS4wIj8+PHN2Zy8+">"#,
            // <!DOCTYPE html><html></html>
            r#"<img src="data:image/jpeg;base64,PCFET0NUWVBFIGh0bWw+PGh0bWw+PC9odG1sPg==">"#,
            // <html><body>x</body></html>
            r#"<img src="data:image/webp;base64,PGh0bWw+PGJvZHk+eDwvYm9keT48L2h0bWw+">"#,
            // "   \n <svg/>" — whitespace before the signature, which a
            // sniffing client skips.
            r#"<img src="data:image/png;base64,ICAgCiA8c3ZnLz4=">"#,
            // Percent-encoded rather than base64.
            r#"<img src="data:image/png,%3Csvg%2F%3E">"#,
            r#"<img src="data:image/png,%20%20%3Csvg%2F%3E">"#,
            // The same dodge routed through CSS.
            r#"<p style="background:url(data:image/png;base64,PHN2Zz4=)">x</p>"#,
            r#"<style>.a{background:url(data:image/png;base64,PHN2Zz4=)}</style>"#,
            // And hidden in a conditional comment.
            r#"<!--[if mso]><img src="data:image/png;base64,PHN2Zz4="><![endif]-->"#,
        ] {
            refusal(markup);
        }
    }

    #[test]
    fn a_mislabelled_data_url_says_the_payload_disagrees_with_the_declared_type() {
        let rendered = refusal(r#"<img src="data:image/png;base64,PHN2Zz4=">"#).to_string();
        assert!(rendered.contains("image/png"), "{rendered}");
        assert!(rendered.contains("markup"), "{rendered}");
    }

    /// The false-positive side of the payload check, which matters more than
    /// the true-positive side: real templates inline raster logos.
    #[test]
    fn a_raster_data_url_still_passes_and_an_unreadable_payload_is_not_refused() {
        for markup in [
            // A real 1x1 PNG, wrapped across lines the way a template does it.
            "<img src=\"data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAAB\n  CAYAAAAfFcSJAAAADUlEQVR4nGNgYGBgAAAABQABV81f1QAAAABJRU5ErkJggg==\">",
            r#"<img src="data:image/jpeg;base64,/9j/4AAQ">"#,
            r#"<img src="data:image/gif;base64,R0lGOD">"#,
            r#"<img src="data:image/webp;base64,UklGRg==">"#,
            // Formats the validator knows no signature for. Allowed: it is only
            // looking for markup, not vouching for the declared type.
            r#"<img src="data:image/avif;base64,AAAAIGZ0eXBhdmlm">"#,
            r#"<img src="data:image/x-icon;base64,AAABAAEAEBA=">"#,
            // Undecodable, empty and comma-less payloads: nothing to sniff, and
            // refusing what cannot be read would refuse legitimate mail.
            r#"<img src="data:image/png;base64,****">"#,
            r#"<img src="data:image/png;base64,">"#,
            r#"<img src="data:image/png">"#,
        ] {
            assert_eq!(validate_html(markup), Ok(()), "wrongly refused: {markup}");
        }
    }

    /// Identical markup written both inside a conditional comment and outside
    /// it is two problems. Comparing whole issues collapsed them into one,
    /// because both carry the same value and the same first-occurrence line.
    #[test]
    fn the_same_offence_in_a_comment_and_in_the_document_is_reported_twice() {
        let error =
            refusal(r#"<!--[if mso]><td onclick="steal()">x<![endif]--><td onclick="steal()">y"#);
        assert_eq!(error.issues.len(), 2, "{error:?}");
    }

    /// The suppression exists so a real table cell is not reported by both the
    /// document parse and the orphan reparse. It has to subtract one copy, not
    /// every copy: a second, genuinely orphaned cell on the same line is a
    /// second problem.
    #[test]
    fn a_real_table_cell_does_not_mask_an_identical_orphan_beside_it() {
        let error = refusal(
            r#"<table><tr><td onclick="steal()">x</td></tr></table><td onclick="steal()">y"#,
        );
        assert_eq!(error.issues.len(), 2, "{error:?}");
    }

    #[test]
    fn the_newly_covered_refusals_still_hand_back_the_exact_source() {
        for source in [
            String::from("<td onclick=\"steal()\">®</td>\n"),
            String::from("<img src=\"data:image/svg+xml;base64,PHN2Zz4=\" alt=\"®\">\n"),
            String::from("<p style=\"width:expression(steal())\">®</p>\n"),
            String::from("<img srcset=\"a.png 1x, javascript:steal() 2x\" alt=\"®\">\n"),
            String::from("<svg><animate attributeName=\"href\"></svg>\n"),
            String::from("<p style=\"background:url(data:image/svg+xml;base64,PHN2Zz4=)\">®</p>\n"),
        ] {
            let before = source.clone();
            assert!(validate_html(&source).is_err(), "{source}");
            assert_eq!(source, before);
        }
    }
}
