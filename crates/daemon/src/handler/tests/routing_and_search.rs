use super::*;

#[test]
fn request_lane_routes_llm_and_network_requests_to_bulk() {
    // Sample of the slow lane: at least one LLM call, one network
    // round-trip, and one heavy rebuild. Locks the classifier so a
    // refactor that drops a slow request back to Hot will trip the
    // test instead of silently re-introducing head-of-line blocking
    // on fast user-initiated commands.
    let llm = Request::SummarizeThread {
        thread_id: mxr_core::ThreadId::new(),
    };
    assert_eq!(request_lane(&llm), IpcLane::Bulk);

    let download = Request::DownloadAttachment {
        message_id: mxr_core::MessageId::new(),
        attachment_id: mxr_core::AttachmentId::from_provider_id("p", "a"),
        destination: None,
    };
    assert_eq!(request_lane(&download), IpcLane::Bulk);

    let rebuild = Request::RefreshContacts;
    assert_eq!(request_lane(&rebuild), IpcLane::Bulk);

    let remote_assets = Request::GetHtmlImageAssets {
        message_id: mxr_core::MessageId::new(),
        allow_remote: true,
    };
    assert_eq!(request_lane(&remote_assets), IpcLane::Bulk);
}

#[test]
fn request_lane_defaults_user_initiated_commands_to_hot() {
    let list = Request::ListEnvelopes {
        label_id: None,
        account_id: None,
        limit: 50,
        offset: 0,
    };
    assert_eq!(request_lane(&list), IpcLane::Hot);

    let archive = Request::Mutation {
        mutation: mxr_protocol::MutationCommand::Archive {
            message_ids: vec![mxr_core::MessageId::new()],
        },
        client_correlation_id: None,
    };
    assert_eq!(request_lane(&archive), IpcLane::Hot);

    // HTML images without remote fetch is a local-only render and
    // should stay on the hot lane.
    let local_assets = Request::GetHtmlImageAssets {
        message_id: mxr_core::MessageId::new(),
        allow_remote: false,
    };
    assert_eq!(request_lane(&local_assets), IpcLane::Hot);

    let ping = Request::Ping;
    assert_eq!(request_lane(&ping), IpcLane::Hot);
}

#[test]
fn sanitized_attachment_filename_truncates_long_names_preserving_extension() {
    let attachment_id = mxr_core::AttachmentId::from_provider_id("test", "long-pdf");
    let filename = format!("{}.pdf", "a".repeat(400));

    let sanitized = sanitized_attachment_filename(&filename, &attachment_id);

    assert!(
        sanitized.len() <= 220,
        "filename should fit conservative path component limit: {} bytes",
        sanitized.len()
    );
    assert!(sanitized.ends_with(&format!("-{}.pdf", attachment_id.as_str())));
}

#[test]
fn sanitized_attachment_filename_truncates_utf8_on_char_boundary() {
    let attachment_id = mxr_core::AttachmentId::from_provider_id("test", "utf8-pdf");
    let filename = format!("{}.pdf", "é".repeat(200));

    let sanitized = sanitized_attachment_filename(&filename, &attachment_id);

    assert!(
        sanitized.len() <= 220,
        "filename should fit conservative path component limit: {} bytes",
        sanitized.len()
    );
    assert!(sanitized.ends_with(&format!("-{}.pdf", attachment_id.as_str())));
}

#[test]
fn sanitized_attachment_filename_uses_stable_fallback_for_blank_names() {
    let attachment_id = mxr_core::AttachmentId::from_provider_id("test", "blank");

    let sanitized = sanitized_attachment_filename("   ", &attachment_id);

    assert_eq!(sanitized, format!("attachment-{}", attachment_id.as_str()));
}

#[test]
fn sanitized_attachment_filename_uses_stable_fallback_for_windows_reserved_names() {
    let attachment_id = mxr_core::AttachmentId::from_provider_id("test", "reserved");

    let sanitized = sanitized_attachment_filename("CON.txt", &attachment_id);

    assert_eq!(sanitized, format!("attachment-{}", attachment_id.as_str()));
}

