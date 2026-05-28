use super::*;

#[tokio::test]
async fn dispatch_get_body_synthesizes_readable_summary_for_calendar_only_messages() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;

    let stored = mxr_core::types::MessageBody {
        message_id: id.clone(),
        text_plain: None,
        text_html: None,
        attachments: vec![mxr_core::types::AttachmentMeta {
            id: mxr_core::AttachmentId::new(),
            message_id: id.clone(),
            filename: "invite.ics".into(),
            mime_type: "text/calendar".into(),
            disposition: mxr_core::types::AttachmentDisposition::Attachment,
            content_id: None,
            content_location: None,
            size_bytes: 2048,
            local_path: None,
            provider_id: "att-calendar".into(),
        }],
        fetched_at: chrono::Utc::now(),
        metadata: mxr_core::types::MessageMetadata {
            calendar: Some(mxr_core::types::CalendarMetadata {
                method: Some("REQUEST".into()),
                summary: Some("Demo call".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
    };
    state.store.insert_body(&stored).await.unwrap();

    let msg = IpcMessage {
        id: 17,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetBody {
            message_id: id.clone(),
        }),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Body { body },
        }) => {
            let text = body
                .text_plain
                .expect("calendar-only body should be synthesized");
            assert!(text.contains("Calendar invite"));
            assert!(text.contains("Summary: Demo call"));
            assert!(text.contains("invite.ics"));
        }
        other => panic!("Expected Body, got {other:?}"),
    }

    let repaired = state.store.get_body(&id).await.unwrap().unwrap();
    assert!(repaired
        .text_plain
        .as_deref()
        .is_some_and(|text| text.contains("Calendar invite")));
}

#[tokio::test]
async fn dispatch_respond_invite_dry_run_builds_imip_preview_without_sending() {
    let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
    let message_id = insert_request_invite_body(&state).await;
    let state = Arc::new(state);

    let resp = handle_request(
        &state,
        &IpcMessage {
            id: 18,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::RespondInvite {
                message_id: message_id.clone(),
                action: mxr_protocol::CalendarInviteActionData::Accept,
                dry_run: true,
            }),
        },
    )
    .await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::InviteResponsePreview { preview },
        }) => {
            assert_eq!(preview.message_id, message_id);
            assert_eq!(preview.organizer_email, "organizer@example.com");
            assert_eq!(preview.attendee_email, "user@example.com");
            assert!(preview.subject.contains("Accepted"));
            assert!(preview.ics.contains("METHOD:REPLY"));
            assert!(preview.ics.contains("UID:planning-uid@example.com"));
            assert!(preview.ics.contains("SEQUENCE:3"));
            assert!(preview.ics.contains("PARTSTAT=ACCEPTED"));
        }
        other => panic!("Expected InviteResponsePreview, got {other:?}"),
    }
    assert!(fake.sent_drafts().is_empty());
}

#[tokio::test]
async fn dispatch_respond_invite_sends_reply_and_updates_local_partstat() {
    let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
    let message_id = insert_request_invite_body(&state).await;
    let state = Arc::new(state);

    let resp = handle_request(
        &state,
        &IpcMessage {
            id: 19,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::RespondInvite {
                message_id: message_id.clone(),
                action: mxr_protocol::CalendarInviteActionData::Decline,
                dry_run: false,
            }),
        },
    )
    .await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::InviteResponseSent { result },
        }) => {
            assert_eq!(result.message_id, message_id);
            assert_eq!(
                result.action,
                mxr_protocol::CalendarInviteActionData::Decline
            );
            assert!(result
                .provider_message_id
                .as_deref()
                .is_some_and(|id| id.starts_with("fake-calendar-sent-")));
        }
        other => panic!("Expected InviteResponseSent, got {other:?}"),
    }

    let sent = fake.sent_drafts();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].to[0].email, "organizer@example.com");
    assert!(sent[0].body_markdown.contains("METHOD:REPLY"));
    assert!(sent[0].body_markdown.contains("PARTSTAT=DECLINED"));

    let stored = state
        .store
        .get_calendar_invite_for_message(&message_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.metadata.attendees[0].partstat.as_deref(),
        Some("DECLINED")
    );
}

