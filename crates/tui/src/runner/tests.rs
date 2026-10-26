use super::{apply_all_envelopes_refresh, handle_daemon_event, run_with_terminal_suspended_with};
use crate::action::Action;
use crate::app::PendingSend;
use crate::app::{
    ActivePane, App, BodySource, BodyViewMetadata, BodyViewState, LayoutMode, MailListMode,
    MailboxView, MutationEffect, PendingSearchRequest, PendingSendMode, Screen, SearchPane,
    SearchTarget, SidebarItem, SEARCH_PAGE_SIZE,
};
use crate::async_result::{ComposeReadyData, SearchResultData};
use crate::compose_flow::{handle_compose_editor_status, pending_send_from_edited_draft};
use crate::input::InputHandler;
use crate::runtime::{enqueue_replaceable_request, ReplaceableRequest};
use crate::test_fixtures::TestEnvelopeBuilder;
use crate::ui::command_palette::default_commands;
use crate::ui::command_palette::CommandPalette;
use crate::ui::search_bar::SearchBar;
use crate::ui::status_bar;
use mxr_config::RenderConfig;
use mxr_core::id::*;
use mxr_core::types::*;
use mxr_core::MxrError;
use mxr_protocol::{BodyFailure, DaemonEvent, LabelCount, MutationCommand, Request};
use mxr_test_support::render_to_string;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::VecDeque;
use std::os::unix::process::ExitStatusExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio::time::Instant;

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

fn account_summary(
    account_id: AccountId,
    enabled: bool,
    is_default: bool,
) -> mxr_protocol::AccountSummaryData {
    mxr_protocol::AccountSummaryData {
        identity: None,
        account_id,
        key: Some("user".into()),
        name: "User".into(),
        email: "user@example.com".into(),
        provider_kind: "imap".into(),
        sync_kind: Some("imap".into()),
        send_kind: Some("smtp".into()),
        enabled,
        is_default,
        source: mxr_protocol::AccountSourceData::Config,
        editable: mxr_protocol::AccountEditModeData::Full,
        sync: None,
        send: None,
        capabilities: Default::default(),
    }
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

struct TestEventSource {
    id: usize,
    dropped: Arc<AtomicBool>,
}

impl Drop for TestEventSource {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::SeqCst);
    }
}

fn exit_status(code: i32) -> std::process::ExitStatus {
    std::process::ExitStatus::from_raw(code)
}

/// Helper for the dormant test: replicates the dormant logic
/// in mail_list_title without going through the title formatter.
fn row_to_dormant(row: &crate::app::MailListRow, days: i64) -> Option<String> {
    if row.message_count < 3 || days < 30 {
        return None;
    }
    Some(format!("Dormant {days}d. Press B for briefing"))
}
fn make_test_labels() -> Vec<Label> {
    crate::test_fixtures::test_system_labels(&AccountId::new())
}

/// Put `app` into an Inbox-active state so optimistic mutation effects
/// (which only fire when the active label matches the labels the
/// mutation removes) take effect during tests.
fn set_active_inbox(app: &mut App) {
    app.mailbox.labels = make_test_labels();
    app.mailbox.active_label = app
        .mailbox
        .labels
        .iter()
        .find(|label| label.name.eq_ignore_ascii_case("INBOX"))
        .map(|label| label.id.clone());
}

// --- Navigation tests ---

// --- Back navigation tests ---

// --- Sidebar tests ---

// --- GoTo navigation tests ---

// --- Mutation effect tests ---

// --- Mail list title tests ---

mod accounts_and_delivery;
mod assistant;
mod back_and_quit;
mod export;
mod input_and_compose;
mod keymap;
mod mailbox_views;
mod messages;
mod moves;
mod mutations_and_bulk;
mod now;
mod pattern_select;
mod places;
mod reader_and_diagnostics;
mod reading;
mod records;
mod semantic_and_connection;
mod todo;
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
