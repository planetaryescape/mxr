use super::*;

#[test]
fn stale_body_response_does_not_clobber_current_view() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(2);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::OpenSelected);
    let first = app.mailbox.envelopes[0].clone();
    app.mailbox.active_pane = ActivePane::MailList;
    app.apply(Action::MoveDown);
    let second = app.mailbox.envelopes[1].clone();

    app.resolve_body_success(MessageBody {
        message_id: first.id.clone(),
        text_plain: Some("Old body".into()),
        text_html: None,
        attachments: vec![],
        fetched_at: chrono::Utc::now(),
        metadata: Default::default(),
    });

    assert_eq!(
        app.mailbox
            .viewing_envelope
            .as_ref()
            .map(|env| env.id.clone()),
        Some(second.id)
    );
    assert!(matches!(
        app.mailbox.body_view_state,
        BodyViewState::Loading { ref preview }
            if preview.as_deref() == Some("Snippet 1")
    ));
}

#[test]
fn html_view_toggle_updates_mode_and_remote_content_status() {
    let mut app = App::new();
    app.mailbox.html_view = true;
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    let env = app.mailbox.envelopes[0].clone();
    app.mailbox.body_cache.insert(
        env.id.clone(),
        MessageBody {
            message_id: env.id.clone(),
            text_plain: Some("Fallback plain".into()),
            text_html: Some(
                "<p>Hello <img alt=\"Hero\" src=\"https://example.com/hero.png\"></p>".into(),
            ),
            attachments: vec![AttachmentMeta {
                id: AttachmentId::new(),
                message_id: env.id.clone(),
                filename: "logo.png".into(),
                mime_type: "image/png".into(),
                disposition: AttachmentDisposition::Inline,
                content_id: Some("logo@example.com".into()),
                content_location: None,
                size_bytes: 2048,
                local_path: None,
                provider_id: "att-inline".into(),
            }],
            fetched_at: chrono::Utc::now(),
            metadata: MessageMetadata {
                text_plain_source: Some(BodyPartSource::Exact),
                text_html_source: Some(BodyPartSource::Exact),
                ..Default::default()
            },
        },
    );

    app.apply(Action::OpenSelected);

    match &app.mailbox.body_view_state {
        BodyViewState::Ready {
            source: BodySource::Html,
            metadata,
            ..
        } => {
            assert_eq!(metadata.mode, crate::app::BodyViewMode::Html);
            assert!(metadata.inline_images);
            assert!(metadata.remote_content_available);
            assert!(metadata.remote_content_enabled);
        }
        other => panic!("expected html ready state, got {other:?}"),
    }

    app.apply(Action::ToggleHtmlView);

    match &app.mailbox.body_view_state {
        BodyViewState::Ready {
            source: BodySource::Plain,
            metadata,
            ..
        } => {
            assert_eq!(metadata.mode, crate::app::BodyViewMode::Text);
            assert!(metadata.inline_images);
            assert!(metadata.remote_content_available);
            assert!(metadata.remote_content_enabled);
        }
        other => panic!("expected text ready state, got {other:?}"),
    }
    assert_eq!(app.status_message.as_deref(), Some("View: Reading"));
    assert!(app
        .status_bar_state()
        .body_status
        .as_deref()
        .is_some_and(|status| status.contains("View: Reading")));

    app.apply(Action::ToggleRemoteContent);

    match &app.mailbox.body_view_state {
        BodyViewState::Ready { metadata, .. } => {
            assert_eq!(metadata.mode, crate::app::BodyViewMode::Text);
            assert!(!metadata.remote_content_enabled);
        }
        other => panic!("expected text ready state, got {other:?}"),
    }
    assert_eq!(
        app.status_message.as_deref(),
        Some("Remote images blocked in HTML view")
    );
    assert!(app
        .status_bar_state()
        .body_status
        .as_deref()
        .is_some_and(|status| status.contains("View: Reading")));
}