#[tokio::test]
async fn dispatch_respond_invite_matches_account_alias_attendee() {
    let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
    let account_id = state.default_account_id_opt().unwrap();
    state
        .store
        .add_account_address(&account_id, "alias@example.com", false)
        .await
        .unwrap();
    let envelope = crate::test_fixtures::TestEnvelopeBuilder::new()
        .account_id(account_id)
        .provider_id("calendar-alias")
        .subject("Alias invite")
        .sender_address("Organizer", "organizer@example.com")
        .recipient_address(Some("Alias"), "alias@example.com")
        .has_attachments(true)
        .build();
    let message_id = envelope.id.clone();
    state.store.upsert_envelope(&envelope).await.unwrap();
    state
        .store
        .insert_body(&mxr_core::types::MessageBody {
            message_id: message_id.clone(),
            text_plain: Some("Alias invite".into()),
            text_html: None,
            attachments: Vec::new(),
            fetched_at: chrono::Utc::now(),
            metadata: mxr_core::types::MessageMetadata {
                calendar: Some(mxr_core::types::CalendarMetadata {
                    method: Some("REQUEST".into()),
                    summary: Some("Alias invite".into()),
                    uid: Some("alias-uid@example.com".into()),
                    organizer: Some(mxr_core::types::CalendarPerson {
                        email: "organizer@example.com".into(),
                        name: Some("Organizer".into()),
                        uri: Some("mailto:organizer@example.com".into()),
                    }),
                    attendees: vec![mxr_core::types::CalendarAttendee {
                        email: "alias@example.com".into(),
                        name: Some("Alias".into()),
                        uri: Some("mailto:alias@example.com".into()),
                        partstat: Some("NEEDS-ACTION".into()),
                        role: Some("REQ-PARTICIPANT".into()),
                        rsvp: Some(true),
                    }],
                    ..Default::default()
                }),
                ..Default::default()
            },
        })
        .await
        .unwrap();
    let state = Arc::new(state);

    let resp = handle_request(
        &state,
        &IpcMessage {
            id: 20,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::RespondInvite {
                message_id: message_id.clone(),
                action: mxr_protocol::CalendarInviteActionData::Accept,
                dry_run: false,
            }),
        },
    )
    .await;

    assert!(matches!(
        resp.payload,
        IpcPayload::Response(Response::Ok {
            data: ResponseData::InviteResponseSent { .. }
        })
    ));
    assert_eq!(fake.sent_drafts().len(), 1);
    let stored = state
        .store
        .get_calendar_invite_for_message(&message_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.metadata.attendees[0].partstat.as_deref(),
        Some("ACCEPTED")
    );
}

#[tokio::test]
async fn dispatch_respond_invite_blocks_stale_sequence_when_newer_invite_exists() {
    let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
    let stale_message_id = insert_request_invite_body(&state).await;
    let account_id = state.default_account_id_opt().unwrap();
    let newer = crate::test_fixtures::TestEnvelopeBuilder::new()
        .account_id(account_id)
        .provider_id("calendar-request-2")
        .subject("Planning session updated")
        .sender_address("Organizer", "organizer@example.com")
        .recipient_address(Some("User"), "user@example.com")
        .has_attachments(true)
        .build();
    state.store.upsert_envelope(&newer).await.unwrap();
    state
        .store
        .insert_body(&mxr_core::types::MessageBody {
            message_id: newer.id.clone(),
            text_plain: Some("Updated planning session invite".into()),
            text_html: None,
            attachments: Vec::new(),
            fetched_at: chrono::Utc::now(),
            metadata: mxr_core::types::MessageMetadata {
                calendar: Some(mxr_core::types::CalendarMetadata {
                    method: Some("REQUEST".into()),
                    summary: Some("Planning session updated".into()),
                    uid: Some("planning-uid@example.com".into()),
                    sequence: Some(4),
                    organizer: Some(mxr_core::types::CalendarPerson {
                        email: "organizer@example.com".into(),
                        name: Some("Organizer".into()),
                        uri: Some("mailto:organizer@example.com".into()),
                    }),
                    attendees: vec![mxr_core::types::CalendarAttendee {
                        email: "user@example.com".into(),
                        name: Some("User".into()),
                        uri: Some("mailto:user@example.com".into()),
                        partstat: Some("NEEDS-ACTION".into()),
                        role: Some("REQ-PARTICIPANT".into()),
                        rsvp: Some(true),
                    }],
                    ..Default::default()
                }),
                ..Default::default()
            },
        })
        .await
        .unwrap();
    let state = Arc::new(state);

    let resp = handle_request(
        &state,
        &IpcMessage {
            id: 20,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::RespondInvite {
                message_id: stale_message_id,
                action: mxr_protocol::CalendarInviteActionData::Accept,
                dry_run: false,
            }),
        },
    )
    .await;

    match resp.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(message.contains("newer update"));
        }
        other => panic!("Expected stale invite error, got {other:?}"),
    }
    assert!(fake.sent_drafts().is_empty());
}

