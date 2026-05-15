//! Reply-all sanity check.
//!
//! Triggered only when caller flags `mode_reply_all`. If the body of the
//! draft addresses exactly one named participant and uses no group
//! language, warn that the reply may have been intended as a direct
//! reply, not reply-all.
//!
//! Severity is always `Warning`. Never blocks.

use mxr_core::types::{Draft, DraftSafetyIssue, DraftSafetyIssueCode, DraftSafetySeverity};
use mxr_reader::{clean, ReaderConfig};
use once_cell::sync::Lazy;
use regex::Regex;

use crate::SafetyContext;

static GROUP_LANG: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?im)^\s*(hi|hey|hello|dear)?\s*(team|all|folks|everyone|y'?all|guys)\b").unwrap()
});
static VOCATIVE: Lazy<Regex> = Lazy::new(|| {
    // "Hi Alice,", "Hey Alice -", "Alice,", "Hello Alice"
    Regex::new(r"(?m)^\s*(?i:hi|hey|hello|dear)?\s*([A-Z][a-zA-Z'’\-]{1,30})\s*[,\-:!]").unwrap()
});

pub fn check(draft: &Draft, ctx: &SafetyContext) -> Vec<DraftSafetyIssue> {
    if !ctx.mode_reply_all {
        return Vec::new();
    }
    let visible = draft.to.len() + draft.cc.len();
    if visible <= 2 {
        return Vec::new();
    }

    let cleaned = reader_clean(&draft.body_markdown);
    // Only the first non-empty paragraph influences the greeting check.
    // Past the first blank line we are reading the body, where capitalized
    // sentence starters like "Thanks," would create false positives.
    let head = first_paragraph(&cleaned);

    if GROUP_LANG.is_match(head) {
        return Vec::new();
    }

    let mut named = Vec::new();
    for cap in VOCATIVE.captures_iter(head) {
        let raw = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        if raw.is_empty() {
            continue;
        }
        let name = raw.to_string();
        if is_quoted_name(&name, ctx) {
            continue;
        }
        if !named.contains(&name) {
            named.push(name);
        }
    }

    if named.len() != 1 {
        return Vec::new();
    }
    let target = &named[0];

    // Find the address that this name resolves to (best-effort), so the
    // warning can name the singular addressee.
    let display_match = draft.to.iter().chain(draft.cc.iter()).find(|addr| {
        addr.name
            .as_deref()
            .map(|d| name_matches(d, target))
            .unwrap_or(false)
            || name_matches(addr.email.split('@').next().unwrap_or(""), target)
    });

    let addressee = match display_match {
        Some(addr) => addr.name.clone().unwrap_or_else(|| addr.email.clone()),
        None => target.clone(),
    };

    vec![DraftSafetyIssue::new(
        DraftSafetyIssueCode::ReplyAll,
        DraftSafetySeverity::Warning,
        format!(
            "draft body addresses only {addressee}, but reply-all sends to {visible} recipients"
        ),
    )]
}

fn first_paragraph(text: &str) -> &str {
    // Trim leading whitespace, then take through the first blank line.
    let trimmed_start = text.trim_start_matches(['\n', '\r', ' ', '\t']);
    let offset = text.len() - trimmed_start.len();
    let end = trimmed_start
        .find("\n\n")
        .or_else(|| trimmed_start.find("\r\n\r\n"))
        .map(|i| offset + i)
        .unwrap_or(text.len());
    &text[..end]
}

fn is_quoted_name(name: &str, ctx: &SafetyContext) -> bool {
    ctx.thread_display_names
        .iter()
        .any(|n| n.eq_ignore_ascii_case(name))
}

fn name_matches(haystack: &str, name: &str) -> bool {
    haystack
        .split(|c: char| !c.is_alphabetic())
        .any(|tok| tok.eq_ignore_ascii_case(name))
}

