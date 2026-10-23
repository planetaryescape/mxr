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
    // The raw text: the reader-cleaned view the other checks use drops quoted
    // lines and signatures, and a gap there would still be sent.
    let mut labels = gap_labels(&draft.subject);
    match &draft.content {
        DraftContent::Markdown { source } => labels.extend(gap_labels(source)),
        DraftContent::Html { html, text } => {
            labels.extend(gap_labels(text.as_deref().unwrap_or("")));
            // A gap only in the HTML part would be sent too.
            if labels.is_empty() {
                labels.extend(gap_labels(html));
            }
        }
    }
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
    fn an_html_draft_is_checked_in_both_parts() {
        let in_text = DraftContent::html("<p>Hi</p>", Some("Hi [[?: name]]".into()));
        assert_eq!(check(&draft("s", in_text)).len(), 1);
        let only_in_html = DraftContent::html("<p>Hi [[?: name]]</p>", None);
        assert_eq!(check(&draft("s", only_in_html)).len(), 1);
        let clean = DraftContent::html("<p>Hi</p>", Some("Hi".into()));
        assert!(check(&draft("s", clean)).is_empty());
    }
}