#[tokio::test]
async fn dispatch_respond_invite_warns_when_same_uid_has_different_organizer() {
    let (state, _fake) = AppState::in_memory_with_fake().await.unwrap();
    let message_id = insert_request_invite_body(&state).await;
    let account_id = state.default_account_id_opt().unwrap();
    let older = crate::test_fixtures::TestEnvelopeBuilder::new()
        .account_id(account_id)
        .provider_id("calendar-organizer-change")
        .subject("Planning session suspicious")
        .sender_address("Other Organizer", "other@example.com")
        .recipient_address(Some("User"), "user@example.com")
        .has_attachments(true)
        .build();
    state.store.upsert_envelope(&older).await.unwrap();
    state
        .store
        .insert_body(&mxr_core::types::MessageBody {
            message_id: older.id.clone(),
            text_plain: Some("Suspicious planning session invite".into()),
            text_html: None,
            attachments: Vec::new(),
            fetched_at: chrono::Utc::now(),
            metadata: mxr_core::types::MessageMetadata {
                calendar: Some(mxr_core::types::CalendarMetadata {
                    method: Some("REQUEST".into()),
                    summary: Some("Planning session suspicious".into()),
                    uid: Some("planning-uid@example.com".into()),
                    sequence: Some(1),
                    organizer: Some(mxr_core::types::CalendarPerson {
                        email: "other@example.com".into(),
                        name: Some("Other Organizer".into()),
                        uri: Some("mailto:other@example.com".into()),
                    }),
                    attendees: vec![mxr_core::types::CalendarAttendee {
                        email: "user@example.com".into(),
                        name: Some("User".into()),
                        uri: Some("mailto:user@example.com".into()),
                        partstat: Some("NEEDS-ACTION".into()),
                        role: Some("REQ-PARTICIPANT".into()),
                        rsvp: Some(true),
                    }],
                    ..Default::default()
                }),
                ..Default::default()
            },
        })
        .await
        .unwrap();
    let state = Arc::new(state);

    let resp = handle_request(
        &state,
        &IpcMessage {
            id: 22,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::RespondInvite {
                message_id,
                action: mxr_protocol::CalendarInviteActionData::Accept,
                dry_run: true,
            }),
        },
    )
    .await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::InviteResponsePreview { preview },
        }) => assert!(preview
            .warnings
            .iter()
            .any(|warning| warning.contains("different organizer"))),
        other => panic!("Expected organizer warning preview, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_respond_invite_blocks_fatal_parser_warning() {
    let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
    let account_id = state.default_account_id_opt().unwrap();
    let envelope = crate::test_fixtures::TestEnvelopeBuilder::new()
        .account_id(account_id)
        .provider_id("calendar-bad-parse")
        .subject("Broken invite")
        .sender_address("Organizer", "organizer@example.com")
        .recipient_address(Some("User"), "user@example.com")
        .has_attachments(true)
        .build();
    let message_id = envelope.id.clone();
    state.store.upsert_envelope(&envelope).await.unwrap();
    state
        .store
        .insert_body(&mxr_core::types::MessageBody {
            message_id: message_id.clone(),
            text_plain: Some("Broken invite".into()),
            text_html: None,
            attachments: Vec::new(),
            fetched_at: chrono::Utc::now(),
            metadata: mxr_core::types::MessageMetadata {
                calendar: Some(mxr_core::types::CalendarMetadata {
                    method: Some("REQUEST".into()),
                    summary: Some("Broken invite".into()),
                    uid: Some("broken-uid@example.com".into()),
                    organizer: Some(mxr_core::types::CalendarPerson {
                        email: "organizer@example.com".into(),
                        name: Some("Organizer".into()),
                        uri: Some("mailto:organizer@example.com".into()),
                    }),
                    attendees: vec![mxr_core::types::CalendarAttendee {
                        email: "user@example.com".into(),
                        name: Some("User".into()),
                        uri: Some("mailto:user@example.com".into()),
                        partstat: Some("NEEDS-ACTION".into()),
                        role: Some("REQ-PARTICIPANT".into()),
                        rsvp: Some(true),
                    }],
                    warnings: vec!["calendar invite could not be parsed as RFC 5545".into()],
                    ..Default::default()
                }),
                ..Default::default()
            },
        })
        .await
        .unwrap();
    let state = Arc::new(state);

    let resp = handle_request(
        &state,
        &IpcMessage {
            id: 21,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::RespondInvite {
                message_id,
                action: mxr_protocol::CalendarInviteActionData::Accept,
                dry_run: false,
            }),
        },
    )
    .await;

    match resp.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(message.contains("fatal parser warnings"));
        }
        other => panic!("Expected fatal parser warning error, got {other:?}"),
    }
    assert!(fake.sent_drafts().is_empty());
}

