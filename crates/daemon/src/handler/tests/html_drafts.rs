//! HTML drafts at the daemon boundary.
//!
//! Two promises are under test. First, the validation gate lives in the daemon,
//! so an IPC client speaking the socket directly cannot route around it — and a
//! refused document is reported back unchanged rather than sanitised into
//! something sendable. Second, an HTML draft that carries no `text/plain`
//! alternative still reaches the safety pipeline with real text: without that,
//! `DraftContent::analysis_text()` is `""` and every check (PII, secrets, tone,
//! commitments) passes by examining nothing.

use super::*;

/// The canonical piece of active content: reported and refused, never stripped.
const DANGEROUS_HTML: &str = "<html><body><p>Hello</p><script>alert(1)</script></body></html>";

const SAFE_HTML: &str = "<html><body><p>Hello, the numbers are below.</p></body></html>";

/// An AWS access key id that appears **only** inside the HTML — not in the
/// subject, and with no supplied `text/plain` alternative. `mxr_safety`'s PII
/// detector reads `DraftContent::analysis_text()`, which is the empty string for
/// an HTML-only draft, so this secret is detectable only once the daemon has
/// materialised a text alternative from the document.
const SECRET_IN_HTML: &str =
    "<html><body><p>the deploy key AKIAIOSFODNN7EXAMPLE ships today</p></body></html>";

