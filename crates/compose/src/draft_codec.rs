//! Bidirectional codec between a stored [`Draft`] and the editor-facing
//! compose-file format (YAML frontmatter + markdown body).
//!
//! Before this module the mapping was open-coded in the daemon, web, and TUI
//! crates (each with its own `format_addresses` and inline `ComposeFrontmatter`
//! construction). Centralizing it here lets every surface load a stored draft
//! into `$EDITOR` and round-trip the edit back to the *same* [`DraftId`] — the
//! basis for editing a draft in place instead of creating a new one and
//! discarding the old.
//!
//! [`DraftId`]: mxr_core::id::DraftId

use crate::frontmatter::{
    parse_compose_file, render_compose_file, ComposeError, ComposeFrontmatter,
};
use chrono::{DateTime, Utc};
use mxr_core::types::{Address, Draft, ReplyHeaders};
use std::path::PathBuf;

/// Render a list of addresses back into a `"Name <email>, email"` header
/// string suitable for a compose-file `to:`/`cc:`/`bcc:` field.
pub fn format_addresses(addresses: &[Address]) -> String {
    addresses
        .iter()
        .map(|address| match address.name.as_deref() {
            Some(name) if !name.trim().is_empty() => format!("{name} <{}>", address.email),
            _ => address.email.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Build editor frontmatter from a stored draft. `from` is the sending
/// account's address — the [`Draft`] stores only an `account_id`, so the
/// caller resolves the email and passes it in.
pub fn frontmatter_from_draft(draft: &Draft, from: &str) -> ComposeFrontmatter {
    ComposeFrontmatter {
        to: format_addresses(&draft.to),
        cc: format_addresses(&draft.cc),
        bcc: format_addresses(&draft.bcc),
        subject: draft.subject.clone(),
        from: from.to_string(),
        in_reply_to: draft
            .reply_headers
            .as_ref()
            .map(|headers| headers.in_reply_to.clone()),
        intent: draft.intent,
        references: draft
            .reply_headers
            .as_ref()
            .map(|headers| headers.references.clone())
            .unwrap_or_default(),
        thread_id: draft
            .reply_headers
            .as_ref()
            .and_then(|headers| headers.thread_id.clone()),
        attach: draft
            .attachments
            .iter()
            .map(|attachment| attachment.display().to_string())
            .collect(),
        signature: None,
    }
}

/// Render a stored draft into the editor-facing compose-file text
/// (frontmatter + markdown body). No context block: an edit reopens the
/// user's own content, not a quoted original.
pub fn draft_to_compose_file(draft: &Draft, from: &str) -> Result<String, ComposeError> {
    let frontmatter = frontmatter_from_draft(draft, from);
    render_compose_file(&frontmatter, &draft.body_markdown, None)
}

/// Re-assemble an edited compose file back into a [`Draft`], preserving the
/// original `id`, `account_id`, `created_at`, and `inline_calendar_reply`,
/// and stamping `updated_at`. This is what makes "edit in place" possible:
/// the same [`DraftId`] round-trips instead of a new one being minted.
///
/// [`DraftId`]: mxr_core::id::DraftId
pub fn apply_edited_compose_file(
    existing: &Draft,
    content: &str,
    updated_at: DateTime<Utc>,
) -> Result<Draft, ComposeError> {
    let (frontmatter, body) = parse_compose_file(content)?;
    let reply_headers = frontmatter
        .in_reply_to
        .as_ref()
        .map(|in_reply_to| ReplyHeaders {
            in_reply_to: in_reply_to.clone(),
            references: frontmatter.references.clone(),
            thread_id: frontmatter.thread_id.clone(),
        })
        .or_else(|| existing.reply_headers.clone());

    Ok(Draft {
        id: existing.id.clone(),
        account_id: existing.account_id.clone(),
        reply_headers,
        // A user who clears `intent:` back to the default keeps the draft's
        // original intent rather than silently downgrading a reply to a new.
        intent: if frontmatter.intent == mxr_core::DraftIntent::New {
            existing.intent
        } else {
            frontmatter.intent
        },
        to: mxr_mail_parse::parse_address_list(&frontmatter.to),
        cc: mxr_mail_parse::parse_address_list(&frontmatter.cc),
        bcc: mxr_mail_parse::parse_address_list(&frontmatter.bcc),
        subject: frontmatter.subject,
        body_markdown: body,
        attachments: frontmatter.attach.iter().map(PathBuf::from).collect(),
        inline_calendar_reply: existing.inline_calendar_reply.clone(),
        created_at: existing.created_at,
        updated_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::id::{AccountId, DraftId};
    use mxr_core::types::DraftIntent;

    fn sample_draft() -> Draft {
        Draft {
            id: DraftId::new(),
            account_id: AccountId::new(),
            reply_headers: None,
            intent: DraftIntent::New,
            to: vec![
                Address {
                    name: Some("Alice Example".into()),
                    email: "alice@example.com".into(),
                },
                Address {
                    name: None,
                    email: "bob@example.com".into(),
                },
            ],
            cc: vec![],
            bcc: vec![],
            subject: "Q4 plan".into(),
            body_markdown: "Body line one.\n\nBody line two.".into(),
            attachments: vec![],
            inline_calendar_reply: None,
            created_at: DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
            updated_at: DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
        }
    }

    #[test]
    fn format_addresses_renders_name_and_bare() {
        let rendered = format_addresses(&sample_draft().to);
        assert_eq!(
            rendered,
            "Alice Example <alice@example.com>, bob@example.com"
        );
    }

    #[test]
    fn round_trip_preserves_id_created_at_and_content() {
        let original = sample_draft();
        let file = draft_to_compose_file(&original, "me@example.com").unwrap();
        assert!(file.contains("subject: Q4 plan"));
        assert!(file.contains("from: me@example.com"));
        assert!(file.contains("Body line one."));

        let edited = file.replace("Q4 plan", "Q4 plan (revised)");
        let later = DateTime::from_timestamp(1_700_000_999, 0).unwrap();
        let result = apply_edited_compose_file(&original, &edited, later).unwrap();

        // Identity and creation time survive the edit; only content + updated_at change.
        assert_eq!(result.id, original.id);
        assert_eq!(result.account_id, original.account_id);
        assert_eq!(result.created_at, original.created_at);
        assert_eq!(result.updated_at, later);
        assert_eq!(result.subject, "Q4 plan (revised)");
        assert_eq!(result.to.len(), 2);
        assert_eq!(result.to[0].email, "alice@example.com");
    }

    #[test]
    fn absent_intent_falls_back_to_existing() {
        let mut original = sample_draft();
        original.intent = DraftIntent::Reply;
        // A compose file with no `intent:` line — the user never touched it, so
        // the parsed frontmatter defaults to New and must fall back to Reply.
        let file = "---\nto: a@example.com\nsubject: Hi\nfrom: me@example.com\n---\n\nBody.";
        let now = DateTime::from_timestamp(1_700_000_500, 0).unwrap();
        let result = apply_edited_compose_file(&original, file, now).unwrap();
        assert_eq!(result.intent, DraftIntent::Reply);
    }
}