#[test]
fn account_switch_complete_closes_open_message_state() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(2);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.mailbox.mailbox_view = MailboxView::Subscriptions;
    app.mailbox.layout_mode = LayoutMode::FullScreen;
    app.mailbox.active_pane = ActivePane::MessageView;
    app.mailbox.viewing_envelope = Some(app.mailbox.envelopes[0].clone());
    app.mailbox.viewed_thread_messages = app.mailbox.envelopes.clone();
    app.mailbox.body_view_state = BodyViewState::ready(
        "hello".into(),
        "hello".into(),
        BodySource::Plain,
        BodyViewMetadata::default(),
    );
    app.mailbox.active_label = Some(LabelId::new());
    app.mailbox.pending_active_label = Some(LabelId::new());
    app.mailbox.pending_label_fetch = Some(LabelId::new());
    app.mailbox
        .selected_set
        .insert(app.mailbox.envelopes[0].id.clone());

    app.handle_account_switch_complete();

    assert!(app.mailbox.viewing_envelope.is_none());
    assert!(app.mailbox.viewed_thread_messages.is_empty());
    assert!(matches!(
        app.mailbox.body_view_state,
        BodyViewState::Empty { .. }
    ));
    assert_eq!(app.mailbox.mailbox_view, MailboxView::Messages);
    assert_eq!(app.mailbox.layout_mode, LayoutMode::TwoPane);
    assert_eq!(app.mailbox.active_pane, ActivePane::MailList);
    assert!(app.mailbox.envelopes.is_empty());
    assert!(app.mailbox.all_envelopes.is_empty());
    assert!(app.search.page.results.is_empty());
    assert!(app.mailbox.subscriptions_page.entries.is_empty());
    assert!(app.mailbox.selected_set.is_empty());
    assert!(app.mailbox.active_label.is_none());
    assert!(app.mailbox.pending_active_label.is_none());
    assert!(app.mailbox.pending_label_fetch.is_none());
    assert!(app.mailbox.pending_labels_refresh);
    assert!(app.mailbox.pending_all_envelopes_refresh);
    assert!(app.mailbox.pending_subscriptions_refresh);
    assert!(app.diagnostics.pending_status_refresh);
    assert_eq!(
        app.mailbox.mailbox_loading_message.as_deref(),
        Some("Loading selected account...")
    );
    assert_eq!(app.mailbox.desired_system_mailbox.as_deref(), Some("INBOX"));
}

#[test]
fn proof_accept_calls_apply_then_reloads_the_draft_for_safety_review() {
    let mut app = App::new();
    let pending = PendingSend {
        draft_revision: None,
        account_id: AccountId::new(),
        fm: mxr_compose::frontmatter::ComposeFrontmatter {
            to: "sender@example.com".into(),
            cc: String::new(),
            bcc: String::new(),
            subject: "Quarterly plan".into(),
            from: "me@example.com".into(),
            in_reply_to: None,
            intent: mxr_core::DraftIntent::New,
            references: vec![],
            thread_id: None,
            attach: vec![],
            signature: None,
            forward_source: None,
        },
        body: "Original text.".into(),
        draft_path: std::path::PathBuf::from("/tmp/mxr-proof-accept.md"),
        intent: mxr_core::DraftIntent::New,
        mode: PendingSendMode::SendOrSave,
        safety_report: None,
        safety_check_failed: None,
        override_token: None,
        suggested_collaborators: vec![],
        invite_reply: None,
        resolved_from: None,
        stored_draft: None,
    };
    app.compose.pending_send_confirm = Some(pending.clone());
    app.compose.assist_review = Some(crate::app::ComposeAssistReview {
        pending,
        proposals: vec![test_compose_proposal()],
        proposal_models: vec![None; 1],
        current_index: 0,
        note_input: None,
        proof_scroll_offset: 0,
        voice_source_index: 0,
        request_pending: false,
    });

    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('y'),
        crossterm::event::KeyModifiers::NONE,
    ));

    let queued = app.take_pending_platform_dispatch();
    assert_eq!(queued.len(), 1);
    assert!(
        matches!(queued[0].prelude.as_slice(), [Request::ApplyComposeProposal { proposal_id, .. }] if proposal_id == "proposal-1")
    );
    assert!(matches!(queued[0].request, Request::GetDraft { .. }));
    assert!(queued[0].compose_assist_accept.is_some());
    assert_eq!(queued[0].title, "Accepting writing proof");
}

fn test_compose_proposal() -> mxr_protocol::ComposeProposalData {
    mxr_protocol::ComposeProposalData {
        proposal_id: "proposal-1".into(),
        action: mxr_protocol::AssistActionData::Polish,
        base: "base".into(),
        text: "Original text, made clearer.".into(),
        changes: vec![],
        gaps: vec![],
        provenance: mxr_protocol::DraftProvenanceData {
            model: "test".into(),
            locality: mxr_protocol::AiLocalityData::Local,
            history_used: false,
            voice_examples: vec![],
            conversation: vec![],
            rewrite: None,
        },
        yours_changed: vec![],
        composer_version: "c1".into(),
        style_marks: Vec::new(),
        style_score: 100,
        style_version: String::new(),
    }
}