fn html_draft(
    account_id: mxr_core::AccountId,
    html: &str,
    text: Option<&str>,
) -> mxr_core::types::Draft {
    mxr_core::types::Draft {
        id: mxr_core::DraftId::new(),
        account_id,
        from: None,
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![mxr_core::types::Address {
            name: None,
            email: "alice@example.com".to_string(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Quarterly numbers".to_string(),
        content: mxr_core::types::DraftContent::html(html, text.map(str::to_string)),
        inline_assets: Vec::new(),
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

fn request(id: u64, payload: Request) -> IpcMessage {
    IpcMessage {
        id,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(payload),
    }
}

fn expect_error(resp: IpcMessage) -> String {
    match resp.payload {
        IpcPayload::Response(Response::Error { message, .. }) => message,
        other => panic!("expected an Error response, got {other:?}"),
    }
}

fn expect_ok(resp: IpcMessage) -> ResponseData {
    match resp.payload {
        IpcPayload::Response(Response::Ok { data }) => data,
        other => panic!("expected an Ok response, got {other:?}"),
    }
}

/// The stored HTML for a draft; panics if the row is gone or is not HTML.
async fn stored_html(state: &AppState, id: &mxr_core::DraftId) -> String {
    let draft = state
        .store
        .get_draft(id)
        .await
        .expect("store read")
        .expect("draft should still exist");
    match draft.content {
        mxr_core::types::DraftContent::Html { html, .. } => html,
        other => panic!("expected an HTML draft, got {other:?}"),
    }
}

fn assert_reports_the_script(message: &str) {
    assert!(
        message.contains("script"),
        "the refusal must name the offending construct; got: {message}"
    );
}

// ---------------------------------------------------------------------------
// The validation gate is in the daemon, on every entry point.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn save_draft_refuses_dangerous_html_and_stores_nothing() {
    let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let draft = html_draft(state.default_account_id(), DANGEROUS_HTML, None);
    let id = draft.id.clone();

    let message =
        expect_error(handle_request(&state, &request(1, Request::SaveDraft { draft })).await);

    assert_reports_the_script(&message);
    assert!(
        state.store.get_draft(&id).await.unwrap().is_none(),
        "a refused draft must not be stored"
    );
    assert!(fake.sent_drafts().is_empty(), "a save must never transmit");
}

#[tokio::test]
async fn update_draft_refuses_dangerous_html_and_leaves_the_stored_document_untouched() {
    let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let draft = html_draft(state.default_account_id(), SAFE_HTML, None);
    let id = draft.id.clone();
    expect_ok(
        handle_request(
            &state,
            &request(
                1,
                Request::SaveDraft {
                    draft: draft.clone(),
                },
            ),
        )
        .await,
    );

    let mut edited = draft;
    edited.content = mxr_core::types::DraftContent::html(DANGEROUS_HTML, None);
    let message = expect_error(
        handle_request(&state, &request(2, Request::UpdateDraft { draft: edited })).await,
    );

    assert_reports_the_script(&message);
    assert_eq!(
        stored_html(&state, &id).await,
        SAFE_HTML,
        "a refused update must leave the previously stored document exactly as it was"
    );
    assert!(
        fake.sent_drafts().is_empty(),
        "an update must never transmit"
    );
}

#[tokio::test]
async fn save_draft_to_server_refuses_dangerous_html() {
    let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let draft = html_draft(state.default_account_id(), DANGEROUS_HTML, None);

    let message = expect_error(
        handle_request(&state, &request(1, Request::SaveDraftToServer { draft })).await,
    );

    assert_reports_the_script(&message);
    assert!(fake.sent_drafts().is_empty());
}

#[tokio::test]
async fn send_draft_refuses_dangerous_html_before_it_reaches_a_provider() {
    let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let draft = html_draft(state.default_account_id(), DANGEROUS_HTML, None);
    let id = draft.id.clone();

    let message = expect_error(
        handle_request(
            &state,
            &request(
                1,
                Request::SendDraft {
                    draft,
                    override_safety_token: None,
                },
            ),
        )
        .await,
    );

    assert_reports_the_script(&message);
    assert!(
        fake.sent_drafts().is_empty(),
        "nothing may reach the provider"
    );
    assert!(
        state.store.get_draft(&id).await.unwrap().is_none(),
        "a refused send must not leave a draft row behind"
    );
}

/// The row was written by a binary that predates the gate. Loading it must not
/// be a way past validation, and the refusal must not rewrite the row.
#[tokio::test]
async fn send_stored_draft_revalidates_a_row_written_before_the_gate_existed() {
    let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let draft = html_draft(state.default_account_id(), DANGEROUS_HTML, None);
    let id = draft.id.clone();
    // Straight into the store, bypassing the handler entirely.
    state.store.insert_draft(&draft).await.unwrap();

    let message = expect_error(
        handle_request(
            &state,
            &request(
                1,
                Request::SendStoredDraft {
                    draft_id: id.clone(),
                    override_safety_token: None,
                },
            ),
        )
        .await,
    );

    assert_reports_the_script(&message);
    assert!(
        fake.sent_drafts().is_empty(),
        "nothing may reach the provider"
    );
    assert_eq!(
        stored_html(&state, &id).await,
        DANGEROUS_HTML,
        "refusing must not sanitise the stored document"
    );
}

/// `inline_assets` is documented as always empty on a markdown draft, but the
/// wire shape allows it and the store persists it. A cid carrying CRLF would
/// forge a `Content-ID` header, so the check cannot sit behind the HTML branch.
#[tokio::test]
async fn a_cid_that_could_forge_a_header_is_refused_on_a_markdown_draft_too() {
    let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let mut draft = html_draft(state.default_account_id(), SAFE_HTML, None);
    draft.content = mxr_core::types::DraftContent::markdown("plain body");
    draft.inline_assets = vec![mxr_core::types::InlineAsset {
        cid: "logo\r\nBcc: attacker@example.com".to_string(),
        path: std::path::PathBuf::from("/tmp/logo.png"),
    }];
    let id = draft.id.clone();

    let message = expect_error(
        handle_request(
            &state,
            &request(
                1,
                Request::SendDraft {
                    draft,
                    override_safety_token: None,
                },
            ),
        )
        .await,
    );

    assert!(
        message.contains("invalid content id"),
        "the refusal must name the bad content id; got: {message}"
    );
    assert!(
        fake.sent_drafts().is_empty(),
        "nothing may reach the provider"
    );
    assert!(
        state.store.get_draft(&id).await.unwrap().is_none(),
        "a refused send must not leave a draft row behind"
    );
}

#[tokio::test]
async fn duplicate_inline_cids_are_refused() {
    let (state, _fake) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let mut draft = html_draft(state.default_account_id(), SAFE_HTML, None);
    draft.inline_assets = vec![
        mxr_core::types::InlineAsset {
            cid: "logo".to_string(),
            path: std::path::PathBuf::from("/tmp/logo.png"),
        },
        mxr_core::types::InlineAsset {
            cid: "logo".to_string(),
            path: std::path::PathBuf::from("/tmp/other.png"),
        },
    ];

    let message =
        expect_error(handle_request(&state, &request(1, Request::SaveDraft { draft })).await);

    assert!(message.contains("duplicate content id"), "got: {message}");
}

// ---------------------------------------------------------------------------
// An update may not change a draft's body kind.
// ---------------------------------------------------------------------------

/// Save an HTML draft and return its id.
async fn stored_html_draft(state: &Arc<AppState>, html: &str) -> mxr_core::DraftId {
    let draft = html_draft(state.default_account_id(), html, None);
    let id = draft.id.clone();
    expect_ok(handle_request(state, &request(1, Request::SaveDraft { draft })).await);
    id
}

/// Round-trip a stored draft through JSON with the named keys removed, the way
/// a client that simply does not model those fields would send it back.
async fn draft_without_keys(
    state: &Arc<AppState>,
    id: &mxr_core::DraftId,
    keys: &[&str],
) -> mxr_core::types::Draft {
    try_draft_without_keys(state, id, keys)
        .await
        .expect("payload should decode")
}

/// The fallible form: some key removals are refused by `DraftContent`'s own
/// deserialiser rather than by a handler, and a test asserting that needs the
/// error rather than a panic.
async fn try_draft_without_keys(
    state: &Arc<AppState>,
    id: &mxr_core::DraftId,
    keys: &[&str],
) -> serde_json::Result<mxr_core::types::Draft> {
    let stored = state.store.get_draft(id).await.unwrap().unwrap();
    let mut payload = serde_json::to_value(&stored).unwrap();
    let object = payload
        .as_object_mut()
        .expect("draft serialises to an object");
    for key in keys {
        object.remove(*key);
    }
    serde_json::from_value(payload)
}

/// The data-loss vector: an update payload with no body fields at all
/// deserialises to an empty markdown body, which would overwrite the supplied
/// document with `""`.
#[tokio::test]
async fn update_draft_refuses_a_payload_that_omits_the_body_entirely() {
    let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let id = stored_html_draft(&state, SAFE_HTML).await;

    let edited = draft_without_keys(&state, &id, &["body_html", "body_text"]).await;
    // Premise: this is what the permissive deserialiser produces.
    assert_eq!(
        edited.content,
        mxr_core::types::DraftContent::markdown(""),
        "a payload with no body fields should decode to empty markdown"
    );

    let message = expect_error(
        handle_request(&state, &request(2, Request::UpdateDraft { draft: edited })).await,
    );

    assert!(
        message.contains("cannot change it to markdown"),
        "the refusal must say what it refused; got: {message}"
    );
    assert_eq!(
        stored_html(&state, &id).await,
        SAFE_HTML,
        "the supplied document must survive a refused update byte for byte"
    );
    assert!(fake.sent_drafts().is_empty());
}

/// Same root cause, second shape: `body_text` without `body_html`. This one is
/// now stopped a layer earlier — `DraftContent`'s deserialiser refuses it — so
/// the payload never becomes a `Draft` for `update_draft` to judge. The
/// assertion follows the guard to where it actually lives; asserting on
/// `update_draft` instead would only prove that the wire never reaches it.
#[tokio::test]
async fn a_payload_that_supplies_only_the_text_alternative_never_decodes() {
    let (state, _fake) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let id = stored_html_draft(&state, SAFE_HTML).await;

    let error = try_draft_without_keys(&state, &id, &["body_html"])
        .await
        .expect_err("body_text with no body_html must not decode to a Draft");

    assert!(
        error.to_string().contains("body_text without body_html"),
        "the refusal must name what was wrong; got: {error}"
    );
    assert_eq!(stored_html(&state, &id).await, SAFE_HTML);
}

#[tokio::test]
async fn update_draft_refuses_an_explicit_markdown_body_for_an_html_draft() {
    let (state, _fake) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let id = stored_html_draft(&state, SAFE_HTML).await;

    let mut edited = state.store.get_draft(&id).await.unwrap().unwrap();
    edited.content = mxr_core::types::DraftContent::markdown("plain replacement");

    let message = expect_error(
        handle_request(&state, &request(2, Request::UpdateDraft { draft: edited })).await,
    );

    assert!(
        message.contains("cannot change it to markdown"),
        "{message}"
    );
    assert_eq!(stored_html(&state, &id).await, SAFE_HTML);
}

/// The mirror direction is refused too: one rule, no asymmetry to reason about.
#[tokio::test]
async fn update_draft_refuses_an_html_body_for_a_markdown_draft() {
    let (state, _fake) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let mut draft = html_draft(state.default_account_id(), SAFE_HTML, None);
    draft.content = mxr_core::types::DraftContent::markdown("plain original");
    let id = draft.id.clone();
    expect_ok(handle_request(&state, &request(1, Request::SaveDraft { draft })).await);

    let mut edited = state.store.get_draft(&id).await.unwrap().unwrap();
    edited.content = mxr_core::types::DraftContent::html(SAFE_HTML, None);

    let message = expect_error(
        handle_request(&state, &request(2, Request::UpdateDraft { draft: edited })).await,
    );

    assert!(message.contains("cannot change it to html"), "{message}");
    let stored = state.store.get_draft(&id).await.unwrap().unwrap();
    assert_eq!(stored.content.markdown_source(), Some("plain original"));
}

#[tokio::test]
async fn update_draft_accepts_a_new_html_document_for_an_html_draft() {
    let (state, _fake) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let id = stored_html_draft(&state, SAFE_HTML).await;
    let revised = "<html><body><p>Revised, still HTML.</p></body></html>";

    let mut edited = state.store.get_draft(&id).await.unwrap().unwrap();
    edited.content = mxr_core::types::DraftContent::html(revised, None);
    expect_ok(handle_request(&state, &request(2, Request::UpdateDraft { draft: edited })).await);

    assert_eq!(stored_html(&state, &id).await, revised);
}

#[tokio::test]
async fn update_draft_accepts_a_new_markdown_body_for_a_markdown_draft() {
    let (state, _fake) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let mut draft = html_draft(state.default_account_id(), SAFE_HTML, None);
    draft.content = mxr_core::types::DraftContent::markdown("first");
    let id = draft.id.clone();
    expect_ok(handle_request(&state, &request(1, Request::SaveDraft { draft })).await);

    let mut edited = state.store.get_draft(&id).await.unwrap().unwrap();
    edited.content = mxr_core::types::DraftContent::markdown("second");
    expect_ok(handle_request(&state, &request(2, Request::UpdateDraft { draft: edited })).await);

    let stored = state.store.get_draft(&id).await.unwrap().unwrap();
    assert_eq!(stored.content.markdown_source(), Some("second"));
}

/// The common case the guard must not break: editing everything except the body.
#[tokio::test]
async fn update_draft_accepts_metadata_changes_that_leave_an_html_body_alone() {
    let (state, _fake) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let id = stored_html_draft(&state, SAFE_HTML).await;

    let mut edited = state.store.get_draft(&id).await.unwrap().unwrap();
    edited.subject = "Quarterly numbers (final)".to_string();
    edited.to = vec![mxr_core::types::Address {
        name: None,
        email: "bob@example.com".to_string(),
    }];
    edited.cc = vec![mxr_core::types::Address {
        name: None,
        email: "carol@example.com".to_string(),
    }];
    edited.attachments = vec![std::path::PathBuf::from("/tmp/notes.txt")];
    expect_ok(handle_request(&state, &request(2, Request::UpdateDraft { draft: edited })).await);

    let stored = state.store.get_draft(&id).await.unwrap().unwrap();
    assert_eq!(stored.subject, "Quarterly numbers (final)");
    assert_eq!(stored.to[0].email, "bob@example.com");
    assert_eq!(stored.cc[0].email, "carol@example.com");
    assert_eq!(stored.attachments.len(), 1);
    assert_eq!(
        stored_html(&state, &id).await,
        SAFE_HTML,
        "a metadata edit must not disturb the document"
    );
}

// ---------------------------------------------------------------------------
// The safety pipeline must never analyse an empty body.
// ---------------------------------------------------------------------------

/// An HTML-only draft with no supplied `text/plain` must still be scanned. The
/// secret lives only in the HTML, so a send that goes through means safety read
/// nothing at all.
#[tokio::test]
async fn sending_an_html_only_draft_scans_the_document_for_secrets() {
    let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let draft = html_draft(state.default_account_id(), SECRET_IN_HTML, None);
    let id = draft.id.clone();
    state.store.insert_draft(&draft).await.unwrap();

    // Guard the premise: if the store itself filled in a text alternative, this
    // test would pass for the wrong reason.
    let loaded = state.store.get_draft(&id).await.unwrap().unwrap();
    assert_eq!(
        loaded.content,
        mxr_core::types::DraftContent::html(SECRET_IN_HTML, None),
        "the stored row must have no text/plain alternative for this test to mean anything"
    );
    assert_eq!(
        loaded.content.analysis_text(),
        "",
        "an HTML-only draft has no analysis text of its own"
    );

    let message = expect_error(
        handle_request(
            &state,
            &request(
                1,
                Request::SendStoredDraft {
                    draft_id: id,
                    override_safety_token: None,
                },
            ),
        )
        .await,
    );

    assert!(
        message.contains("AWS access key id"),
        "safety must read the HTML document; got: {message}"
    );
    assert!(
        fake.sent_drafts().is_empty(),
        "a blocked draft must not be sent"
    );
}

/// The blank-alternative variant, and the more dangerous one: the row carries
/// `text: Some("")`, which is "supplied" as far as every consumer is concerned.
/// `materialize_text_alternative` used to match only `None`, so this row kept
/// its useless alternative and safety scanned an empty string — a live
/// credential in the document, and a green verdict.
#[tokio::test]
async fn sending_an_html_draft_with_a_blank_text_alternative_still_scans_the_document() {
    let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let draft = html_draft(state.default_account_id(), SECRET_IN_HTML, Some("   \n"));
    let id = draft.id.clone();
    // Straight into the store: this is the shape an older binary, or any IPC
    // client that skips the CLI, leaves behind.
    state.store.insert_draft(&draft).await.unwrap();

    // Guard the premise: the row must really come back blank, or this test
    // would pass for the wrong reason.
    let loaded = state.store.get_draft(&id).await.unwrap().unwrap();
    assert!(
        loaded.content.analysis_text().trim().is_empty(),
        "the stored row must have a blank text alternative; got {:?}",
        loaded.content
    );

    let message = expect_error(
        handle_request(
            &state,
            &request(
                1,
                Request::SendStoredDraft {
                    draft_id: id,
                    override_safety_token: None,
                },
            ),
        )
        .await,
    );

    assert!(
        message.contains("AWS access key id"),
        "safety must read the HTML document, not the blank alternative; got: {message}"
    );
    assert!(
        fake.sent_drafts().is_empty(),
        "a blocked draft must not be sent"
    );
}

/// The same seam through the other entry point: `CheckDraftSafety` builds its
/// verdict from a transient draft that was never stored.
#[tokio::test]
async fn check_draft_safety_scans_an_html_only_draft() {
    let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let draft = html_draft(state.default_account_id(), SECRET_IN_HTML, None);

    let data = expect_ok(
        handle_request(
            &state,
            &request(
                1,
                Request::CheckDraftSafety {
                    draft,
                    context: mxr_protocol::DraftSafetyContextData::default(),
                },
            ),
        )
        .await,
    );

    let report = match data {
        ResponseData::DraftSafetyReportResponse { report } => report,
        other => panic!("expected a safety report, got {other:?}"),
    };
    assert!(
        !report.allowed,
        "a secret in the HTML must block; report: {report:?}"
    );
    assert!(
        report.issues.iter().any(|issue| {
            issue.code == mxr_core::types::DraftSafetyIssueCode::PiiSecret
                && issue.severity == mxr_core::types::DraftSafetySeverity::Blocker
        }),
        "expected a PiiSecret blocker; report: {report:?}"
    );
    assert!(
        !serde_json::to_string(&report)
            .unwrap()
            .contains("AKIAIOSFODNN7EXAMPLE"),
        "the safety report echoed the raw secret back"
    );
    assert!(fake.sent_drafts().is_empty(), "a check must never transmit");
}

// ---------------------------------------------------------------------------
// No-send proofs for the ordinary, valid HTML paths.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn saving_and_updating_a_valid_html_draft_transmits_nothing() {
    let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let draft = html_draft(state.default_account_id(), SAFE_HTML, None);
    let id = draft.id.clone();

    expect_ok(
        handle_request(
            &state,
            &request(
                1,
                Request::SaveDraft {
                    draft: draft.clone(),
                },
            ),
        )
        .await,
    );
    assert!(fake.sent_drafts().is_empty(), "a save must never transmit");

    let mut edited = draft;
    edited.subject = "Quarterly numbers (v2)".to_string();
    expect_ok(handle_request(&state, &request(2, Request::UpdateDraft { draft: edited })).await);
    assert!(
        fake.sent_drafts().is_empty(),
        "an update must never transmit"
    );

    let stored = state.store.get_draft(&id).await.unwrap().unwrap();
    assert_eq!(stored.subject, "Quarterly numbers (v2)");
    assert_eq!(
        stored.content.markdown_source(),
        None,
        "an HTML draft must not be stored as markdown"
    );
}

/// A stored HTML draft carries a text alternative even though the caller
/// supplied none, so downstream readers (safety, preview, export, TUI) are not
/// looking at an empty body.
#[tokio::test]
async fn a_saved_html_draft_carries_a_generated_text_alternative() {
    let (state, _fake) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let draft = html_draft(state.default_account_id(), SAFE_HTML, None);
    let id = draft.id.clone();

    expect_ok(handle_request(&state, &request(1, Request::SaveDraft { draft })).await);

    let stored = state.store.get_draft(&id).await.unwrap().unwrap();
    assert!(
        stored.content.analysis_text().contains("numbers are below"),
        "stored draft has no usable analysis text: {:?}",
        stored.content
    );
    assert_eq!(
        stored_html(&state, &id).await,
        SAFE_HTML,
        "generating the alternative must not touch the HTML"
    );
}
