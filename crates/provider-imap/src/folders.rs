#![cfg_attr(
    test,
    expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )
)]

use mxr_core::id::{AccountId, LabelId};
use mxr_core::types::{Label, LabelKind, Role};

/// Map IMAP folders to mxr labels using RFC 6154 SPECIAL-USE attributes.
pub fn map_folder_to_label(
    folder_name: &str,
    special_use: Option<&str>,
    account_id: &AccountId,
) -> Label {
    let (name, kind, role) = match special_use {
        Some("\\Inbox") => ("INBOX".to_string(), LabelKind::System, Some(Role::Inbox)),
        Some("\\Sent") => ("SENT".to_string(), LabelKind::System, Some(Role::Sent)),
        Some("\\Drafts") => ("DRAFT".to_string(), LabelKind::System, Some(Role::Drafts)),
        Some("\\Trash") => ("TRASH".to_string(), LabelKind::System, Some(Role::Trash)),
        Some("\\Junk" | "\\Spam") => ("SPAM".to_string(), LabelKind::System, Some(Role::Spam)),
        Some("\\Archive") => (
            "ARCHIVE".to_string(),
            LabelKind::System,
            Some(Role::Archive),
        ),
        Some("\\All") => ("ALL".to_string(), LabelKind::System, Some(Role::AllMail)),
        Some("\\Flagged") => (
            "STARRED".to_string(),
            LabelKind::System,
            Some(Role::Starred),
        ),
        _ => (folder_name.to_string(), LabelKind::Folder, None),
    };

    Label {
        id: LabelId::from_scoped_provider_id(account_id, "imap", &name),
        account_id: account_id.clone(),
        name,
        kind,
        color: None,
        provider_id: folder_name.to_string(),
        unread_count: 0,
        total_count: 0,
        role,
    }
}

/// Map Gmail IMAP folders to labels whose provider IDs match X-GM-LABELS.
pub fn map_gmail_folder_to_label(
    folder_name: &str,
    special_use: Option<&str>,
    account_id: &AccountId,
) -> Label {
    let mut label = map_folder_to_label(folder_name, special_use, account_id);
    if let Some(system_provider_id) = special_use.and_then(normalize_gmail_label_provider_id) {
        label.provider_id = system_provider_id.clone();
        label.id = LabelId::from_scoped_provider_id(account_id, "imap", &system_provider_id);
    } else if folder_name.eq_ignore_ascii_case("inbox") {
        label.provider_id = "INBOX".to_string();
        label.id = LabelId::from_scoped_provider_id(account_id, "imap", "INBOX");
    }
    label
}

/// Provider ID format for IMAP: "mailbox:uid" (e.g., "INBOX:12345")
pub fn format_provider_id(mailbox: &str, uid: u32) -> String {
    format!("{mailbox}:{uid}")
}

/// Normalize Gmail's X-GM-LABELS atoms to mxr provider label IDs.
/// Gmail returns system labels as IMAP atoms like `\\Inbox` and user labels
/// as their display names; mxr's label matching expects stable provider IDs
/// such as `INBOX`, `SENT`, `STARRED`, or the user label name.
pub fn normalize_gmail_label_provider_id(label: &str) -> Option<String> {
    let trimmed = label.trim().trim_matches('"');
    let normalized = match trimmed.to_ascii_lowercase().as_str() {
        "\\inbox" | "inbox" => "INBOX".to_string(),
        "\\sent" | "sent" => "SENT".to_string(),
        "\\draft" | "\\drafts" | "draft" | "drafts" => "DRAFT".to_string(),
        "\\trash" | "trash" => "TRASH".to_string(),
        "\\spam" | "\\junk" | "spam" | "junk" => "SPAM".to_string(),
        "\\flagged" | "\\starred" | "flagged" | "starred" => "STARRED".to_string(),
        "\\all" | "\\allmail" | "all" | "all mail" => "ALL".to_string(),
        "" => return None,
        _ => trimmed.to_string(),
    };
    Some(normalized)
}

pub fn parse_provider_id(id: &str) -> Result<(String, u32), crate::error::ImapProviderError> {
    let (mailbox, uid_str) = id
        .rsplit_once(':')
        .ok_or_else(|| crate::error::ImapProviderError::InvalidProviderId(id.to_string()))?;
    let uid = uid_str
        .parse()
        .map_err(|_| crate::error::ImapProviderError::InvalidProviderId(id.to_string()))?;
    Ok((mailbox.to_string(), uid))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_to_label_maps_special_use() {
        let aid = AccountId::new();
        let label = map_folder_to_label("INBOX", Some("\\Inbox"), &aid);
        assert_eq!(label.name, "INBOX");
        assert_eq!(label.kind, LabelKind::System);

        let label = map_folder_to_label("Sent Messages", Some("\\Sent"), &aid);
        assert_eq!(label.name, "SENT");

        let label = map_folder_to_label("Junk", Some("\\Junk"), &aid);
        assert_eq!(label.name, "SPAM");

        let label = map_folder_to_label("Archive", Some("\\Archive"), &aid);
        assert_eq!(label.name, "ARCHIVE");

        let label = map_folder_to_label("All Mail", Some("\\All"), &aid);
        assert_eq!(label.name, "ALL");
    }

    #[test]
    fn folder_to_label_custom_folder() {
        let aid = AccountId::new();
        let label = map_folder_to_label("Projects/Work", None, &aid);
        assert_eq!(label.name, "Projects/Work");
        assert_eq!(label.kind, LabelKind::Folder);
    }

    #[test]
    fn same_imap_folder_is_distinct_across_accounts() {
        let first_account = AccountId::from_provider_id("imap", "first@example.com");
        let second_account = AccountId::from_provider_id("imap", "second@example.com");

        let first = map_folder_to_label("INBOX", Some("\\Inbox"), &first_account);
        let second = map_folder_to_label("INBOX", Some("\\Inbox"), &second_account);

        assert_eq!(first.provider_id, second.provider_id);
        assert_ne!(first.account_id, second.account_id);
        assert_ne!(first.id, second.id);
    }

    #[test]
    fn provider_id_roundtrip() {
        let id = format_provider_id("INBOX", 12345);
        assert_eq!(id, "INBOX:12345");

        let (mailbox, uid) = parse_provider_id(&id).unwrap();
        assert_eq!(mailbox, "INBOX");
        assert_eq!(uid, 12345);
    }

    #[test]
    fn provider_id_with_nested_folder() {
        let id = format_provider_id("Projects/Work", 42);
        let (mailbox, uid) = parse_provider_id(&id).unwrap();
        assert_eq!(mailbox, "Projects/Work");
        assert_eq!(uid, 42);
    }

    #[test]
    fn provider_id_invalid() {
        assert!(parse_provider_id("no-colon").is_err());
        assert!(parse_provider_id("INBOX:notanumber").is_err());
    }
}
