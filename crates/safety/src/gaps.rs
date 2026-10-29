//! Unfilled-gap check (blueprint 34, D159).
//!
//! Writing help marks what only the user can decide as `[[?: the date]]`.
//! Nobody means to send one, and filling it is one edit, so a draft that still
//! has one cannot be sent. It is a blocker with no override: someone who
//! writes `[[?:` on purpose can't send it either, which the decision accepts.

use mxr_core::types::{
    Draft, DraftContent, DraftSafetyIssue, DraftSafetyIssueCode, DraftSafetySeverity,
};

const OPEN: &str = "[[?:";

pub fn check(draft: &Draft) -> Vec<DraftSafetyIssue> {
    // What is sent, not what was typed: the recipient reads the HTML part, and
    // `&#91;&#91;?: …]]`, a marker split by tags or one broken across lines by
    // a renderer is a gap although no serialized part holds the literal
    // `[[?:`. Quoted text and the signature count too, since they are sent.
    let rendered_markdown = match &draft.content {
        DraftContent::Markdown { source } => {
            Some(mxr_outbound::render::render_markdown(source).html)
        }
        DraftContent::Html { .. } => None,
    };
    let html = match (&draft.content, &rendered_markdown) {
        (DraftContent::Html { html, .. }, _) => html.as_str(),
        (_, Some(rendered)) => rendered.as_str(),
        (DraftContent::Markdown { .. }, None) => "",
    };
    // Over the parser budget the HTML cannot be read, so the gap check cannot
    // run. That is a blocker with no override: a gap hidden in such HTML must
    // not be sent.
    if !mxr_reader::within_budget(html) {
        return vec![DraftSafetyIssue::new(
            DraftSafetyIssueCode::UnfilledGap,
            DraftSafetySeverity::Blocker,
            "HTML is too complex to check for unfilled gaps, so it cannot be sent: simplify \
             it (fewer tags, attributes or table cells) and try again",
        )];
    }
    let body_views: Vec<String> = match &draft.content {
        DraftContent::Markdown { source } => {
            vec![source.clone(), visible_text(html), attribute_text(html)]
        }
        DraftContent::Html { text, .. } => vec![
            html.to_string(),
            text.clone().unwrap_or_default(),
            visible_text(html),
            attribute_text(html),
        ],
    };
    let mut labels = labels_in(std::slice::from_ref(&draft.subject));
    labels.extend(labels_in(&body_views));
    if labels.is_empty() {
        return Vec::new();
    }
    let count = labels.len();
    let noun = if count == 1 { "gap" } else { "gaps" };
    vec![DraftSafetyIssue::new(
        DraftSafetyIssueCode::UnfilledGap,
        DraftSafetySeverity::Blocker,
        format!("{count} {noun} left: {}", labels.join(", ")),
    )]
}

/// The gaps one piece of the draft holds, seen through each of its `views`.
/// The plain scan names them. The decisive count also looks with all
/// whitespace removed, so a marker split by a space, a line wrap or a tag still
/// counts; one the plain scan could not read is an unnamed gap.
fn labels_in(views: &[String]) -> Vec<String> {
    let mut labels = views
        .iter()
        .map(|view| gap_labels(view))
        .max_by_key(Vec::len)
        .unwrap_or_default();
    let squashed = views
        .iter()
        .map(|view| squash(view).matches(OPEN).count())
        .max()
        .unwrap_or(0);
    for _ in labels.len()..squashed {
        labels.push("an unnamed gap".to_string());
    }
    labels
}

