//! End-to-end IMAP sync flow integration tests.
//!
//! Tests the full cycle: initial sync → delta sync → fetch body → mutate.
//! Uses MockImapSessionFactory — no real IMAP server needed.

use mxr_core::id::AccountId;
use mxr_core::types::MessageFlags;
use mxr_provider_imap::config::ImapConfig;
use mxr_provider_imap::types::{FetchedMessage, ImapAddress, ImapEnvelope};

// We need to re-export the mock from the library for integration tests.
// Since mock is cfg(test) only and integration tests are separate crates,
// we test through the public API using the test constructor.

// NOTE: The MockImapSessionFactory is pub(crate) / cfg(test) in session.rs,
// so integration tests cannot directly use it. The full end-to-end test
// lives in lib.rs as `full_sync_flow_initial_then_delta_then_fetch_and_mutate`.
//
// This file tests through the public API surface only.

fn test_config() -> ImapConfig {
    serde_json::from_str(
        r#"{
        "host": "imap.test.com",
        "port": 993,
        "username": "test@test.com",
        "password_ref": "test/imap"
    }"#,
    )
    .unwrap()
}

#[test]
fn imap_config_deserializes_correctly() {
    let config = test_config();
    assert_eq!(config.host, "imap.test.com");
    assert_eq!(config.port, 993);
    assert_eq!(config.username, "test@test.com");
    assert!(config.use_tls);
}

#[test]
fn provider_id_format_roundtrip() {
    use mxr_provider_imap::folders::{format_provider_id, parse_provider_id};

    let id = format_provider_id("INBOX", 42);
    assert_eq!(id, "INBOX:42");

    let (mailbox, uid) = parse_provider_id(&id).unwrap();
    assert_eq!(mailbox, "INBOX");
    assert_eq!(uid, 42);
}

#[test]
fn provider_id_nested_folder() {
    use mxr_provider_imap::folders::{format_provider_id, parse_provider_id};

    let id = format_provider_id("Work/Projects/Active", 999);
    let (mailbox, uid) = parse_provider_id(&id).unwrap();
    assert_eq!(mailbox, "Work/Projects/Active");
    assert_eq!(uid, 999);
}

#[test]
fn folder_to_label_mapping_comprehensive() {
    use mxr_core::types::LabelKind;
    use mxr_provider_imap::folders::map_folder_to_label;

    let aid = AccountId::new();

    // System folders
    let cases = [
        ("INBOX", Some("\\Inbox"), "INBOX", LabelKind::System),
        ("Sent Mail", Some("\\Sent"), "SENT", LabelKind::System),
        ("Drafts", Some("\\Drafts"), "DRAFT", LabelKind::System),
        ("Bin", Some("\\Trash"), "TRASH", LabelKind::System),
        ("Junk", Some("\\Junk"), "SPAM", LabelKind::System),
        ("All Mail", Some("\\All"), "ALL", LabelKind::System),
        ("Starred", Some("\\Flagged"), "STARRED", LabelKind::System),
    ];

    for (folder, special, expected_name, expected_kind) in cases {
        let label = map_folder_to_label(folder, special, &aid);
        assert_eq!(label.name, expected_name, "folder={folder}");
        assert_eq!(label.kind, expected_kind, "folder={folder}");
        assert_eq!(label.provider_id, folder);
    }

    // Custom folder
    let label = map_folder_to_label("Receipts", None, &aid);
    assert_eq!(label.name, "Receipts");
    assert_eq!(label.kind, LabelKind::Folder);
}

#[test]
fn parse_flags_comprehensive() {
    use mxr_provider_imap::parse::flags_from_imap;

    // All standard flags
    let all = flags_from_imap(&[
        "\\Seen".into(),
        "\\Flagged".into(),
        "\\Draft".into(),
        "\\Deleted".into(),
        "\\Answered".into(),
    ]);
    assert!(all.contains(MessageFlags::READ));
    assert!(all.contains(MessageFlags::STARRED));
    assert!(all.contains(MessageFlags::DRAFT));
    assert!(all.contains(MessageFlags::TRASH));
    assert!(all.contains(MessageFlags::ANSWERED));

    // Empty
    let empty = flags_from_imap(&[]);
    assert!(empty.is_empty());

    // Unknown flags ignored
    let unknown = flags_from_imap(&["$MDNSent".into(), "$Forwarded".into()]);
    assert!(unknown.is_empty());
}

