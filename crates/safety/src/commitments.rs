//! Outgoing commitment candidate extractor — deterministic prefilter.
//!
//! Slice 2.1 will add LLM extraction layered on top of this prefilter.
//! Today this module ships only the deterministic regex hits so the
//! safety report can include `Severity::Info` candidate hints even
//! when the LLM is disabled or not yet wired.

use mxr_core::types::{Draft, DraftSafetyIssue, DraftSafetyIssueCode, DraftSafetySeverity};
use mxr_reader::{clean, ReaderConfig};
use once_cell::sync::Lazy;
use regex::Regex;

static FIRST_PERSON_PROMISE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?im)\bI\s*(?:'?ll|will|can|am\s+going\s+to|'?m\s+going\s+to)\s+([a-z][a-z\s\-']{2,80}?)(?:[.!\n]|$)",
    )
    .expect("first-person promise regex literal compiles")
});

static FOLLOW_UP_PHRASE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\bI\s*(?:'?ll|will)\s+(follow\s+up|get\s+back|circle\s+back|send|share|file|review|update|ping|reach\s+out)\b")
        .expect("follow-up phrase regex literal compiles")
});

pub fn detect_candidates(draft: &Draft) -> Vec<DraftSafetyIssue> {
    let cleaned = reader_clean(&draft.body_markdown);
    let mut hits = Vec::new();
    for cap in FIRST_PERSON_PROMISE.captures_iter(&cleaned) {
        if let Some(m) = cap.get(0) {
            hits.push(m.as_str().trim().to_string());
        }
    }
    for m in FOLLOW_UP_PHRASE.find_iter(&cleaned) {
        let txt = m.as_str().trim().to_string();
        if !hits.iter().any(|h| h.contains(&txt)) {
            hits.push(txt);
        }
    }
    hits.into_iter()
        .take(5)
        .map(|hit| {
            DraftSafetyIssue::new(
                DraftSafetyIssueCode::CommitmentCandidate,
                DraftSafetySeverity::Info,
                format!("possible commitment: {hit}"),
            )
            .with_detail(hit)
        })
        .collect()
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

    fn draft_with(body: &str) -> Draft {
        Draft {
            id: DraftId::new(),
            account_id: AccountId::new(),
            from: None,
            reply_headers: None,
            intent: DraftIntent::New,
            to: Vec::new(),
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: "subject".into(),
            body_markdown: body.into(),
            attachments: Vec::new(),
            inline_calendar_reply: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn ill_send_friday_is_a_candidate() {
        let issues = detect_candidates(&draft_with("Hi, I'll send the deck Friday."));
        assert!(!issues.is_empty(), "expected candidate, got none");
        assert_eq!(issues[0].code, DraftSafetyIssueCode::CommitmentCandidate);
        assert_eq!(issues[0].severity, DraftSafetySeverity::Info);
    }

    #[test]
    fn third_person_is_not_a_candidate() {
        let issues = detect_candidates(&draft_with("She'll send the deck Friday."));
        assert!(issues.is_empty(), "third-person should not match");
    }

    #[test]
    fn quoted_promise_is_ignored() {
        let body = "Got it.\n\nOn Mon, Alice wrote:\n> I'll send the deck Friday.\n";
        let issues = detect_candidates(&draft_with(body));
        assert!(
            issues.is_empty(),
            "quoted prior promise should not surface ({issues:?})"
        );
    }

    #[test]
    fn follow_up_phrase_variants_each_match() {
        for phrase in [
            "I'll follow up tomorrow.",
            "I'll get back to you Thursday.",
            "I will circle back next week.",
            "I'll ping the team.",
            "I'll reach out by Friday.",
        ] {
            let issues = detect_candidates(&draft_with(phrase));
            assert!(
                !issues.is_empty(),
                "expected candidate for `{phrase}`, got none"
            );
        }
    }

    #[test]
    fn multiple_commitments_capped_at_five() {
        let body = (0..10)
            .map(|i| format!("I'll send item {i} tomorrow."))
            .collect::<Vec<_>>()
            .join("\n");
        let issues = detect_candidates(&draft_with(&body));
        assert!(
            issues.len() <= 5,
            "candidate list must be capped at 5 (got {})",
            issues.len()
        );
        assert!(!issues.is_empty(), "but at least one should be present");
    }

    #[test]
    fn dedup_does_not_double_count_same_phrase() {
        // The follow-up phrase regex is a subset of the first-person
        // promise regex; impl should not emit two candidates for the
        // same span.
        let body = "I'll follow up tomorrow.";
        let issues = detect_candidates(&draft_with(body));
        assert_eq!(
            issues.len(),
            1,
            "single promise should produce one candidate (got {issues:?})"
        );
    }
}
