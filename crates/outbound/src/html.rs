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
const URL_ATTRIBUTES: &[&str] = &["href", "src", "action", "background", "poster", "formaction"];

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
                write!(f, "<{tag}> is not allowed in email HTML{where_}: {}", self.detail)
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
                write!(f, "<style> block contains active content{where_}: {}", self.detail)
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

                for (name, value) in element.attrs() {
                    let attribute = name.to_ascii_lowercase();

                    // html5ever has already decoded entities, so an obfuscated
                    // `&#111;nclick` arrives here as `onclick`.
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
            }
            Node::Comment(comment) => {
                // Conditional comments are markup to Outlook. Scan them with
                // the same element rules rather than trusting the `<!--`.
                for tag in forbidden_tags_in(comment.comment.as_ref()) {
                    issues.push(HtmlIssue {
                        line: locate_line(html, &format!("<{tag}")),
                        detail: truncate(comment.comment.as_ref(), 80),
                        kind: HtmlIssueKind::ForbiddenElementInComment { tag },
                    });
                }
            }
            _ => {}
        }
    }

    // `<style>` is allowed — media queries are the whole point of a responsive
    // template — but its contents still get checked for active constructs.
    for issue in unsafe_style_blocks(&document, html) {
        issues.push(issue);
    }

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
    if scheme == "data" && cleaned[colon + 1..].to_ascii_lowercase().starts_with("image/") {
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

    #[test]
    fn a_branded_responsive_template_passes() {
        assert_eq!(validate_html(BRANDED), Ok(()));
    }

    #[test]
    fn validation_never_mutates_the_input() {
        let original = BRANDED.to_string();
        let _ = validate_html(&original);
        assert_eq!(original, BRANDED);
    }

    #[test]
    fn rejected_documents_are_also_left_unmodified() {
        let source = r#"<p>hi</p><script>alert(1)</script>"#.to_string();
        let error = validate_html(&source).unwrap_err();
        assert!(!error.issues.is_empty());
        // The caller still holds exactly what they passed in.
        assert_eq!(source, r#"<p>hi</p><script>alert(1)</script>"#);
    }

    #[test]
    fn scripts_objects_embeds_and_forms_are_rejected() {
        for markup in [
            "<script>alert(1)</script>",
            "<object data='x.swf'></object>",
            "<embed src='x.swf'>",
            "<applet code='x'></applet>",
            "<form action='https://evil.example'><input name='p'></form>",
            "<iframe src='https://evil.example'></iframe>",
            "<base href='https://evil.example'>",
        ] {
            assert!(
                validate_html(markup).is_err(),
                "should have rejected: {markup}"
            );
        }
    }

    #[test]
    fn event_handlers_are_rejected_even_when_entity_obfuscated() {
        assert!(validate_html(r#"<p onclick="steal()">x</p>"#).is_err());
        // html5ever decodes the entity, so regex-based scanning would miss
        // this but parsing catches it.
        assert!(validate_html(r#"<p o&#110;click="steal()">x</p>"#).is_err());
    }

    #[test]
    fn javascript_urls_are_rejected_including_whitespace_obfuscation() {
        assert!(validate_html(r#"<a href="javascript:steal()">x</a>"#).is_err());
        assert!(validate_html("<a href=\"java\tscript:steal()\">x</a>").is_err());
        assert!(validate_html(r#"<a href="vbscript:steal()">x</a>"#).is_err());
    }

    #[test]
    fn safe_url_schemes_pass() {
        for markup in [
            r#"<a href="https://example.com">x</a>"#,
            r#"<a href="http://example.com">x</a>"#,
            r#"<a href="mailto:a@example.com">x</a>"#,
            r#"<a href="tel:+15550100">x</a>"#,
            r#"<img src="cid:logo">"#,
            r#"<img src="data:image/png;base64,iVBORw0KGgo=">"#,
            r#"<a href="/relative/path">x</a>"#,
            r##"<a href="#anchor">x</a>"##,
        ] {
            assert_eq!(validate_html(markup), Ok(()), "should have allowed: {markup}");
        }
    }

    #[test]
    fn data_urls_are_allowed_only_for_images() {
        assert_eq!(
            validate_html(r#"<img src="data:image/gif;base64,R0lGOD">"#),
            Ok(())
        );
        assert!(validate_html(r#"<a href="data:text/html,<script>x</script>">y</a>"#).is_err());
    }

    #[test]
    fn style_blocks_and_media_queries_are_preserved_but_active_css_is_rejected() {
        assert_eq!(
            validate_html("<style>@media (max-width:600px){.a{width:100%}}</style>"),
            Ok(())
        );
        assert!(validate_html("<style>.a{width:expression(alert(1))}</style>").is_err());
        assert!(validate_html("<style>.a{background:url(javascript:alert(1))}</style>").is_err());
    }

    #[test]
    fn a_script_hidden_in_a_conditional_comment_is_not_a_blind_spot() {
        let markup = "<!--[if mso]><script>alert(1)</script><![endif]-->";
        let error = validate_html(markup).unwrap_err();
        assert!(matches!(
            error.issues[0].kind,
            HtmlIssueKind::ForbiddenElementInComment { .. }
        ));
    }

    #[test]
    fn benign_conditional_comments_are_not_flagged() {
        let markup =
            "<!--[if mso]><style>.fallback{font-family:Arial}</style><![endif]--><p>hi</p>";
        assert_eq!(validate_html(markup), Ok(()));
    }

    #[test]
    fn meta_refresh_is_rejected_but_charset_meta_is_fine() {
        assert!(validate_html(r#"<meta http-equiv="refresh" content="0;url=https://x">"#).is_err());
        assert_eq!(validate_html(r#"<meta charset="utf-8">"#), Ok(()));
    }

    #[test]
    fn issues_report_a_line_number_and_do_not_leak_the_whole_document() {
        let markup = "<html>\n<body>\n<script>alert(1)</script>\n</body>\n</html>";
        let error = validate_html(markup).unwrap_err();
        assert_eq!(error.issues[0].line, Some(3));
        assert!(error.issues[0].detail.len() <= 81);
    }

    #[test]
    fn the_error_message_says_the_html_was_not_touched() {
        let error = validate_html("<script>x</script>").unwrap_err();
        assert!(error.to_string().contains("was not modified"));
    }

    #[test]
    fn generated_text_alternative_is_deterministic_and_readable() {
        let html = "<h1>Digest</h1><p>Hi Dumi — the report is ready.</p>";
        let first = generate_text_alternative(html);
        assert_eq!(first, generate_text_alternative(html));
        assert!(first.contains("Digest"));
        assert!(first.contains("Dumi"));
    }

    #[test]
    fn generating_the_text_alternative_does_not_touch_the_html() {
        let html = BRANDED.to_string();
        let _ = generate_text_alternative(&html);
        assert_eq!(html, BRANDED);
    }

    #[test]
    fn unicode_and_registered_marks_survive_validation() {
        let markup = "<p>Notto® — café, naïve, 日本語, 🎉</p>";
        assert_eq!(validate_html(markup), Ok(()));
    }
}