#[tokio::test]
async fn dispatch_ping_returns_pong() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Ping),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Pong,
        }) => {}
        other => panic!("Expected Pong, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_list_envelopes_after_sync() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    // Initial sync
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    let msg = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListEnvelopes {
            label_id: None,
            account_id: None,
            limit: 100,
            offset: 0,
        }),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Envelopes { envelopes },
        }) => {
            assert_eq!(envelopes.len(), 55);
        }
        other => panic!("Expected Envelopes, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_list_envelopes_by_label() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    // Get labels first
    let labels_msg = IpcMessage {
        id: 10,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListLabels { account_id: None }),
    };
    let resp = handle_request(&state, &labels_msg).await;
    let labels = match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Labels { labels },
        }) => labels,
        other => panic!("Expected Labels, got {other:?}"),
    };

    // Find Inbox label
    let inbox = labels
        .iter()
        .find(|l| l.name == "Inbox")
        .expect("Inbox label missing");

    // Fetch envelopes by Inbox label
    let msg = IpcMessage {
        id: 11,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListEnvelopes {
            label_id: Some(inbox.id.clone()),
            account_id: None,
            limit: 100,
            offset: 0,
        }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Envelopes { envelopes },
        }) => {
            assert!(
                !envelopes.is_empty(),
                "Inbox label should have envelopes, got 0. Inbox label_id={}",
                inbox.id
            );
        }
        IpcPayload::Response(Response::Error { message, .. }) => {
            panic!("Got error response: {message}");
        }
        other => panic!("Expected Envelopes, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_list_labels_without_accounts_returns_empty() {
    let state = Arc::new(AppState::in_memory_without_accounts().await.unwrap());

    let msg = IpcMessage {
        id: 12,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListLabels { account_id: None }),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Labels { labels },
        }) => assert!(labels.is_empty()),
        other => panic!("Expected Labels, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_list_envelopes_without_accounts_returns_empty() {
    let state = Arc::new(AppState::in_memory_without_accounts().await.unwrap());

    let msg = IpcMessage {
        id: 13,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListEnvelopes {
            label_id: None,
            account_id: None,
            limit: 100,
            offset: 0,
        }),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Envelopes { envelopes },
        }) => assert!(envelopes.is_empty()),
        other => panic!("Expected Envelopes, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_read_only_mailbox_uses_local_account_when_provider_missing() {
    let state = Arc::new(AppState::in_memory_without_accounts().await.unwrap());
    let account_id = mxr_core::AccountId::from_provider_id("gmail", "me@example.com");
    let account = mxr_core::types::Account {
        id: account_id.clone(),
        name: "Personal".to_string(),
        email: "me@example.com".to_string(),
        sync_backend: Some(mxr_core::types::BackendRef {
            provider_kind: mxr_core::types::ProviderKind::Gmail,
            config_key: "personal".to_string(),
        }),
        send_backend: None,
        enabled: true,
    };
    state.store.insert_account(&account).await.unwrap();

    let label = mxr_core::types::Label {
        id: mxr_core::LabelId::from_provider_id("gmail", "INBOX"),
        account_id: account_id.clone(),
        name: "Inbox".to_string(),
        kind: mxr_core::types::LabelKind::System,
        color: None,
        provider_id: "INBOX".to_string(),
        unread_count: 0,
        total_count: 1,
        role: None,
    };
    state.store.upsert_label(&label).await.unwrap();

    let message_id = mxr_core::MessageId::from_provider_id("gmail", "msg-1");
    let envelope = mxr_core::types::Envelope {
        id: message_id.clone(),
        account_id: account_id.clone(),
        provider_id: "msg-1".to_string(),
        thread_id: mxr_core::ThreadId::from_provider_id("gmail", "thread-1"),
        message_id_header: Some("<msg-1@example.com>".to_string()),
        in_reply_to: None,
        references: vec![],
        from: mxr_core::types::Address {
            name: Some("Sender".to_string()),
            email: "sender@example.com".to_string(),
        },
        to: vec![mxr_core::types::Address {
            name: None,
            email: "me@example.com".to_string(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Still local".to_string(),
        date: chrono::Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
        flags: mxr_core::types::MessageFlags::empty(),
        snippet: "cached body".to_string(),
        has_attachments: false,
        size_bytes: 128,
        unsubscribe: mxr_core::types::UnsubscribeMethod::None,
        link_count: 0,
        body_word_count: 2,
        label_provider_ids: vec![],
        keywords: std::collections::BTreeSet::new(),
    };
    state.store.upsert_envelope(&envelope).await.unwrap();

    let envelopes_msg = IpcMessage {
        id: 14,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListEnvelopes {
            label_id: None,
            account_id: None,
            limit: 100,
            offset: 0,
        }),
    };
    let resp = handle_request(&state, &envelopes_msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Envelopes { envelopes },
        }) => assert_eq!(
            envelopes.iter().map(|env| &env.id).collect::<Vec<_>>(),
            vec![&message_id]
        ),
        other => panic!("Expected Envelopes, got {other:?}"),
    }

    let labels_msg = IpcMessage {
        id: 15,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListLabels { account_id: None }),
    };
    let resp = handle_request(&state, &labels_msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Labels { labels },
        }) => assert_eq!(
            labels.iter().map(|label| &label.id).collect::<Vec<_>>(),
            vec![&label.id]
        ),
        other => panic!("Expected Labels, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_create_label_persists_and_returns_label() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let account_id = state.default_account_id();

    let create_msg = IpcMessage {
        id: 14,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::CreateLabel {
            name: "Urgent".to_string(),
            color: Some("#ff6600".to_string()),
            account_id: Some(account_id.clone()),
        }),
    };
    let resp = handle_request(&state, &create_msg).await;
    let created = match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Label { label },
        }) => label,
        other => panic!("Expected Label, got {other:?}"),
    };
    assert_eq!(created.name, "Urgent");
    assert_eq!(created.color.as_deref(), Some("#ff6600"));
    assert_eq!(created.account_id, account_id);

    let list_msg = IpcMessage {
        id: 15,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListLabels {
            account_id: Some(account_id),
        }),
    };
    let resp = handle_request(&state, &list_msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Labels { labels },
        }) => {
            assert!(labels.iter().any(|label| label.name == "Urgent"));
        }
        other => panic!("Expected Labels, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_upsert_and_list_rules() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let now = chrono::Utc::now();
    let rule = serde_json::json!({
        "id": "rule-1",
        "name": "Archive newsletters",
        "enabled": true,
        "priority": 10,
        "conditions": {"type":"field","field":"has_label","label":"newsletters"},
        "actions": [{"type":"archive"}],
        "created_at": now,
        "updated_at": now
    });

    let upsert_msg = IpcMessage {
        id: 20,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::UpsertRule { rule: rule.clone() }),
    };
    let resp = handle_request(&state, &upsert_msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::RuleData { rule: returned },
        }) => {
            assert_eq!(returned["name"], "Archive newsletters");
        }
        other => panic!("Expected RuleData, got {other:?}"),
    }

    let list_msg = IpcMessage {
        id: 21,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListRules),
    };
    let resp = handle_request(&state, &list_msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Rules { rules },
        }) => {
            assert_eq!(rules.len(), 1);
            assert_eq!(rules[0]["id"], "rule-1");
        }
        other => panic!("Expected Rules, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_dry_run_rules_returns_matching_messages() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();
    let now = chrono::Utc::now();
    let rule = serde_json::json!({
        "id": "rule-1",
        "name": "Mark unread",
        "enabled": true,
        "priority": 10,
        "conditions": {"type":"field","field":"is_unread"},
        "actions": [{"type":"mark_read"}],
        "created_at": now,
        "updated_at": now
    });
    let _ = handle_request(
        &state,
        &IpcMessage {
            id: 22,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::UpsertRule { rule }),
        },
    )
    .await;

    let dry_run_msg = IpcMessage {
        id: 23,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::DryRunRules {
            rule: Some("rule-1".to_string()),
            all: false,
            after: None,
        }),
    };
    let resp = handle_request(&state, &dry_run_msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::RuleDryRun { results },
        }) => {
            assert_eq!(results.len(), 1);
            let matches = results[0]["matches"]
                .as_array()
                .expect("matches should be an array");
            assert!(!matches.is_empty());
        }
        other => panic!("Expected RuleDryRun, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_upsert_rule_form_and_get_rule_form() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    let upsert_msg = IpcMessage {
        id: 231,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::UpsertRuleForm {
            existing_rule: None,
            name: "Archive unread".into(),
            condition: "is:unread".into(),
            action: "archive".into(),
            priority: 25,
            enabled: true,
        }),
    };
    let resp = handle_request(&state, &upsert_msg).await;
    let rule_id = match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::RuleData { rule },
        }) => {
            assert_eq!(rule["name"], "Archive unread");
            rule["id"].as_str().unwrap().to_string()
        }
        other => panic!("Expected RuleData, got {other:?}"),
    };

    let get_form_msg = IpcMessage {
        id: 232,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetRuleForm { rule: rule_id }),
    };
    let resp = handle_request(&state, &get_form_msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::RuleFormData { form },
        }) => {
            assert_eq!(form.name, "Archive unread");
            assert_eq!(form.condition, "is:unread");
            assert_eq!(form.action, "archive");
            assert_eq!(form.priority, 25);
            assert!(form.enabled);
        }
        other => panic!("Expected RuleFormData, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_rename_label_updates_visible_label() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let account_id = state.default_account_id();

    let create_msg = IpcMessage {
        id: 14,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::CreateLabel {
            name: "Projects".to_string(),
            color: None,
            account_id: Some(account_id.clone()),
        }),
    };
    let _ = handle_request(&state, &create_msg).await;

    let rename_msg = IpcMessage {
        id: 15,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::RenameLabel {
            old: "Projects".to_string(),
            new: "Client Work".to_string(),
            account_id: Some(account_id.clone()),
        }),
    };
    let resp = handle_request(&state, &rename_msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Label { label },
        }) => {
            assert_eq!(label.name, "Client Work");
            assert_eq!(label.provider_id, "Client Work");
        }
        other => panic!("Expected Label, got {other:?}"),
    }

    let list_msg = IpcMessage {
        id: 16,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListLabels {
            account_id: Some(account_id),
        }),
    };
    let resp = handle_request(&state, &list_msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Labels { labels },
        }) => {
            assert!(labels.iter().any(|label| label.name == "Client Work"));
            assert!(!labels.iter().any(|label| label.name == "Projects"));
        }
        other => panic!("Expected Labels, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_delete_label_removes_it_from_store() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let account_id = state.default_account_id();

    let create_msg = IpcMessage {
        id: 17,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::CreateLabel {
            name: "Temporary".to_string(),
            color: None,
            account_id: Some(account_id.clone()),
        }),
    };
    let _ = handle_request(&state, &create_msg).await;

    let delete_msg = IpcMessage {
        id: 18,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::DeleteLabel {
            name: "Temporary".to_string(),
            account_id: Some(account_id.clone()),
        }),
    };
    let resp = handle_request(&state, &delete_msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack,
        }) => {}
        other => panic!("Expected Ack, got {other:?}"),
    }

    let list_msg = IpcMessage {
        id: 19,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListLabels {
            account_id: Some(account_id),
        }),
    };
    let resp = handle_request(&state, &list_msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Labels { labels },
        }) => {
            assert!(!labels.iter().any(|label| label.name == "Temporary"));
        }
        other => panic!("Expected Labels, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_count_after_sync() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    let msg = IpcMessage {
        id: 3,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Count {
            query: "deployment".to_string(),
            mode: None,
        }),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Count { count },
        }) => {
            assert!(count > 0, "Expected non-zero count for 'deployment'");
        }
        other => panic!("Expected Count, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_list_saved_searches_empty() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    let msg = IpcMessage {
        id: 4,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListSavedSearches),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::SavedSearches { searches },
        }) => {
            assert!(searches.is_empty());
        }
        other => panic!("Expected empty SavedSearches, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_create_and_list_saved_searches() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    // Create
    let create_msg = IpcMessage {
        id: 5,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::CreateSavedSearch {
            name: "Important".to_string(),
            query: "is:starred".to_string(),
            search_mode: mxr_core::SearchMode::Lexical,
        }),
    };
    let resp = handle_request(&state, &create_msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::SavedSearchData { search },
        }) => {
            assert_eq!(search.name, "Important");
            assert_eq!(search.query, "is:starred");
            assert_eq!(search.search_mode, mxr_core::SearchMode::Lexical);
        }
        other => panic!("Expected SavedSearchData, got {other:?}"),
    }

    // List
    let list_msg = IpcMessage {
        id: 6,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListSavedSearches),
    };
    let resp = handle_request(&state, &list_msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::SavedSearches { searches },
        }) => {
            assert_eq!(searches.len(), 1);
            assert_eq!(searches[0].name, "Important");
            assert_eq!(searches[0].search_mode, mxr_core::SearchMode::Lexical);
        }
        other => panic!("Expected SavedSearches, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_create_saved_search_persists_requested_mode() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let create_msg = IpcMessage {
        id: 51,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::CreateSavedSearch {
            name: "Hybrid".to_string(),
            query: "deployment".to_string(),
            search_mode: mxr_core::SearchMode::Hybrid,
        }),
    };

    let resp = handle_request(&state, &create_msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::SavedSearchData { search },
        }) => {
            assert_eq!(search.search_mode, mxr_core::SearchMode::Hybrid);
        }
        other => panic!("Expected SavedSearchData, got {other:?}"),
    }

    let saved = state
        .store
        .get_saved_search_by_name("Hybrid")
        .await
        .unwrap()
        .expect("saved search");
    assert_eq!(saved.search_mode, mxr_core::SearchMode::Hybrid);
}

