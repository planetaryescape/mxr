use mxr_core::id::{AccountId, LabelId};
use mxr_core::types::{Label, LabelKind};

/// Map IMAP folders to mxr labels using RFC 6154 SPECIAL-USE attributes.
pub fn map_folder_to_label(
    folder_name: &str,
    special_use: Option<&str>,
    account_id: &AccountId,
) -> Label {
    let (name, kind) = match special_use {
        Some("\\Inbox") => ("INBOX".to_string(), LabelKind::System),
        Some("\\Sent") => ("SENT".to_string(), LabelKind::System),
        Some("\\Drafts") => ("DRAFT".to_string(), LabelKind::System),
        Some("\\Trash") => ("TRASH".to_string(), LabelKind::System),
        Some("\\Junk") | Some("\\Spam") => ("SPAM".to_string(), LabelKind::System),
        Some("\\Archive") => ("ARCHIVE".to_string(), LabelKind::System),
        Some("\\All") => ("ALL".to_string(), LabelKind::System),
        Some("\\Flagged") => ("STARRED".to_string(), LabelKind::System),
        _ => (folder_name.to_string(), LabelKind::Folder),
    };

    Label {
        id: LabelId::from_provider_id("imap", &name),
        account_id: account_id.clone(),
        name,
        kind,
        color: None,
        provider_id: folder_name.to_string(),
        unread_count: 0,
        total_count: 0,
    }
}

/// Provider ID format for IMAP: "mailbox:uid" (e.g., "INBOX:12345")
pub fn format_provider_id(mailbox: &str, uid: u32) -> String {
    format!("{mailbox}:{uid}")
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