#[tokio::test]
async fn dispatch_get_body_preserves_exact_sources_and_inline_metadata() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;
    let attachment_id = mxr_core::AttachmentId::new();

    let stored = mxr_core::types::MessageBody {
        message_id: id.clone(),
        text_plain: Some("Hello team, \n> exact quote\n".into()),
        text_html: Some("<p>Hello <img src=\"cid:logo@example.com\"></p>".into()),
        attachments: vec![mxr_core::types::AttachmentMeta {
            id: attachment_id.clone(),
            message_id: id.clone(),
            filename: "logo.png".into(),
            mime_type: "image/png".into(),
            disposition: mxr_core::types::AttachmentDisposition::Inline,
            content_id: Some("logo@example.com".into()),
            content_location: Some("https://example.com/logo.png".into()),
            size_bytes: 2048,
            local_path: None,
            provider_id: "att-inline".into(),
        }],
        fetched_at: chrono::Utc::now(),
        metadata: mxr_core::types::MessageMetadata {
            text_plain_format: Some(mxr_core::types::TextPlainFormat::Flowed { delsp: true }),
            text_plain_source: Some(mxr_core::types::BodyPartSource::Exact),
            text_html_source: Some(mxr_core::types::BodyPartSource::Exact),
            ..Default::default()
        },
    };

    state.store.insert_body(&stored).await.unwrap();

    let msg = IpcMessage {
        id: 18,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetBody {
            message_id: id.clone(),
        }),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Body { body },
        }) => {
            assert_eq!(body.text_plain, stored.text_plain);
            assert_eq!(body.text_html, stored.text_html);
            assert_eq!(
                body.metadata.text_plain_format,
                stored.metadata.text_plain_format
            );
            assert_eq!(
                body.metadata.text_plain_source,
                stored.metadata.text_plain_source
            );
            assert_eq!(
                body.metadata.text_html_source,
                stored.metadata.text_html_source
            );
            assert_eq!(body.attachments.len(), 1);
            assert_eq!(body.attachments[0].id, attachment_id);
            assert_eq!(
                body.attachments[0].content_id.as_deref(),
                Some("logo@example.com")
            );
            assert_eq!(
                body.attachments[0].content_location.as_deref(),
                Some("https://example.com/logo.png")
            );
        }
        other => panic!("Expected Body, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_get_html_image_assets_resolves_inline_and_blocks_remote() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;
    let attachment_id = mxr_core::AttachmentId::new();
    let temp_dir = tempfile::tempdir().unwrap();
    let inline_path = temp_dir.path().join("logo.png");
    std::fs::write(&inline_path, tiny_png_bytes()).unwrap();

    let stored = mxr_core::types::MessageBody {
            message_id: id.clone(),
            text_plain: None,
            text_html: Some(concat!(
                "<img alt=\"Logo\" src=\"cid:logo@example.com\">",
                "<img alt=\"Badge\" src=\"data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO9xw1QAAAAASUVORK5CYII=\">",
                "<img alt=\"Hero\" src=\"https://example.com/hero.png\">"
            ).into()),
            attachments: vec![mxr_core::types::AttachmentMeta {
                id: attachment_id.clone(),
                message_id: id.clone(),
                filename: "logo.png".into(),
                mime_type: "image/png".into(),
                disposition: mxr_core::types::AttachmentDisposition::Inline,
                content_id: Some("logo@example.com".into()),
                content_location: None,
                size_bytes: 67,
                local_path: Some(inline_path.clone()),
                provider_id: "att-inline".into(),
            }],
            fetched_at: chrono::Utc::now(),
            metadata: mxr_core::types::MessageMetadata {
                text_html_source: Some(mxr_core::types::BodyPartSource::Exact),
                ..Default::default()
            },
        };
    state.store.insert_body(&stored).await.unwrap();

    let msg = IpcMessage {
        id: 16,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetHtmlImageAssets {
            message_id: id.clone(),
            allow_remote: false,
        }),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::HtmlImageAssets { assets, .. },
        }) => {
            assert_eq!(assets.len(), 3);

            let inline = assets
                .iter()
                .find(|asset| asset.source.starts_with("cid:"))
                .expect("cid asset");
            assert_eq!(inline.status, mxr_core::types::HtmlImageAssetStatus::Ready);
            assert_eq!(inline.path.as_deref(), Some(inline_path.as_path()));

            let embedded = assets
                .iter()
                .find(|asset| asset.source.starts_with("data:"))
                .expect("data asset");
            assert_eq!(
                embedded.status,
                mxr_core::types::HtmlImageAssetStatus::Ready,
                "embedded asset: {embedded:?}"
            );
            assert!(embedded.path.as_ref().is_some_and(|path| path.exists()));

            let remote = assets
                .iter()
                .find(|asset| asset.source.starts_with("https://"))
                .expect("remote asset");
            assert_eq!(
                remote.status,
                mxr_core::types::HtmlImageAssetStatus::Blocked
            );
            assert!(remote.path.is_none());
        }
        other => panic!("Expected HtmlImageAssets, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_get_html_image_assets_fetches_remote_when_enabled() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .insert_header("content-type", "image/png")
                .set_body_bytes(tiny_png_bytes()),
        )
        .mount(&server)
        .await;

    let stored = mxr_core::types::MessageBody {
        message_id: id.clone(),
        text_plain: None,
        text_html: Some(format!(
            r#"<img alt="Hero" src="{}/hero.png">"#,
            server.uri()
        )),
        attachments: vec![],
        fetched_at: chrono::Utc::now(),
        metadata: mxr_core::types::MessageMetadata {
            text_html_source: Some(mxr_core::types::BodyPartSource::Exact),
            ..Default::default()
        },
    };
    state.store.insert_body(&stored).await.unwrap();

    let msg = IpcMessage {
        id: 17,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetHtmlImageAssets {
            message_id: id.clone(),
            allow_remote: true,
        }),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::HtmlImageAssets { assets, .. },
        }) => {
            assert_eq!(assets.len(), 1);
            assert_eq!(
                assets[0].status,
                mxr_core::types::HtmlImageAssetStatus::Ready
            );
            let path = assets[0].path.as_ref().expect("cached path");
            assert!(path.exists());
            assert_eq!(std::fs::read(path).unwrap(), tiny_png_bytes());
        }
        other => panic!("Expected HtmlImageAssets, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_download_attachment_persists_local_path() {
    let state = AppState::in_memory().await.unwrap();
    state.set_attachment_dir_for_tests(
        std::env::temp_dir().join(format!("mxr-attachments-test-{}", uuid::Uuid::new_v4())),
    );
    let state = Arc::new(state);

    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    let list_msg = IpcMessage {
        id: 14,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListEnvelopes {
            label_id: None,
            account_id: None,
            limit: 200,
            offset: 0,
        }),
    };
    let resp = handle_request(&state, &list_msg).await;
    let envelope = match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Envelopes { envelopes },
        }) => envelopes
            .into_iter()
            .find(|envelope| envelope.has_attachments)
            .expect("fixture should include an attachment"),
        other => panic!("Expected Envelopes, got {other:?}"),
    };

    let body_msg = IpcMessage {
        id: 15,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetBody {
            message_id: envelope.id.clone(),
        }),
    };
    let resp = handle_request(&state, &body_msg).await;
    let attachment_id = match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Body { body },
        }) => body.attachments[0].id.clone(),
        other => panic!("Expected Body, got {other:?}"),
    };

    let download_msg = IpcMessage {
        id: 16,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::DownloadAttachment {
            message_id: envelope.id.clone(),
            attachment_id: attachment_id.clone(),
            destination: None,
        }),
    };
    let resp = handle_request(&state, &download_msg).await;
    let path = match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::AttachmentFile { file },
        }) => std::path::PathBuf::from(file.path),
        other => panic!("Expected AttachmentFile, got {other:?}"),
    };

    assert!(path.exists(), "downloaded attachment should exist on disk");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "downloaded attachments must be private");
    }

    let body = state
        .store
        .get_body(&envelope.id)
        .await
        .unwrap()
        .expect("body should remain cached");
    let attachment = body
        .attachments
        .iter()
        .find(|attachment| attachment.id == attachment_id)
        .expect("attachment should still exist");
    assert_eq!(attachment.local_path.as_ref(), Some(&path));

    let _ = std::fs::remove_dir_all(state.attachment_dir());
}