#[tokio::test]
async fn dispatch_run_saved_search_returns_results() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    let create = IpcMessage {
        id: 200,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::CreateSavedSearch {
            name: "Deploy".into(),
            query: "deployment".into(),
            search_mode: mxr_core::SearchMode::Lexical,
        }),
    };
    handle_request(&state, &create).await;

    let msg = IpcMessage {
        id: 201,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::RunSavedSearch {
            name: "Deploy".into(),
            limit: 10,
        }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data:
                ResponseData::SearchResults {
                    results,
                    has_more,
                    explain,
                    ..
                },
        }) => {
            assert!(!has_more);
            assert!(explain.is_none());
            assert!(!results.is_empty());
            assert!(results.len() <= 10);
            assert!(
                results
                    .iter()
                    .all(|item| item.mode == mxr_core::SearchMode::Lexical),
                "saved search should return lexical results"
            );
        }
        other => panic!("Expected SearchResults, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_status() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    let msg = IpcMessage {
        id: 7,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetStatus),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data:
                ResponseData::Status {
                    uptime_secs: _,
                    accounts,
                    total_messages: _,
                    daemon_pid,
                    sync_statuses,
                    protocol_version,
                    daemon_version,
                    daemon_build_id,
                    repair_required,
                    ..
                },
        }) => {
            assert_eq!(accounts.len(), 1);
            let daemon_pid = daemon_pid.expect("daemon pid should be present");
            assert!(daemon_pid > 0);
            assert_eq!(sync_statuses.len(), 1);
            assert!(protocol_version >= mxr_protocol::IPC_PROTOCOL_VERSION);
            let daemon_version = daemon_version.expect("daemon version should be present");
            assert_ne!(daemon_version, "");
            let daemon_build_id = daemon_build_id.expect("daemon build id should be present");
            assert_ne!(daemon_build_id, "");
            assert!(!repair_required);
        }
        other => panic!("Expected Status, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_status_reports_degraded_relationship_llm_features_when_llm_disabled() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    let msg = IpcMessage {
        id: 7,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetStatus),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data:
                ResponseData::Status {
                    feature_health: Some(feature_health),
                    ..
                },
        }) => {
            assert!(matches!(
                feature_health.relationship_profile,
                FeatureHealth::Degraded { .. }
            ));
            assert!(matches!(
                feature_health.commitments,
                FeatureHealth::Degraded { .. }
            ));
            assert!(matches!(
                feature_health.humanizer,
                FeatureHealth::Degraded { .. }
            ));
        }
        other => panic!("Expected Status with feature health, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_status_does_not_block_when_search_is_busy() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    let msg = IpcMessage {
        id: 8,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetStatus),
    };

    let resp = tokio::time::timeout(
        std::time::Duration::from_millis(250),
        handle_request(&state, &msg),
    )
    .await
    .expect("status should not block on a busy search index");

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Status { .. },
        }) => {}
        other => panic!("Expected Status, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_shutdown_acknowledges_without_exiting() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    let msg = IpcMessage {
        id: 9,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Shutdown),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack,
        }) => {}
        other => panic!("Expected Ack, got {other:?}"),
    }
    assert!(state.shutdown_requested());
}

