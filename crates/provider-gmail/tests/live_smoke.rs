use mxr::mxr_core::id::AccountId;
use mxr::mxr_core::types::{MessageFlags, UnsubscribeMethod};
use mxr::mxr_provider_gmail::parse::{
    gmail_message_to_envelope, labels_to_flags, parse_address_list,
};
use mxr::mxr_provider_gmail::types::GmailMessage;
use serde_json::json;

fn fixture_message() -> GmailMessage {
    serde_json::from_value(json!({
        "id": "msg-1",
        "threadId": "thread-1",
        "labelIds": ["INBOX", "UNREAD", "STARRED"],
        "snippet": "fixture snippet",
        "historyId": "42",
        "internalDate": "1710495000000",
        "sizeEstimate": 1024,
        "payload": {
            "mimeType": "text/plain",
            "headers": [
                {"name": "From", "value": "Alice Example <alice@example.com>"},
                {"name": "To", "value": "Bob Example <bob@example.com>"},
                {"name": "Subject", "value": "Fixture subject"},
                {"name": "Date", "value": "Fri, 15 Mar 2024 09:30:00 +0000"},
                {"name": "Message-ID", "value": "<msg-1@example.com>"},
                {"name": "List-Unsubscribe", "value": "<https://example.com/unsubscribe>"}
            ],
            "body": {"size": 12, "data": "SGVsbG8gd29ybGQ"}
        }
    }))
    .expect("valid Gmail fixture")
}

#[test]
fn provider_offline_smoke_gmail_labels_to_flags_maps_expected_flags() {
    let flags = labels_to_flags(&["INBOX".into(), "STARRED".into()]);
    assert!(flags.contains(MessageFlags::READ));
    assert!(flags.contains(MessageFlags::STARRED));
    assert!(!flags.contains(MessageFlags::DRAFT));
}

#[test]
fn provider_offline_smoke_gmail_parses_rfc_address_list() {
    let addresses = parse_address_list("Alice <alice@example.com>, bob@example.com");
    assert_eq!(addresses.len(), 2);
    assert_eq!(addresses[0].email, "alice@example.com");
    assert_eq!(addresses[1].email, "bob@example.com");
}

#[test]
fn provider_offline_smoke_gmail_message_maps_to_envelope() {
    let message = fixture_message();
    let envelope = gmail_message_to_envelope(&message, &AccountId::new()).expect("envelope");

    assert_eq!(envelope.provider_id, "msg-1");
    assert_eq!(envelope.subject, "Fixture subject");
    assert_eq!(envelope.from.email, "alice@example.com");
    assert!(envelope.flags.contains(MessageFlags::STARRED));
    assert!(!envelope.flags.contains(MessageFlags::READ));
    assert!(matches!(
        envelope.unsubscribe,
        UnsubscribeMethod::HttpLink { .. }
    ));
}