#[tokio::test]
async fn dispatch_set_reply_later_persists_flag_visible_in_queue() {
    // Behavior: marking a message reply-later via IPC persists the flag,
    // and subsequent `ListReplyQueue` requests return the envelope.
    // Clearing the flag removes it from the queue.
    let (state, _) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let id = sync_and_get_first_id(&state).await;

    // Initially the queue is empty.
    let resp = handle_request(
        &state,
        &IpcMessage {
            id: 200,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::ListReplyQueue),
        },
    )
    .await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::ReplyQueue { messages },
        }) => assert!(messages.is_empty(), "fresh queue is empty"),
        other => panic!("expected ReplyQueue, got {other:?}"),
    }

    // Set the flag.
    let resp = handle_request(
        &state,
        &IpcMessage {
            id: 201,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::SetReplyLater {
                message_id: id.clone(),
                flag: true,
            }),
        },
    )
    .await;
    assert!(matches!(
        resp.payload,
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack
        })
    ));

    // Queue now contains the flagged envelope.
    let resp = handle_request(
        &state,
        &IpcMessage {
            id: 202,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::ListReplyQueue),
        },
    )
    .await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::ReplyQueue { messages },
        }) => {
            assert_eq!(messages.len(), 1, "one flagged message");
            assert_eq!(messages[0].id, id);
        }
        other => panic!("expected ReplyQueue, got {other:?}"),
    }
    let ast = mxr_search::parse_query("is:reply-later").unwrap();
    let schema = mxr_search::MxrSchema::build();
    let query = mxr_search::QueryBuilder::new(&schema).build(&ast);
    let search_page = state
        .search
        .search_ast(query, 10, 0, mxr_core::types::SortOrder::DateDesc)
        .await
        .unwrap();
    assert_eq!(search_page.results.len(), 1, "search sees reply-later");
    assert_eq!(search_page.results[0].message_id, id.as_str());

    // Clear the flag.
    let resp = handle_request(
        &state,
        &IpcMessage {
            id: 203,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::SetReplyLater {
                message_id: id.clone(),
                flag: false,
            }),
        },
    )
    .await;
    assert!(matches!(
        resp.payload,
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack
        })
    ));

    // Queue is empty again.
    let resp = handle_request(
        &state,
        &IpcMessage {
            id: 204,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::ListReplyQueue),
        },
    )
    .await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::ReplyQueue { messages },
        }) => assert!(messages.is_empty(), "queue empty after clear"),
        other => panic!("expected ReplyQueue, got {other:?}"),
    }
    let ast = mxr_search::parse_query("is:reply-later").unwrap();
    let schema = mxr_search::MxrSchema::build();
    let query = mxr_search::QueryBuilder::new(&schema).build(&ast);
    let search_page = state
        .search
        .search_ast(query, 10, 0, mxr_core::types::SortOrder::DateDesc)
        .await
        .unwrap();
    assert!(search_page.results.is_empty(), "search updates after clear");
}

