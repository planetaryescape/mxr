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
    let cleaned = reader_clean(draft.content.analysis_text());
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
    use mxr_core::types::{Draft, DraftContent, DraftIntent};
    use mxr_core::{AccountId, DraftId};

    fn draft_with(body: &str) -> Draft {
        Draft {
            revision: Some(1),
            id: DraftId::new(),
            account_id: AccountId::new(),
            from: None,
            reply_headers: None,
            intent: DraftIntent::New,
            to: Vec::new(),
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: "subject".into(),
            content: DraftContent::markdown(body),
            managed_attachments: None,
            attachments: Vec::new(),
            inline_assets: Vec::new(),
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
}
