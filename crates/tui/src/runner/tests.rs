use super::{apply_all_envelopes_refresh, handle_daemon_event};
use crate::action::Action;
use crate::app::PendingSend;
use crate::app::{
    ActivePane, App, BodySource, BodyViewMetadata, BodyViewState, LayoutMode, MailListMode,
    MailboxView, MutationEffect, PendingSendMode, Screen, SearchPane,
};
use crate::async_result::ComposeReadyData;
use crate::compose_flow::handle_compose_editor_status;
use crate::input::InputHandler;
use crate::test_fixtures::TestEnvelopeBuilder;
use mxr_core::id::*;
use mxr_core::types::*;
use mxr_core::MxrError;
use mxr_protocol::{DaemonEvent, MutationCommand, Request};
use mxr_test_support::render_to_string;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::os::unix::process::ExitStatusExt;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

fn make_test_envelopes(count: usize) -> Vec<Envelope> {
    (0..count)
        .map(|i| {
            TestEnvelopeBuilder::new()
                .provider_id(format!("fake-{i}"))
                .with_from_address(&format!("User {i}"), &format!("user{i}@example.com"))
                .to(vec![])
                .subject(format!("Subject {i}"))
                .message_id_header(None)
                .flags(if i % 2 == 0 {
                    MessageFlags::READ
                } else {
                    MessageFlags::empty()
                })
                .snippet(format!("Snippet {i}"))
                .size_bytes(1000)
                .build()
        })
        .collect()
}

fn make_unsubscribe_envelope(
    account_id: AccountId,
    sender_email: &str,
    unsub: UnsubscribeMethod,
) -> Envelope {
    TestEnvelopeBuilder::new()
        .account_id(account_id)
        .provider_id("unsub-fixture")
        .with_from_address("Newsletter", sender_email)
        .to(vec![])
        .subject("Newsletter")
        .message_id_header(None)
        .snippet("newsletter")
        .size_bytes(42)
        .unsubscribe(unsub)
        .build()
}

fn exit_status(code: i32) -> std::process::ExitStatus {
    std::process::ExitStatus::from_raw(code)
}

fn make_test_labels() -> Vec<Label> {
    crate::test_fixtures::test_system_labels(&AccountId::new())
}

fn set_active_inbox(app: &mut App) {
    app.mailbox.labels = make_test_labels();
    app.mailbox.active_label = app
        .mailbox
        .labels
        .iter()
        .find(|label| label.name.eq_ignore_ascii_case("INBOX"))
        .map(|label| label.id.clone());
}

fn notice_while_announced<'a>(
    place: &str,
    keys: &str,
    notice: &'a str,
    before: Option<&'a str>,
) -> Option<&'a str> {
    match mxr_keymap::defaults().moved_entry(place, keys) {
        Some(_) => Some(notice),
        None => before,
    }
}

fn with_move(app: &mut App, id: &str, key: &str, contexts: &[&str], until: &str) {
    let mut keymap = app.keybindings.keymap().clone();
    let action = keymap
        .actions
        .iter_mut()
        .find(|action| action.id == id)
        .unwrap_or_else(|| panic!("{id} is defined"));
    action.moved.push(mxr_keymap::MovedKey {
        key: key.to_string(),
        contexts: contexts.iter().map(ToString::to_string).collect(),
        until: until.to_string(),
        to: None,
        to_key: None,
    });
    app.keybindings
        .set_keymap(keymap.live_at(mxr_keymap::VERSION));
}

mod accounts_and_delivery;
mod assistant;
mod back_and_quit;
mod export;
mod focus;
mod input_and_compose;
mod keymap;
mod mailbox_views;
mod messages;
mod moves;
mod mutations_and_bulk;
mod now;
mod overlay_keys;
mod pattern_select;
mod places;
mod prefix_guide;
mod reader_and_diagnostics;
mod reader_folding;
mod reading;
mod records;
mod screen_keys;
mod semantic_and_connection;
mod snoozed;
mod todo;
mod unified_mailboxes;
mod updates;

mod rules;

#[test]
fn outcome_guidance_uses_retained_native_query_when_no_owner_is_in_response() {
    let account_id = AccountId::new();
    let operation_id = "draft:original-operation".to_string();
    let text = super::format_platform_response(&mxr_protocol::ResponseData::SendOutcome {
        outcome: Some(mxr_core::DraftSendOutcome {
            identity: mxr_core::SendIdentity {
                operation_id: operation_id.clone(),
                expected_revision: 7,
            },
            account_id,
            draft_id: DraftId::new(),
            state: mxr_core::SendOutcomeState::Accepted,
            local_message_id: None,
            provider_message_id: None,
            rfc2822_message_id: None,
            recovery: None,
        }),
    });
    assert!(text.contains("Provider accepted this send. Do not resend."));
    assert!(text.contains(&operation_id));
    assert!(text.contains("Check last send outcome"));
    assert!(
        !text.contains("mxr send-outcome"),
        "CLI recovery requires an original owner absent from this response"
    );
}

#[test]
fn waiting_outcome_reports_automatic_retry_without_claiming_acceptance() {
    let text = super::format_platform_response(&mxr_protocol::ResponseData::SendOutcome {
        outcome: Some(mxr_core::DraftSendOutcome {
            identity: mxr_core::SendIdentity {
                operation_id: "synthetic:2".into(),
                expected_revision: 2,
            },
            account_id: AccountId::new(),
            draft_id: DraftId::new(),
            state: mxr_core::SendOutcomeState::Waiting,
            local_message_id: None,
            provider_message_id: None,
            rfc2822_message_id: Some("<synthetic@example.test>".into()),
            recovery: Some(mxr_core::SendRecoveryStatus {
                phase: mxr_core::SendSubmissionPhase::Cooldown,
                attempts: 1,
                retry_at: Some("2026-10-26T12:15:00Z".parse().unwrap()),
                reason: None,
            }),
        }),
    });
    assert!(text.contains("Waiting for the mail provider"), "{text}");
    assert!(text.contains("2026-10-26"), "{text}");
    assert!(!text.contains("Provider accepted"), "{text}");
    assert!(!text.contains("Acceptance uncertain"), "{text}");
}