/// Phase 2.1: dismissing a reply-later flag is a pure metadata
/// operation. It removes the message from the queue, but it must
/// not generate a draft, hand a message to the outbound pipeline,
/// or otherwise pretend the user replied. The user is saying
/// "never mind, I'm not going to reply" — the daemon must take that
/// at face value.
#[tokio::test]
async fn dispatch_clearing_reply_later_does_not_send_reply() {
    let (state, _) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let id = sync_and_get_first_id(&state).await;
    let account_id = state.default_account_id();

    // Flag it first so we have something to dismiss.
    let resp = handle_request(
        &state,
        &IpcMessage {
            id: 250,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::SetReplyLater {
                message_id: id.clone(),
                flag: true,
            }),
        },
    )
    .await;
    assert!(matches!(
        resp.payload,
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack
        })
    ));

    // Capture a baseline of state that would change if a reply were
    // queued or sent.
    let drafts_before = state.store.list_drafts(&account_id).await.unwrap().len();
    let mut events = state.event_tx.subscribe();

    // Dismiss the flag — this is the "I'm not going to reply"
    // outcome the user signals by clearing the queue entry.
    let resp = handle_request(
        &state,
        &IpcMessage {
            id: 251,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::SetReplyLater {
                message_id: id.clone(),
                flag: false,
            }),
        },
    )
    .await;
    assert!(matches!(
        resp.payload,
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack
        })
    ));

    // No draft created, none consumed — the drafts table is exactly
    // where it was before the dismiss.
    let drafts_after = state.store.list_drafts(&account_id).await.unwrap().len();
    assert_eq!(
        drafts_after, drafts_before,
        "dismissing reply-later must not touch the drafts table"
    );

    // No daemon event was emitted by the dismiss. The flag clear
    // is a pure metadata edit; anything published here means the
    // path is doing more than the user asked for.
    match events.try_recv() {
        Err(tokio::sync::broadcast::error::TryRecvError::Empty) => {} // expected
        Ok(received) => panic!(
            "dismissing reply-later must not emit any daemon event; got {:?}",
            received.payload
        ),
        Err(err) => panic!("unexpected event channel state: {err:?}"),
    }
}