fn squash(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Every attribute value of an HTML document with its entities decoded, which
/// is how an image's `alt` or a link's `title` reaches a recipient. Values are
/// joined by a mark that is not whitespace, so the whitespace-blind scan never
/// joins the end of one value to the start of the next.
fn attribute_text(html: &str) -> String {
    // Over the parser budget the raw markup stands in: attribute values stay
    // inside it, so the scan still sees them, just unparsed.
    if !mxr_reader::within_budget(html) {
        return html.to_string();
    }
    scraper::Html::parse_document(html)
        .tree
        .nodes()
        .filter_map(|node| node.value().as_element())
        .flat_map(|element| element.attrs().map(|(_, value)| value))
        .collect::<Vec<_>>()
        .join("\u{1}")
}

/// The text of an HTML document as a reader sees it: entities decoded and tags
/// dropped, with no line wrapping (the reader's renderer wraps at 80 columns,
/// which can split a marker in two).
fn visible_text(html: &str) -> String {
    // Over the parser budget, the linear tag strip stands in for the document.
    if !mxr_reader::within_budget(html) {
        return mxr_reader::strip_html_tags(html);
    }
    scraper::Html::parse_document(html)
        .root_element()
        .text()
        .collect()
}

/// The label of each `[[?: …]]` in `text`, in order. A marker left open ends
/// where the next one starts or at the end of its line, whichever comes first.
/// An empty label is a gap all the same.
fn gap_labels(text: &str) -> Vec<String> {
    let mut labels = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find(OPEN) {
        let after = &rest[at + OPEN.len()..];
        let close = after.find("]]");
        let reopened = after.find(OPEN);
        let (inside, next) = match close {
            Some(close) if reopened.is_none_or(|reopened| close < reopened) => {
                (&after[..close], &after[close + 2..])
            }
            _ => {
                let stop = [reopened, after.find('\n')]
                    .into_iter()
                    .flatten()
                    .min()
                    .unwrap_or(after.len());
                (&after[..stop], &after[stop..])
            }
        };
        let label = inside.split_whitespace().collect::<Vec<_>>().join(" ");
        labels.push(if label.is_empty() {
            "an unnamed gap".to_string()
        } else {
            label
        });
        rest = next;
    }
    labels
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::id::{AccountId, DraftId};
    use mxr_core::types::{Address, DraftIntent};

    fn draft(subject: &str, content: DraftContent) -> Draft {
        Draft {
            revision: None,
            id: DraftId::new(),
            account_id: AccountId::new(),
            from: None,
            reply_headers: None,
            intent: DraftIntent::New,
            to: vec![Address {
                name: None,
                email: "sam@example.com".into(),
            }],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: subject.into(),
            content,
            attachments: Vec::new(),
            managed_attachments: None,
            inline_assets: Vec::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            inline_calendar_reply: None,
        }
    }

    fn markdown(body: &str) -> Draft {
        draft("Hello", DraftContent::markdown(body))
    }

    #[test]
    fn a_draft_with_no_gap_is_clear() {
        assert!(check(&markdown("See you Thursday at 3pm.")).is_empty());
        // Brackets and a question mark are not a gap.
        assert!(check(&markdown("Are we set [[ok]]? [?: no]")).is_empty());
    }

    #[test]
    fn each_gap_is_named_and_blocks() {
        let issues = check(&markdown(
            "Meeting on [[?: the date]] for [[?:  the\n price ]].",
        ));
        assert_eq!(issues.len(), 1);
        let issue = &issues[0];
        assert_eq!(issue.code, DraftSafetyIssueCode::UnfilledGap);
        assert_eq!(issue.severity, DraftSafetySeverity::Blocker);
        assert_eq!(issue.message, "2 gaps left: the date, the price");
    }

    #[test]
    fn one_gap_is_singular() {
        let issues = check(&markdown("Hi [[?: your name]]"));
        assert_eq!(issues[0].message, "1 gap left: your name");
    }

    #[test]
    fn a_gap_in_the_quote_or_the_signature_still_counts() {
        let body = "Thanks.\n\n> earlier [[?: the number]]\n\n-- \nAlex [[?: title]]";
        let issues = check(&markdown(body));
        assert_eq!(issues[0].message, "2 gaps left: the number, title");
    }

    #[test]
    fn a_gap_in_the_subject_counts() {
        let issues = check(&draft("Re: [[?: topic]]", DraftContent::markdown("Fine.")));
        assert_eq!(issues[0].message, "1 gap left: topic");
    }

    #[test]
    fn a_marker_left_open_and_an_empty_one_still_block() {
        let issues = check(&markdown("Call at [[?: the time\nThanks [[?:]]"));
        assert_eq!(issues[0].message, "2 gaps left: the time, an unnamed gap");
    }

    #[test]
    fn a_gap_written_with_entities_or_split_by_tags_is_still_a_gap() {
        // The text alternative is clean; the recipient's HTML view is not.
        for html in [
            "<p>&#91;&#91;?: the date]]</p>",
            "<p>&lbrack;&lbrack;?: the date]]</p>",
            "<p>[[<b>?</b>: the date]]</p>",
            "<p>[<span>[?: the date]]</span></p>",
        ] {
            let content = DraftContent::html(html, Some("See you then.".into()));
            let issues = check(&draft("s", content));
            assert_eq!(issues.len(), 1, "{html}");
            assert_eq!(issues[0].message, "1 gap left: the date", "{html}");
        }
        let clean = DraftContent::html("<p>See &#91;you&#93; then.</p>", Some("See you.".into()));
        assert!(check(&draft("s", clean)).is_empty());
    }

    fn only_message(draft: &Draft) -> String {
        let issues = check(draft);
        assert_eq!(issues.len(), 1, "{:?}", draft.content);
        issues[0].message.clone()
    }

    #[test]
    fn a_marker_the_renderer_wraps_across_lines_still_counts() {
        // 79 characters, then the marker: the reader's 80-column wrap would put
        // `[` on one line and `[?:` on the next.
        let html = format!("<p>{}&#91;&#91;?: the date]]</p>", "A".repeat(79));
        let content = DraftContent::html(html, Some("Clean text.".into()));
        assert_eq!(only_message(&draft("s", content)), "1 gap left: the date");
    }

    #[test]
    fn markdown_that_renders_to_a_gap_is_a_gap() {
        for source in [
            "Meet on &#91;&#91;?: the date]]",
            "Meet on &lbrack;&lbrack;?: the date]]",
        ] {
            assert_eq!(
                only_message(&markdown(source)),
                "1 gap left: the date",
                "{source}"
            );
        }
    }

    #[test]
    fn a_marker_split_by_whitespace_or_tags_is_an_unnamed_gap() {
        assert_eq!(
            only_message(&markdown("Meet on [[ ?: the date]]")),
            "1 gap left: an unnamed gap"
        );
        assert_eq!(
            only_message(&markdown("Meet on [\n[?: the date]]")),
            "1 gap left: an unnamed gap"
        );
        let split = DraftContent::html("<p>[<b></b>[?: the date]]</p>", Some("Clean.".into()));
        assert_eq!(only_message(&draft("s", split)), "1 gap left: the date");
        // One named and one only the whitespace-blind scan can read.
        assert_eq!(
            only_message(&markdown("[[?: the price]] and [[  ?: the date]]")),
            "2 gaps left: the price, an unnamed gap"
        );
    }

    #[test]
    fn a_gap_in_an_attribute_the_recipient_can_see_is_a_gap() {
        for html in [
            r#"<img src="https://example.com/x.png" alt="&#91;&#91;?: the date&#93;&#93;">"#,
            r#"<a href="https://example.com" title="&#91;&#91;?: the date&#93;&#93;">link</a>"#,
        ] {
            let content = DraftContent::html(html, Some("Clean text.".into()));
            assert_eq!(
                only_message(&draft("s", content)),
                "1 gap left: the date",
                "{html}"
            );
        }
        // Markdown renders an image's description into its `alt`.
        assert_eq!(
            only_message(&markdown(
                "![&#91;&#91;?: the date&#93;&#93;](https://example.com/x.png)"
            )),
            "1 gap left: the date"
        );
        let clean = DraftContent::html(
            r#"<img src="https://example.com/x.png" alt="A chart [x] of sales" title="Q3?: ok">"#,
            Some("Clean.".into()),
        );
        assert!(check(&draft("s", clean)).is_empty());
    }

    #[test]
    fn brackets_and_a_question_mark_apart_are_not_a_gap() {
        assert!(check(&markdown(
            "Tasks: [x] done. Is it ready?: yes. See [1] and [2]."
        ))
        .is_empty());
        let html = DraftContent::html("<p>Done [x] and what?: no</p>", Some("Done [x].".into()));
        assert!(check(&draft("s", html)).is_empty());
    }

    #[test]
    fn an_html_draft_is_checked_in_both_parts() {
        let in_text = DraftContent::html("<p>Hi</p>", Some("Hi [[?: name]]".into()));
        assert_eq!(check(&draft("s", in_text)).len(), 1);
        let only_in_html = DraftContent::html("<p>Hi [[?: name]]</p>", None);
        assert_eq!(check(&draft("s", only_in_html)).len(), 1);
        let clean = DraftContent::html("<p>Hi</p>", Some("Hi".into()));
        assert!(check(&draft("s", clean)).is_empty());
    }

    #[test]
    fn a_gap_hidden_among_65_attributes_is_a_non_overridable_blocker() {
        let attributes: String = (0..65).map(|n| format!(" a{n}")).collect();
        let html = format!("<p{attributes}>&#91;&#91;?: the date]]</p>");
        let issues = check(&draft("subject", DraftContent::Html { html, text: None }));
        assert!(
            issues
                .iter()
                .any(|issue| issue.code == DraftSafetyIssueCode::UnfilledGap
                    && issue.severity == mxr_core::types::DraftSafetySeverity::Blocker
                    && issue.override_token.is_none()),
            "got {issues:?}"
        );
    }
}

#[cfg(test)]
mod budget_tests {
    use super::*;

    #[test]
    fn an_over_budget_document_keeps_its_attribute_text_for_the_scan() {
        let html = format!("<img alt=\"zzalt\">{}", "<br>".repeat(200_000));
        assert!(attribute_text(&html).contains("zzalt"));
    }

    #[test]
    fn an_over_budget_document_keeps_its_visible_text_via_the_tag_strip() {
        let html = format!("<p>zzvisible</p>{}", "<br>".repeat(200_000));
        let text = visible_text(&html);
        assert!(text.contains("zzvisible"));
        assert!(!text.contains('<'));
    }
}