#[tokio::test]
async fn dispatch_doctor_report() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    let msg = IpcMessage {
        id: 81,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetDoctorReport),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::DoctorReport { report },
        }) => {
            assert!(report.database_path.contains("mxr.db"));
            assert!(report.index_path.contains("search_index"));
            let daemon_version = report.daemon_version.expect("doctor report daemon version");
            assert_ne!(daemon_version, "");
            let daemon_build_id = report.daemon_build_id.expect("doctor report build id");
            assert_ne!(daemon_build_id, "");
        }
        other => panic!("Expected DoctorReport, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_sync_status() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let account_id = state.default_account_id();

    let msg = IpcMessage {
        id: 82,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetSyncStatus { account_id }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::SyncStatus { sync },
        }) => {
            assert_ne!(sync.account_name, "");
            let summary = sync
                .current_cursor_summary
                .expect("sync status should include cursor summary");
            assert_ne!(summary, "");
        }
        other => panic!("Expected SyncStatus, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_search_returns_results() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    // Sync first so search index is populated
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    let msg = IpcMessage {
        id: 10,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Search {
            query: "deployment".to_string(),
            limit: 10,
            offset: 0,
            mode: None,
            sort: None,
            explain: false,
        }),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::SearchResults { results, .. },
        }) => {
            assert!(
                !results.is_empty(),
                "Search for 'deployment' should return results"
            );
            assert!(results.len() <= 10);
            assert_eq!(results[0].mode, mxr_core::SearchMode::Lexical);
        }
        other => panic!("Expected SearchResults, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_search_explain_returns_execution_details() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    let msg = IpcMessage {
        id: 11,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Search {
            query: "deployment".to_string(),
            limit: 5,
            offset: 0,
            mode: Some(mxr_core::SearchMode::Lexical),
            sort: Some(mxr_core::types::SortOrder::DateDesc),
            explain: true,
        }),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data:
                ResponseData::SearchResults {
                    results,
                    explain: Some(explain),
                    ..
                },
        }) => {
            assert!(!results.is_empty());
            assert!(results.len() <= 5);
            assert_eq!(explain.requested_mode, mxr_core::SearchMode::Lexical);
            assert_eq!(explain.executed_mode, mxr_core::SearchMode::Lexical);
            assert_eq!(explain.dense_candidates, 0);
            assert_eq!(explain.final_results as usize, results.len());
            assert_eq!(explain.results.len(), results.len());
        }
        other => panic!("Expected SearchResults with explain payload, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_structured_search_in_semantic_mode_falls_back_to_lexical() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    let msg = IpcMessage {
        id: 13,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Search {
            query: "is:unread".to_string(),
            limit: 10,
            offset: 0,
            mode: Some(mxr_core::SearchMode::Semantic),
            sort: Some(mxr_core::types::SortOrder::DateDesc),
            explain: false,
        }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::SearchResults { results, .. },
        }) => {
            assert!(!results.is_empty());
            assert!(results.len() <= 10);
        }
        other => panic!("Expected SearchResults, got {other:?}"),
    }
}