#[tokio::test]
async fn dispatch_set_auto_reminder_persists_and_loop_fires_when_due() {
    // End-to-end: setting a reminder via IPC persists it; the
    // background-loop function fires it once `now >= remind_at` and
    // emits a `ReminderTriggered` event.
    let (state, _) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let id = sync_and_get_first_id(&state).await;
    let mut events = state.event_tx.subscribe();

    // Set the reminder for "1 hour ago" so it's already due.
    let remind_at = chrono::Utc::now() - chrono::Duration::hours(1);
    let resp = handle_request(
        &state,
        &IpcMessage {
            id: 300,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::SetAutoReminder {
                sent_message_id: id.clone(),
                remind_at,
            }),
        },
    )
    .await;
    assert!(matches!(
        resp.payload,
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack
        })
    ));

    // Run one tick of the loop with `now` past the reminder.
    let fired = crate::loops::process_due_reminders(&state, chrono::Utc::now())
        .await
        .unwrap();
    assert_eq!(fired, 1, "one due reminder fires");

    // Expect a ReminderTriggered event for the right message.
    let received = events.try_recv().expect("event published");
    match received.payload {
        IpcPayload::Event(DaemonEvent::ReminderTriggered { sent_message_id }) => {
            assert_eq!(sent_message_id, id);
        }
        other => panic!("expected ReminderTriggered event, got {other:?}"),
    }

    let queue = handle_request(
        &state,
        &IpcMessage {
            id: 302,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::ListReplyQueue),
        },
    )
    .await;
    match queue.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::ReplyQueue { messages },
        }) => {
            assert!(
                messages.iter().any(|message| message.id == id),
                "due reminders must be visible in the reply-later queue"
            );
        }
        other => panic!("expected ReplyQueue response, got {other:?}"),
    }

    // Second tick: nothing fires (already-triggered reminders are
    // excluded).
    let fired_again = crate::loops::process_due_reminders(&state, chrono::Utc::now())
        .await
        .unwrap();
    assert_eq!(fired_again, 0, "fired reminders are not re-fired");
}

#[tokio::test]
async fn dispatch_cancel_auto_reminder_prevents_firing() {
    // Setting then cancelling a reminder leaves no due rows for
    // the loop to fire.
    let (state, _) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let id = sync_and_get_first_id(&state).await;

    let remind_at = chrono::Utc::now() - chrono::Duration::hours(1);
    let _ = handle_request(
        &state,
        &IpcMessage {
            id: 310,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::SetAutoReminder {
                sent_message_id: id.clone(),
                remind_at,
            }),
        },
    )
    .await;
    let resp = handle_request(
        &state,
        &IpcMessage {
            id: 311,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::CancelAutoReminder {
                sent_message_id: id.clone(),
            }),
        },
    )
    .await;
    assert!(matches!(
        resp.payload,
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack
        })
    ));

    let fired = crate::loops::process_due_reminders(&state, chrono::Utc::now())
        .await
        .unwrap();
    assert_eq!(fired, 0, "cancelled reminders never fire");
}