fn reader_clean(body: &str) -> String {
    let cfg = ReaderConfig {
        html_command: None,
        strip_signatures: true,
        collapse_quotes: true,
        strip_boilerplate: true,
        strip_tracking: true,
    };
    clean(Some(body), None, &cfg).content
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use mxr_core::types::{Address, Draft, DraftIntent};
    use mxr_core::{AccountId, DraftId};

    fn addr(email: &str, display: Option<&str>) -> Address {
        Address {
            email: email.into(),
            name: display.map(str::to_string),
        }
    }

    fn draft(to: Vec<Address>, cc: Vec<Address>, body: &str) -> Draft {
        Draft {
            id: DraftId::new(),
            account_id: AccountId::new(),
            reply_headers: None,
            intent: DraftIntent::ReplyAll,
            to,
            cc,
            bcc: Vec::new(),
            subject: "subject".into(),
            body_markdown: body.into(),
            attachments: Vec::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn ctx_reply_all() -> SafetyContext {
        SafetyContext {
            mode_reply_all: true,
            ..Default::default()
        }
    }

    #[test]
    fn reply_all_to_six_addressing_one_warns() {
        let to = vec![
            addr("alice@x.com", Some("Alice")),
            addr("bob@x.com", Some("Bob")),
            addr("carol@x.com", Some("Carol")),
            addr("dave@x.com", Some("Dave")),
            addr("eve@x.com", Some("Eve")),
            addr("frank@x.com", Some("Frank")),
        ];
        let body = "Hi Alice,\n\nThanks for the heads up. I'll send the deck Friday.";
        let issues = check(&draft(to, vec![], body), &ctx_reply_all());
        assert_eq!(issues.len(), 1, "expected warn, got {issues:?}");
        assert_eq!(issues[0].code, DraftSafetyIssueCode::ReplyAll);
        assert!(issues[0].message.contains("Alice"));
    }

    #[test]
    fn reply_all_with_team_greeting_passes() {
        let to = (0..6)
            .map(|i| addr(&format!("user{i}@x.com"), Some(&format!("User{i}"))))
            .collect();
        let body = "Hi Team,\n\nQuick update: launch slips to Friday.";
        assert!(check(&draft(to, vec![], body), &ctx_reply_all()).is_empty());
    }

    #[test]
    fn small_thread_passes() {
        let to = vec![addr("alice@x.com", Some("Alice"))];
        let body = "Hi Alice, thanks.";
        assert!(check(&draft(to, vec![], body), &ctx_reply_all()).is_empty());
    }

    #[test]
    fn not_reply_all_passes() {
        let to = (0..5)
            .map(|i| addr(&format!("user{i}@x.com"), Some(&format!("User{i}"))))
            .collect();
        let body = "Hi Alice,\nthanks";
        let mut ctx = ctx_reply_all();
        ctx.mode_reply_all = false;
        assert!(check(&draft(to, vec![], body), &ctx).is_empty());
    }

    #[test]
    fn group_greeting_short_circuits() {
        let to = (0..6)
            .map(|i| addr(&format!("user{i}@x.com"), Some(&format!("User{i}"))))
            .collect();
        let body = "Hi Team,\n\nGood point.\n\nOn Mon, Alice wrote:\n> Hi everyone, thoughts?";
        assert!(
            check(&draft(to, vec![], body), &ctx_reply_all()).is_empty(),
            "Hi Team should suppress via GROUP_LANG"
        );
    }

    #[test]
    fn vocative_matching_thread_participant_is_filtered() {
        // Same body, two contexts. With Sam in thread_display_names the
        // vocative is treated as continuing thread context (ambiguous);
        // without it, the warning fires.
        let to: Vec<_> = (0..6)
            .map(|i| addr(&format!("user{i}@x.com"), Some(&format!("User{i}"))))
            .collect();
        let body = "Hi Sam,\n\nThanks, that works.";

        let mut ctx_with = ctx_reply_all();
        ctx_with.thread_display_names = vec!["Sam".to_string()];
        assert!(
            check(&draft(to.clone(), vec![], body), &ctx_with).is_empty(),
            "Sam is on the thread → ambiguous → no warn"
        );

        let ctx_without = ctx_reply_all();
        let issues = check(&draft(to, vec![], body), &ctx_without);
        assert_eq!(issues.len(), 1, "Sam not on thread → warn (got {issues:?})");
        assert!(issues[0].message.contains("Sam"));
    }

    #[test]
    fn two_named_people_in_one_greeting_does_not_warn() {
        // Documented contract: when greeting names more than one person
        // ("Hi Alice and Bob,"), we are NOT confidently addressing one,
        // so do not fire.
        let to = (0..6)
            .map(|i| addr(&format!("user{i}@x.com"), Some(&format!("User{i}"))))
            .collect();
        let body = "Hi Alice and Bob,\n\nLet's sync.";
        let issues = check(&draft(to, vec![], body), &ctx_reply_all());
        assert!(
            issues.is_empty(),
            "two-person greeting must not warn (got {issues:?})"
        );
    }

    #[test]
    fn three_recipient_boundary() {
        // visible == 3 should still be considered for warn (3 > 2).
        let to = vec![
            addr("alice@x.com", Some("Alice")),
            addr("bob@x.com", Some("Bob")),
            addr("carol@x.com", Some("Carol")),
        ];
        let body = "Hi Alice,\n\nQuick note.";
        let issues = check(&draft(to, vec![], body), &ctx_reply_all());
        assert_eq!(issues.len(), 1, "3 recipients addressing 1 should warn");
    }
}
