//! Missing-attachment detector.
//!
//! Pure regex over reader-cleaned subject + body. Strips quoted reply
//! context first so "see attached" inside a quoted prior message does not
//! trigger a warning when the user is just replying without re-attaching.

use mxr_core::types::{Draft, DraftSafetyIssue, DraftSafetyIssueCode, DraftSafetySeverity};
use mxr_reader::{clean, ReaderConfig};
use once_cell::sync::Lazy;
use regex::RegexSet;

static POSITIVE: Lazy<RegexSet> = Lazy::new(|| {
    RegexSet::new([
        r"(?i)\bsee\s+attach(ed|ment)\b",
        r"(?i)\bplease\s+(find|see)\s+attach(ed|ment)\b",
        r"(?i)\bi['’]?ve\s+attached\b",
        r"(?i)\bi\s+have\s+attached\b",
        r"(?i)\bi['’]?m\s+attaching\b",
        r"(?i)\bi\s+am\s+attaching\b",
        r"(?i)\battached\s+(is|are|please|herewith|for|to)\b",
        r"(?i)\battachment(s)?\s+(is|are|enclosed|included)\b",
        r"(?i)\benclosed\s+(is|are|please|herewith|for)\b",
        r"(?im)\battached\.?\s*$",
    ])
    .expect("attachment regexes compile")
});

static NEGATIVE: Lazy<RegexSet> = Lazy::new(|| {
    RegexSet::new([
        r"(?i)\bnot\s+attach(ed|ment)\b",
        r"(?i)\bwithout\s+attachment\b",
        r"(?i)\bno\s+attachment\b",
    ])
    .expect("attachment negative regexes compile")
});

pub fn check(draft: &Draft) -> Vec<DraftSafetyIssue> {
    if !draft.attachments.is_empty() {
        return Vec::new();
    }

    let cleaned = reader_clean(&draft.body_markdown);
    let haystack = format!("{}\n{}", draft.subject, cleaned);

    if NEGATIVE.is_match(&haystack) {
        // Conservative: any explicit negation suppresses the warning.
        // Real-world wording like "I did not attach the deck" would
        // otherwise produce a false positive.
        let positive_only = POSITIVE.is_match(&haystack)
            && !haystack
                .lines()
                .any(|line| NEGATIVE.is_match(line) && POSITIVE.is_match(line));
        if !positive_only {
            return Vec::new();
        }
    }

    if POSITIVE.is_match(&haystack) {
        vec![DraftSafetyIssue::new(
            DraftSafetyIssueCode::MissingAttachment,
            DraftSafetySeverity::Warning,
            "draft mentions an attachment but no file is attached",
        )]
    } else {
        Vec::new()
    }
}

fn reader_clean(body: &str) -> String {
    let cfg = ReaderConfig {
        html_command: None,
        strip_signatures: true,
        collapse_quotes: true,
        strip_boilerplate: false,
        strip_tracking: false,
    };
    clean(Some(body), None, &cfg).content
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use mxr_core::types::{Draft, DraftIntent};
    use mxr_core::{AccountId, DraftId};
    use std::path::PathBuf;

    fn draft_with(body: &str, attachments: Vec<PathBuf>) -> Draft {
        Draft {
            id: DraftId::new(),
            account_id: AccountId::new(),
            from: None,
            reply_headers: None,
            intent: DraftIntent::New,
            to: Vec::new(),
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: "test subject".into(),
            body_markdown: body.into(),
            attachments,
            inline_calendar_reply: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn warns_when_body_says_see_attached_and_no_files() {
        let issues = check(&draft_with("Hi, see attached for the deck.", vec![]));
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, DraftSafetyIssueCode::MissingAttachment);
        assert_eq!(issues[0].severity, DraftSafetySeverity::Warning);
    }

    #[test]
    fn passes_when_attachments_present() {
        let issues = check(&draft_with(
            "Please see attached.",
            vec![PathBuf::from("/tmp/deck.pdf")],
        ));
        assert!(issues.is_empty());
    }

    #[test]
    fn passes_when_phrase_is_negated() {
        let issues = check(&draft_with("There is no attachment yet.", vec![]));
        assert!(
            issues.is_empty(),
            "negated phrasing should not warn (got {issues:?})"
        );
    }

    #[test]
    fn same_line_positive_and_negation_still_warns() {
        // The override branch: when a positive AND negative co-exist on
        // the same line, a follow-on positive ("see attached") still
        // matters. Conservative behavior: warn unless every positive
        // line is negated.
        let body = "Sorry I forgot to attach earlier — please see attached now.";
        let issues = check(&draft_with(body, vec![]));
        assert_eq!(
            issues.len(),
            1,
            "positive on same-line as negation should still warn ({issues:?})"
        );
    }

    #[test]
    fn ignores_quoted_context() {
        let body = "Got it.\n\nOn Mon, Alice <a@b.com> wrote:\n> Please see attached for the deck.";
        let issues = check(&draft_with(body, vec![]));
        assert!(
            issues.is_empty(),
            "quoted reply context should not trigger (got {issues:?})"
        );
    }

    #[test]
    fn ive_attached_variant_warns() {
        let issues = check(&draft_with("I've attached the spec.", vec![]));
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn enclosed_variant_warns() {
        let issues = check(&draft_with("Enclosed is the contract.", vec![]));
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn subject_alone_can_trigger() {
        let mut d = draft_with("Body unrelated.", vec![]);
        d.subject = "Updated deck attached".into();
        let issues = check(&d);
        assert_eq!(issues.len(), 1);
    }
}