#[tokio::test]
async fn dispatch_schedule_send_persists_and_loop_flushes_when_due() {
    // End-to-end: schedule an existing draft for a past send_at,
    // run one tick of the loop, expect the send pipeline to fire
    // and the draft's status to advance past 'draft'.
    let (state, _) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let _ = sync_and_get_first_id(&state).await;

    // Insert a draft for the synthetic account.
    let account = state
        .store
        .list_accounts()
        .await
        .unwrap()
        .first()
        .unwrap()
        .clone();
    let draft = mxr_core::types::Draft {
        id: mxr_core::id::DraftId::new(),
        account_id: account.id.clone(),
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![mxr_core::types::Address {
            name: None,
            email: "you@example.com".into(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "scheduled".into(),
        body_markdown: "Body".into(),
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    state.store.insert_draft(&draft).await.unwrap();

    // Schedule for "1 hour ago" — already due.
    let resp = handle_request(
        &state,
        &IpcMessage {
            id: 400,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::ScheduleSend {
                draft_id: draft.id.clone(),
                send_at: chrono::Utc::now() - chrono::Duration::hours(1),
            }),
        },
    )
    .await;
    assert!(matches!(
        resp.payload,
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack
        })
    ));
    assert!(
        state
            .store
            .get_scheduled_send(&draft.id)
            .await
            .unwrap()
            .is_some(),
        "send_at persisted"
    );

    // Run a tick of the flusher.
    let fired = crate::loops::process_due_scheduled_sends(&state, chrono::Utc::now())
        .await
        .unwrap();
    assert_eq!(fired, 1);

    // Draft no longer needs sending: either advanced past `draft`
    // status (FakeProvider may delete on success) or is gone entirely.
    let status = state.store.get_draft_status(&draft.id).await.unwrap();
    assert!(
        !matches!(status, Some(mxr_core::types::DraftStatus::Draft)),
        "draft no longer in 'draft' status: {status:?}"
    );

    // The schedule entry is cleared (the row may be gone too) so a
    // second tick won't try to re-flush it.
    assert!(state
        .store
        .get_scheduled_send(&draft.id)
        .await
        .unwrap()
        .is_none());
    let fired_again = crate::loops::process_due_scheduled_sends(&state, chrono::Utc::now())
        .await
        .unwrap();
    assert_eq!(fired_again, 0);
}

#[tokio::test]
async fn dispatch_cancel_scheduled_send_prevents_flush() {
    let (state, _) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let _ = sync_and_get_first_id(&state).await;

    let account = state
        .store
        .list_accounts()
        .await
        .unwrap()
        .first()
        .unwrap()
        .clone();
    let draft = mxr_core::types::Draft {
        id: mxr_core::id::DraftId::new(),
        account_id: account.id.clone(),
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![mxr_core::types::Address {
            name: None,
            email: "you@example.com".into(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "scheduled-then-cancelled".into(),
        body_markdown: "Body".into(),
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    state.store.insert_draft(&draft).await.unwrap();

    let _ = handle_request(
        &state,
        &IpcMessage {
            id: 410,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::ScheduleSend {
                draft_id: draft.id.clone(),
                send_at: chrono::Utc::now() - chrono::Duration::hours(1),
            }),
        },
    )
    .await;
    let resp = handle_request(
        &state,
        &IpcMessage {
            id: 411,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::CancelScheduledSend {
                draft_id: draft.id.clone(),
            }),
        },
    )
    .await;
    assert!(matches!(
        resp.payload,
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack
        })
    ));

    let fired = crate::loops::process_due_scheduled_sends(&state, chrono::Utc::now())
        .await
        .unwrap();
    assert_eq!(fired, 0);

    // Draft remains in 'draft' status — never sent.
    assert_eq!(
        state.store.get_draft_status(&draft.id).await.unwrap(),
        Some(mxr_core::types::DraftStatus::Draft)
    );
}

#[tokio::test]
async fn dispatch_mutation_star() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;

    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::mutation(MutationCommand::Star {
            message_ids: vec![id.clone()],
            starred: true,
        })),
    };
    let resp = handle_request(&state, &msg).await;
    assert_mutation_succeeded(resp.payload);

    // Verify flag is set
    let get_msg = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetEnvelope { message_id: id }),
    };
    let resp = handle_request(&state, &get_msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Envelope { envelope },
        }) => {
            assert!(
                envelope
                    .flags
                    .contains(mxr_core::types::MessageFlags::STARRED),
                "Expected STARRED flag to be set, got {:?}",
                envelope.flags
            );
        }
        other => panic!("Expected Envelope, got {other:?}"),
    }
}

#[tokio::test]
async fn modify_labels_on_folder_provider_does_not_leave_one_message_in_two_folders() {
    let state = folder_copy_state().await;
    let id = sync_and_get_first_id(&state).await;

    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::mutation(MutationCommand::ModifyLabels {
            message_ids: vec![id],
            add: vec!["Archive".to_string()],
            remove: vec![],
        })),
    };
    let resp = handle_request(&state, &msg).await;
    assert_mutation_succeeded(resp.payload);

    let envelopes = state
        .store
        .list_envelopes_by_account(&state.default_account_id(), 20, 0)
        .await
        .unwrap();
    assert_eq!(
        envelopes.len(),
        2,
        "expected exactly one inbox copy and one archive copy after folder add: {envelopes:?}"
    );
    assert!(
            !envelopes.iter().any(|envelope| {
                envelope
                    .label_provider_ids
                    .iter()
                    .any(|provider_id| provider_id == "INBOX")
                    && envelope
                        .label_provider_ids
                        .iter()
                        .any(|provider_id| provider_id == "Archive")
            }),
            "folder-based providers should not be flattened into one message with two folders: {envelopes:?}"
        );
    assert!(
        envelopes
            .iter()
            .any(|envelope| envelope.label_provider_ids == vec!["INBOX".to_string()]),
        "expected inbox copy after folder add"
    );
    assert!(
        envelopes
            .iter()
            .any(|envelope| envelope.label_provider_ids == vec!["Archive".to_string()]),
        "expected archive copy after folder add"
    );
}
