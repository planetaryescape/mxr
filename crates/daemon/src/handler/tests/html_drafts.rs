use super::*;

const DANGEROUS_HTML: &str = "<html><body><p>Hello</p><script>alert(1)</script></body></html>";

const SAFE_HTML: &str = "<html><body><p>Hello, the numbers are below.</p></body></html>";

const SECRET_IN_HTML: &str =
    "<html><body><p>the deploy key AKIAIOSFODNN7EXAMPLE ships today</p></body></html>";

fn html_draft(
    account_id: mxr_core::AccountId,
    html: &str,
    text: Option<&str>,
) -> mxr_core::types::Draft {
    mxr_core::types::Draft {
        revision: Some(1),
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
        managed_attachments: None,
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

fn request(id: u64, payload: Request) -> IpcMessage {
    IpcMessage {
        expected_workspace: None,
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
                    send_identity: Some(mxr_core::SendIdentity::for_draft(&draft)),
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

#[tokio::test]
async fn send_stored_draft_revalidates_a_row_written_before_the_gate_existed() {
    let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let draft = html_draft(state.default_account_id(), DANGEROUS_HTML, None);
    let id = draft.id.clone();
    state.store.insert_draft(&draft).await.unwrap();

    let message = expect_error(
        handle_request(
            &state,
            &request(
                1,
                Request::SendStoredDraft {
                    managed_attachments_reviewed: true,
                    send_identity: Some(mxr_core::SendIdentity::for_revision(&id, 1)),
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
                    send_identity: Some(mxr_core::SendIdentity::for_draft(&draft)),
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

async fn stored_html_draft(state: &Arc<AppState>, html: &str) -> mxr_core::DraftId {
    let draft = html_draft(state.default_account_id(), html, None);
    let id = draft.id.clone();
    expect_ok(handle_request(state, &request(1, Request::SaveDraft { draft })).await);
    id
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
async fn sending_an_html_only_draft_scans_the_document_for_secrets() {
    let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let draft = html_draft(state.default_account_id(), SECRET_IN_HTML, None);
    let id = draft.id.clone();
    state.store.insert_draft(&draft).await.unwrap();

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
                    managed_attachments_reviewed: true,
                    send_identity: Some(mxr_core::SendIdentity::for_revision(&id, 1)),
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
