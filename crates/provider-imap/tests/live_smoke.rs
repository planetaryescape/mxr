use mxr_core::id::AccountId;
use mxr_core::types::{LabelKind, MessageFlags};
use mxr_provider_imap::config::ImapConfig;
use mxr_provider_imap::folders::{format_provider_id, map_folder_to_label, parse_provider_id};
use mxr_provider_imap::parse::flags_from_imap;

#[test]
fn provider_offline_smoke_imap_config_deserializes_defaults() {
    let config: ImapConfig = serde_json::from_str(
        r#"{
            "host": "imap.example.com",
            "port": 993,
            "username": "user@example.com",
            "password_ref": "mxr/test-imap"
        }"#,
    )
    .expect("valid config");

    assert_eq!(config.host, "imap.example.com");
    assert_eq!(config.port, 993);
    assert!(config.auth_required);
    assert!(config.use_tls);
}

#[test]
fn provider_offline_smoke_imap_provider_id_roundtrip() {
    let id = format_provider_id("INBOX", 42);
    let (mailbox, uid) = parse_provider_id(&id).expect("provider id");

    assert_eq!(mailbox, "INBOX");
    assert_eq!(uid, 42);
}

#[test]
fn provider_offline_smoke_imap_maps_folders_and_flags() {
    let account_id = AccountId::new();
    let label = map_folder_to_label("Sent Mail", Some("\\Sent"), &account_id);
    let flags = flags_from_imap(&["\\Seen".into(), "\\Flagged".into()]);

    assert_eq!(label.name, "SENT");
    assert_eq!(label.kind, LabelKind::System);
    assert!(flags.contains(MessageFlags::READ));
    assert!(flags.contains(MessageFlags::STARRED));
}