#[test]
fn parse_imap_date_formats() {
    use mxr_provider_imap::parse::parse_imap_date;

    // RFC 2822
    let dt = parse_imap_date("Fri, 15 Mar 2024 09:30:00 +0000").unwrap();
    assert_eq!(dt.format("%Y-%m-%d").to_string(), "2024-03-15");

    // IMAP INTERNALDATE format
    let dt = parse_imap_date("01-Jan-2025 00:00:00 +0000").unwrap();
    assert_eq!(dt.format("%Y-%m-%d").to_string(), "2025-01-01");

    // Invalid
    assert!(parse_imap_date("garbage").is_err());
}

#[test]
fn imap_fetch_to_envelope_end_to_end() {
    use mxr_provider_imap::parse::imap_fetch_to_envelope;

    let account_id = AccountId::new();
    let msg = FetchedMessage {
        uid: 100,
        flags: vec!["\\Seen".into(), "\\Flagged".into()],
        envelope: Some(ImapEnvelope {
            date: Some("Wed, 1 Jan 2025 10:00:00 +0000".into()),
            subject: Some("Project update".into()),
            from: vec![ImapAddress {
                name: Some("Alice Smith".into()),
                email: "alice@company.com".into(),
            }],
            to: vec![
                ImapAddress {
                    name: Some("Bob".into()),
                    email: "bob@company.com".into(),
                },
                ImapAddress {
                    name: None,
                    email: "carol@company.com".into(),
                },
            ],
            cc: vec![ImapAddress {
                name: None,
                email: "team@company.com".into(),
            }],
            bcc: vec![],
            message_id: Some("<update-100@company.com>".into()),
            in_reply_to: Some("<original@company.com>".into()),
        }),
        body: None,
        header: Some(
            b"References: <thread-start@company.com> <original@company.com>\r\n".to_vec(),
        ),
        size: Some(4096),
    };

    let env = imap_fetch_to_envelope(&msg, "INBOX", &account_id).unwrap();

    assert_eq!(env.provider_id, "INBOX:100");
    assert_eq!(env.subject, "Project update");
    assert_eq!(env.from.email, "alice@company.com");
    assert_eq!(env.from.name, Some("Alice Smith".into()));
    assert_eq!(env.to.len(), 2);
    assert_eq!(env.cc.len(), 1);
    assert!(env.flags.contains(MessageFlags::READ));
    assert!(env.flags.contains(MessageFlags::STARRED));
    assert_eq!(env.size_bytes, 4096);
    assert_eq!(
        env.message_id_header,
        Some("<update-100@company.com>".into())
    );
    assert_eq!(env.in_reply_to, Some("<original@company.com>".into()));
    assert_eq!(env.references.len(), 2);
    assert_eq!(env.label_provider_ids, vec!["INBOX"]);
}

#[test]
fn parse_message_body_multipart_with_attachment() {
    use mxr_core::id::MessageId;
    use mxr_provider_imap::parse::parse_message_body;

    let raw = concat!(
        "From: sender@example.com\r\n",
        "To: receiver@example.com\r\n",
        "Subject: Report\r\n",
        "MIME-Version: 1.0\r\n",
        "Content-Type: multipart/mixed; boundary=\"sep\"\r\n",
        "\r\n",
        "--sep\r\n",
        "Content-Type: text/plain\r\n",
        "\r\n",
        "Please find the report attached.\r\n",
        "--sep\r\n",
        "Content-Type: text/html\r\n",
        "\r\n",
        "<p>Please find the report attached.</p>\r\n",
        "--sep\r\n",
        "Content-Type: application/pdf; name=\"Q4-report.pdf\"\r\n",
        "Content-Disposition: attachment; filename=\"Q4-report.pdf\"\r\n",
        "Content-Transfer-Encoding: base64\r\n",
        "\r\n",
        "JVBERi0xLjQK\r\n",
        "--sep--\r\n",
    );

    let msg_id = MessageId::new();
    let body = parse_message_body(raw.as_bytes(), &msg_id);

    assert!(body.text_plain.is_some());
    assert!(body.text_html.is_some());
    assert_eq!(body.attachments.len(), 1);
    assert_eq!(body.attachments[0].filename, "Q4-report.pdf");
    assert!(body.attachments[0].mime_type.contains("pdf"));
    assert_eq!(body.attachments[0].message_id, msg_id);
}
