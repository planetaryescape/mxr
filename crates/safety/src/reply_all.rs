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
    Regex::new(r"(?im)^\s*(hi|hey|hello|dear)?\s*(team|all|folks|everyone|y'?all|guys)\b")
        .expect("group language regex literal compiles")
});
static VOCATIVE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)^\s*(?i:hi|hey|hello|dear)?\s*([A-Z][a-zA-Z'’\-]{1,30})\s*[,\-:!]")
        .expect("vocative regex literal compiles")
});

pub fn check(draft: &Draft, ctx: &SafetyContext) -> Vec<DraftSafetyIssue> {
    if !ctx.mode_reply_all {
        return Vec::new();
    }
    let visible = draft.to.len() + draft.cc.len();
    if visible <= 2 {
        return Vec::new();
    }

    let cleaned = reader_clean(draft.content.analysis_text());
    let head = first_paragraph(&cleaned);

    if GROUP_LANG.is_match(head) {
        return Vec::new();
    }

    let mut named = Vec::new();
    for cap in VOCATIVE.captures_iter(head) {
        let raw = cap.get(1).map_or("", |m| m.as_str());
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

    let display_match = draft.to.iter().chain(draft.cc.iter()).find(|addr| {
        addr.name
            .as_deref()
            .is_some_and(|d| name_matches(d, target))
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
    let trimmed_start = text.trim_start_matches(['\n', '\r', ' ', '\t']);
    let offset = text.len() - trimmed_start.len();
    let end = trimmed_start
        .find("\n\n")
        .or_else(|| trimmed_start.find("\r\n\r\n"))
        .map_or(text.len(), |i| offset + i);
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
    use mxr_core::types::{Address, Draft, DraftContent, DraftIntent};
    use mxr_core::{AccountId, DraftId};

    fn addr(email: &str, display: Option<&str>) -> Address {
        Address {
            email: email.into(),
            name: display.map(str::to_string),
        }
    }

    fn draft(to: Vec<Address>, cc: Vec<Address>, body: &str) -> Draft {
        Draft {
            revision: Some(1),
            id: DraftId::new(),
            account_id: AccountId::new(),
            from: None,
            reply_headers: None,
            intent: DraftIntent::ReplyAll,
            to,
            cc,
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
}