// Requires the local semantic embedder to populate fallback explanation details;
// gate to the semantic-local lane so the fast lane stays green.
#[cfg(feature = "semantic-local")]
#[tokio::test]
async fn dispatch_structured_search_in_semantic_mode_explains_fallback() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    let msg = IpcMessage {
        id: 14,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Search {
            query: "is:unread".to_string(),
            limit: 10,
            offset: 0,
            mode: Some(mxr_core::SearchMode::Semantic),
            sort: Some(mxr_core::types::SortOrder::DateDesc),
            explain: true,
        }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data:
                ResponseData::SearchResults {
                    explain: Some(explain),
                    ..
                },
        }) => {
            assert_eq!(explain.requested_mode, mxr_core::SearchMode::Semantic);
            assert_eq!(explain.executed_mode, mxr_core::SearchMode::Lexical);
            assert!(explain
                .notes
                .iter()
                .any(|note| note.contains("no semantic text terms")));
        }
        other => panic!("Expected SearchResults with explain payload, got {other:?}"),
    }
}

// Requires the local semantic embedder to populate explain.semantic_query;
// gate to the semantic-local lane so the fast lane stays green.
#[cfg(feature = "semantic-local")]
#[tokio::test]
async fn dispatch_fielded_semantic_query_explains_disabled_fallback() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let mut config = state.config_snapshot();
    config.search.semantic.enabled = false;
    state.set_config_for_test(config).await;

    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    let msg = IpcMessage {
        id: 15,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Search {
            query: "body:deployment".to_string(),
            limit: 10,
            offset: 0,
            mode: Some(mxr_core::SearchMode::Hybrid),
            sort: Some(mxr_core::types::SortOrder::DateDesc),
            explain: true,
        }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data:
                ResponseData::SearchResults {
                    results,
                    explain: Some(explain),
                    ..
                },
        }) => {
            assert!(!results.is_empty());
            assert_eq!(explain.requested_mode, mxr_core::SearchMode::Hybrid);
            assert_eq!(explain.executed_mode, mxr_core::SearchMode::Lexical);
            assert_eq!(explain.semantic_query.as_deref(), Some("deployment"));
            assert!(explain
                .notes
                .iter()
                .any(|note| note.contains("semantic search disabled in config")));
        }
        other => panic!("Expected SearchResults with explain payload, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_search_rejects_invalid_structured_query() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let msg = IpcMessage {
        id: 12,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Search {
            query: "older:30q".to_string(),
            limit: 10,
            offset: 0,
            mode: None,
            sort: None,
            explain: false,
        }),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(message.contains("Invalid search query"));
            assert!(message.contains("invalid date"));
        }
        other => panic!("Expected Error, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_get_body_after_sync() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    // Get first envelope
    let envelopes_msg = IpcMessage {
        id: 11,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListEnvelopes {
            label_id: None,
            account_id: None,
            limit: 1,
            offset: 0,
        }),
    };
    let resp = handle_request(&state, &envelopes_msg).await;
    let message_id = match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Envelopes { envelopes },
        }) => {
            assert_eq!(envelopes.len(), 1);
            envelopes[0].id.clone()
        }
        other => panic!("Expected Envelopes, got {other:?}"),
    };

    // Get body for that envelope
    let body_msg = IpcMessage {
        id: 12,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetBody {
            message_id: message_id.clone(),
        }),
    };
    let resp = handle_request(&state, &body_msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Body { body },
        }) => {
            assert!(
                body.text_plain.is_some(),
                "Body should have text_plain content"
            );
        }
        other => panic!("Expected Body, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_list_bodies_omits_missing_rows() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let missing_id = mxr_core::MessageId::new();

    let msg = IpcMessage {
        id: 13,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListBodies {
            message_ids: vec![missing_id],
        }),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Bodies { bodies, .. },
        }) => {
            assert!(
                bodies.is_empty(),
                "missing body rows should be omitted so clients can retry"
            );
        }
        other => panic!("Expected Bodies, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_get_body_rehydrates_missing_store_row_from_provider() {
    let (state, _) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let id = sync_and_get_first_id(&state).await;

    sqlx::query("DELETE FROM bodies WHERE message_id = ?")
        .bind(id.to_string())
        .execute(state.store.writer())
        .await
        .unwrap();

    let msg = IpcMessage {
        id: 14,
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
            assert!(
                body.text_plain.is_some() || body.text_html.is_some(),
                "provider hydration should restore a readable body"
            );
        }
        other => panic!("Expected Body, got {other:?}"),
    }

    let stored = state.store.get_body(&id).await.unwrap().unwrap();
    assert!(
        stored.text_plain.is_some() || stored.text_html.is_some(),
        "hydrated body should be persisted back into the store"
    );
}

