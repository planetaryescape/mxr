//! Phase 1.4: visible-first-batch render check.
//!
//! Streamed search keeps the UI responsive by rendering each batch as
//! soon as it lands, instead of gating on the final query completion.
//! This widget-level test pins that behavior: with `ui_status =
//! Searching` (the still-in-flight state) and a first-batch result
//! already applied to `search.page.results`, the rendered output
//! must show those rows.
//!
//! If we ever flipped to "wait for done before rendering rows", this
//! test would catch it.

use chrono::{TimeZone, Utc};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::types::{Address, Envelope, MessageFlags, UnsubscribeMethod};
use mxr_test_support::render_to_string;
use mxr_tui::app::{MailListMode, MailListRow, SearchPageState, SearchUiStatus};
use mxr_tui::ui::search_page::draw as draw_search_page;
use ratatui::layout::Rect;

fn streaming_envelope(slug: &str, subject: &str, sender: &str) -> Envelope {
    Envelope {
        id: MessageId::new(),
        account_id: AccountId::new(),
        provider_id: slug.into(),
        thread_id: ThreadId::new(),
        message_id_header: Some(format!("<{slug}@example.com>")),
        in_reply_to: None,
        references: vec![],
        from: Address {
            name: Some(sender.into()),
            email: format!("{slug}@example.com"),
        },
        to: vec![Address {
            name: Some("Me".into()),
            email: "me@example.com".into(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: subject.into(),
        date: Utc.with_ymd_and_hms(2024, 3, 15, 9, 30, 0).unwrap(),
        flags: MessageFlags::READ,
        snippet: "snippet".into(),
        has_attachments: false,
        size_bytes: 200,
        unsubscribe: UnsubscribeMethod::None,
        link_count: 0,
        body_word_count: 0,
        label_provider_ids: vec!["INBOX".into()],
    }
}

fn streaming_row(env: Envelope) -> MailListRow {
    MailListRow {
        thread_id: env.thread_id.clone(),
        representative: env,
        message_count: 1,
        unread_count: 0,
        other_participant_count: 0,
        open_commitment_count: 0,
        reply_later: false,
        pending_mutation: false,
    }
}

#[test]
fn result_list_renders_first_batch_before_query_completes() {
    // First batch landed; second batch still pending. `ui_status =
    // Searching` is the precondition: the request is in-flight.
    let envelope = streaming_envelope("first-batch", "Build report", "Alice");
    let state = SearchPageState {
        query: "report".into(),
        editing: false,
        results: vec![envelope.clone()],
        scores: Default::default(),
        mode: mxr_core::SearchMode::Lexical,
        sort: mxr_core::SortOrder::DateDesc,
        // has_more = true is the daemon's signal that more results
        // are still on the wire.
        has_more: true,
        loading_more: true,
        total_count: None,
        count_pending: true,
        ui_status: SearchUiStatus::Searching,
        session_active: true,
        load_to_end: false,
        session_id: 1,
        active_pane: mxr_tui::app::SearchPane::Results,
        preview_fullscreen: false,
        selected_index: 0,
        scroll_offset: 0,
        result_selected: false,
        throbber: Default::default(),
    };
    let rows = vec![streaming_row(envelope.clone())];

    let mut html_images = std::collections::HashMap::new();
    let rendered = render_to_string(160, 24, |frame| {
        draw_search_page(
            frame,
            Rect::new(0, 0, 160, 24),
            &state,
            &rows,
            &std::collections::HashSet::new(),
            MailListMode::Messages,
            &[],
            0,
            &mut html_images,
            &mxr_tui::theme::Theme::default(),
        );
    });

    // The sender and subject from the first batch are present in the
    // rendered output. If the page ever gated row rendering on
    // `ui_status == Loaded`, this would fail.
    assert!(
        rendered.contains("Alice") || rendered.contains("first-batch"),
        "first batch sender should be visible while the request is still streaming; got:\n{rendered}"
    );
    assert!(
        rendered.contains("Build report"),
        "first batch subject should be visible while the request is still streaming; got:\n{rendered}"
    );
}
