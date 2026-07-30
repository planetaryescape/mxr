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

use std::collections::BTreeSet;
use std::fmt;

use scraper::{Html, Node};

/// Schemes a URL-bearing attribute may use.
///
/// `data:` is handled separately: only `data:image/*` is allowed, because a
/// `data:text/html` payload is a script vector.
const ALLOWED_SCHEMES: &[&str] = &["http", "https", "mailto", "cid", "tel"];

/// Elements that execute, submit, navigate, or embed foreign content.
const FORBIDDEN_ELEMENTS: &[&str] = &[
    "script", "object", "embed", "applet", "iframe", "frame", "frameset", "form", "input",
    "button", "textarea", "select", "base",
];

/// Attributes that can carry a URL.
const URL_ATTRIBUTES: &[&str] = &[
    "href",
    "src",
    "action",
    "background",
    "poster",
    "formaction",
];

/// One reason a document was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlIssue {
    pub kind: HtmlIssueKind,
    /// 1-based line in the supplied document, when it could be located.
    pub line: Option<usize>,
    /// Short, truncated excerpt of the offending construct.
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HtmlIssueKind {
    /// An element that executes or submits, e.g. `<script>`, `<form>`.
    ForbiddenElement { tag: String },
    /// An inline event handler, e.g. `onclick`.
    EventHandler { attribute: String },
    /// A URL whose scheme is not on the allowlist.
    UnsafeUrlScheme { attribute: String, scheme: String },
    /// A `<style>` block containing an active-content construct.
    UnsafeStyle,
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
            HtmlIssueKind::UnsafeStyle => {
                write!(
                    f,
                    "<style> block contains active content{where_}: {}",
                    self.detail
                )
            }
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
/// silently rewrites to "make safe".
pub fn validate_html(html: &str) -> Result<(), HtmlValidationError> {
    let document = Html::parse_document(html);
    let mut issues = Vec::new();

    for node in document.tree.nodes() {
        match node.value() {
            Node::Element(element) => {
                let tag = element.name().to_ascii_lowercase();

                if FORBIDDEN_ELEMENTS.contains(&tag.as_str()) {
                    issues.push(HtmlIssue {
                        line: locate_line(html, &format!("<{tag}")),
                        detail: truncate(&format!("<{tag}>"), 80),
                        kind: HtmlIssueKind::ForbiddenElement { tag: tag.clone() },
                    });
                }

                if tag == "meta" && is_meta_refresh(element) {
                    issues.push(HtmlIssue {
                        line: locate_line(html, "http-equiv"),
                        detail: "<meta http-equiv=\"refresh\">".to_string(),
                        kind: HtmlIssueKind::ForbiddenElement {
                            tag: "meta http-equiv=refresh".to_string(),
                        },
                    });
                }

                issues.extend(attribute_issues(element, html));
            }
            Node::Comment(comment) => {
                // Conditional comments are markup to Outlook. Scan them with
                // the same rules rather than trusting the `<!--`.
                let body = comment.comment.as_ref();

                for tag in forbidden_tags_in(body) {
                    issues.push(HtmlIssue {
                        line: locate_line(html, &format!("<{tag}")),
                        detail: truncate(body, 80),
                        kind: HtmlIssueKind::ForbiddenElementInComment { tag },
                    });
                }

                // Outlook runs the handlers, URLs and `<style>` bodies in
                // there too, so they get the same treatment as real markup.
                // Comments inside the fragment are ignored rather than
                // recursed into — HTML comments do not nest.
                let inner = Html::parse_fragment(body);
                for node in inner.tree.nodes() {
                    if let Node::Element(element) = node.value() {
                        issues.extend(attribute_issues(element, html));
                    }
                }
                issues.extend(unsafe_style_blocks(&inner, html));
            }
            _ => {}
        }
    }

    // `<style>` is allowed — media queries are the whole point of a responsive
    // template — but its contents still get checked for active constructs.
    issues.extend(unsafe_style_blocks(&document, html));

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
            });
            continue;
        }

        if URL_ATTRIBUTES.contains(&attribute.as_str()) {
            if let Some(scheme) = unsafe_scheme(value) {
                issues.push(HtmlIssue {
                    line: locate_line(html, value.trim()),
                    detail: truncate(value, 60),
                    kind: HtmlIssueKind::UnsafeUrlScheme { attribute, scheme },
                });
            }
        }
    }

    issues
}

fn is_meta_refresh(element: &scraper::node::Element) -> bool {
    element
        .attr("http-equiv")
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("refresh"))
}

/// The scheme of `url`, when it is one we refuse.
///
/// Returns `None` for allowed schemes, `data:image/*`, and scheme-relative or
/// relative URLs.
fn unsafe_scheme(url: &str) -> Option<String> {
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
    if scheme == "data"
        && cleaned[colon + 1..]
            .to_ascii_lowercase()
            .starts_with("image/")
    {
        return None;
    }

    Some(scheme)
}

fn unsafe_style_blocks(document: &Html, html: &str) -> Vec<HtmlIssue> {
    let selector = match scraper::Selector::parse("style") {
        Ok(selector) => selector,
        Err(_) => return Vec::new(),
    };

    document
        .select(&selector)
        .filter_map(|element| {
            let css = element.text().collect::<String>();
            let lowered = css.to_ascii_lowercase();
            // `expression()` is legacy IE script-in-CSS; `javascript:` in a
            // `url()` is the same idea.
            let offender = ["expression(", "javascript:", "-moz-binding"]
                .into_iter()
                .find(|needle| lowered.contains(needle))?;
            Some(HtmlIssue {
                line: locate_line(html, offender),
                detail: truncate(offender, 40),
                kind: HtmlIssueKind::UnsafeStyle,
            })
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
}