#[tokio::test]
async fn dispatch_list_bodies_stays_local_when_store_row_is_missing() {
    let (state, _) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let id = sync_and_get_first_id(&state).await;

    sqlx::query("DELETE FROM bodies WHERE message_id = ?")
        .bind(id.to_string())
        .execute(state.store.writer())
        .await
        .unwrap();

    let msg = IpcMessage {
        id: 15,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListBodies {
            message_ids: vec![id.clone()],
        }),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Bodies { bodies, failures },
        }) => {
            assert!(bodies.is_empty());
            assert_eq!(failures.len(), 1);
            assert_eq!(failures[0].message_id, id);
            assert!(
                state
                    .store
                    .get_body(&failures[0].message_id)
                    .await
                    .unwrap()
                    .is_none(),
                "bulk prefetch must not repair from provider and block the TUI queue"
            );
        }
        other => panic!("Expected Bodies, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_get_body_rehydrates_legacy_best_effort_body_from_provider() {
    let (state, _) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let id = sync_and_get_first_id(&state).await;

    let stale = mxr_core::types::MessageBody {
        message_id: id.clone(),
        text_plain: Some("No readable body content was available for this message.".into()),
        text_html: None,
        attachments: vec![],
        fetched_at: chrono::Utc::now(),
        metadata: mxr_core::types::MessageMetadata::default(),
    };
    state.store.insert_body(&stale).await.unwrap();

    let msg = IpcMessage {
        id: 19,
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
            assert_ne!(body.text_plain, stale.text_plain);
            assert!(
                body.text_plain.is_some() || body.text_html.is_some(),
                "legacy synthesized body should be replaced with provider content"
            );
        }
        other => panic!("Expected Body, got {other:?}"),
    }

    let stored = state.store.get_body(&id).await.unwrap().unwrap();
    assert_ne!(stored.text_plain, stale.text_plain);
    assert!(
        stored.text_plain.is_some() || stored.text_html.is_some(),
        "rehydrated body should be persisted back into the store"
    );
}

#[tokio::test]
async fn dispatch_get_body_rehydrates_best_effort_summary_when_snippet_implies_real_body() {
    let (state, _) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let id = sync_and_get_first_id(&state).await;

    let stale = mxr_core::types::MessageBody {
        message_id: id.clone(),
        text_plain: Some("No readable body content was available for this message.".into()),
        text_html: None,
        attachments: vec![],
        fetched_at: chrono::Utc::now(),
        metadata: mxr_core::types::MessageMetadata {
            text_plain_source: Some(mxr_core::types::BodyPartSource::BestEffortSummary),
            raw_headers: Some(
                "Content-Type: multipart/alternative; boundary=\"debug-boundary\"".into(),
            ),
            ..Default::default()
        },
    };
    state.store.insert_body(&stale).await.unwrap();

    let msg = IpcMessage {
        id: 20,
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
            assert_ne!(body.text_plain, stale.text_plain);
            assert!(
                body.text_plain.is_some() || body.text_html.is_some(),
                "stored best-effort summaries should be repaired when provider content exists"
            );
        }
        other => panic!("Expected Body, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_list_bodies_preserves_attachments() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;
    let attachment_id = mxr_core::AttachmentId::new();

    state
        .store
        .insert_body(&mxr_core::types::MessageBody {
            message_id: id.clone(),
            text_plain: Some("hello".into()),
            text_html: Some("<p>hello</p>".into()),
            attachments: vec![mxr_core::types::AttachmentMeta {
                id: attachment_id.clone(),
                message_id: id.clone(),
                filename: "report.pdf".into(),
                mime_type: "application/pdf".into(),
                disposition: mxr_core::types::AttachmentDisposition::Attachment,
                content_id: None,
                content_location: None,
                size_bytes: 1024,
                local_path: None,
                provider_id: "att-1".into(),
            }],
            fetched_at: chrono::Utc::now(),
            metadata: Default::default(),
        })
        .await
        .unwrap();

    let msg = IpcMessage {
        id: 16,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListBodies {
            message_ids: vec![id.clone()],
        }),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Bodies { bodies, .. },
        }) => {
            assert_eq!(bodies.len(), 1);
            assert_eq!(bodies[0].text_plain.as_deref(), Some("hello"));
            assert_eq!(bodies[0].text_html.as_deref(), Some("<p>hello</p>"));
            assert_eq!(bodies[0].attachments.len(), 1);
            assert_eq!(bodies[0].attachments[0].id, attachment_id);
            assert_eq!(bodies[0].attachments[0].filename, "report.pdf");
        }
        other => panic!("Expected Bodies, got {other:?}"),
    }
}
