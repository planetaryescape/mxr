use super::*;

#[test]
fn request_lane_routes_llm_and_network_requests_to_bulk() {
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
fn sanitized_attachment_filename_rejects_special_or_unsafe_components() {
    let target_dir = std::path::Path::new("/tmp/mxr-message-cache");

    for filename in [
        ".",
        "..",
        "nested/name.txt",
        "nested\\name.txt",
        "bad\nname.txt",
    ] {
        let attachment_id = mxr_core::AttachmentId::from_provider_id("test", filename);
        let sanitized = sanitized_attachment_filename(filename, &attachment_id);
        let path = target_dir.join(&sanitized);

        assert_ne!(sanitized, ".");
        assert_ne!(sanitized, "..");
        assert!(!sanitized.contains(['/', '\\', ':']));
        assert!(!sanitized.chars().any(char::is_control));
        assert_ne!(path, target_dir);
        assert_ne!(Some(path.as_path()), target_dir.parent());
        assert_eq!(path.parent(), Some(target_dir));
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

    let labels_msg = IpcMessage {
        expected_workspace: None,
        id: 10,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListLabels {
            account_id: None,
            all_accounts: false,
        }),
    };
    let resp = handle_request(&state, &labels_msg).await;
    let labels = match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Labels { labels },
        }) => labels,
        other => panic!("Expected Labels, got {other:?}"),
    };

    let inbox = labels
        .iter()
        .find(|l| l.name == "Inbox")
        .expect("Inbox label missing");

    let msg = IpcMessage {
        expected_workspace: None,
        id: 11,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListEnvelopes {
            label_id: Some(inbox.id.clone()),
            account_id: None,
            limit: 100,
            offset: 0,
            label_role: None,
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
async fn dispatch_create_label_persists_and_returns_label() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let account_id = state.default_account_id();

    let create_msg = IpcMessage {
        expected_workspace: None,
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
        expected_workspace: None,
        id: 15,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListLabels {
            account_id: Some(account_id),
            all_accounts: false,
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
        expected_workspace: None,
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
        expected_workspace: None,
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
            expected_workspace: None,
            id: 22,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::UpsertRule { rule }),
        },
    )
    .await;

    let dry_run_msg = IpcMessage {
        expected_workspace: None,
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
async fn dispatch_delete_label_removes_it_from_store() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let account_id = state.default_account_id();

    let create_msg = IpcMessage {
        expected_workspace: None,
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
        expected_workspace: None,
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
        expected_workspace: None,
        id: 19,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListLabels {
            account_id: Some(account_id),
            all_accounts: false,
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
        expected_workspace: None,
        id: 3,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Count {
            query: "deployment".to_string(),
            account_id: None,
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
async fn dispatch_run_saved_search_returns_results() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    let create = IpcMessage {
        expected_workspace: None,
        id: 200,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::CreateSavedSearch {
            name: "Deploy".into(),
            query: "deployment".into(),
            account_id: None,
            search_mode: mxr_core::SearchMode::Lexical,
        }),
    };
    handle_request(&state, &create).await;

    let msg = IpcMessage {
        expected_workspace: None,
        id: 201,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::RunSavedSearch {
            name: "Deploy".into(),
            limit: 10,
            account_id: None,
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
async fn dispatch_search_aggregation_groups_query_by_sender() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    let msg = IpcMessage {
        expected_workspace: None,
        id: 202,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::SearchAggregation {
            query: "in:inbox".into(),
            account_id: None,
            mode: Some(mxr_core::SearchMode::Lexical),
            group_by: mxr_protocol::SearchAggregationGroupBy::From,
            limit: Some(20),
        }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data:
                ResponseData::SearchAggregation {
                    total,
                    groups,
                    group_by,
                    ..
                },
        }) => {
            assert_eq!(group_by, mxr_protocol::SearchAggregationGroupBy::From);
            assert!(total > 0);
            assert!(!groups.is_empty());
            assert!(groups.iter().all(|row| row.count > 0));
            assert!(groups.iter().all(|row| row.oldest <= row.newest));
        }
        other => panic!("Expected SearchAggregation, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_status() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    let msg = IpcMessage {
        expected_workspace: None,
        id: 7,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetStatus),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data:
                ResponseData::Status {
                    workspace_owner_supported: true,
                    draft_revision_supported: true,
                    send_outcome_supported: true,
                    archive_recovery_supported: true,
                    attachment_transfer_supported: true,
                    draft_attachment_upload_supported: true,
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
async fn status_and_doctor_report_a_stopped_search_worker() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    state.abort_search_worker().await;

    let status = IpcMessage {
        expected_workspace: None,
        id: 12,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetStatus),
    };
    match handle_request(&state, &status).await.payload {
        IpcPayload::Response(Response::Ok {
            data:
                ResponseData::Status {
                    feature_health: Some(feature_health),
                    ..
                },
        }) => match feature_health.search {
            FeatureHealth::Degraded { reason } => {
                assert!(
                    reason.contains("mxr restart"),
                    "a dead search worker must carry the restart hint; got {reason:?}"
                );
            }
            other => panic!("Expected degraded search health, got {other:?}"),
        },
        other => panic!("Expected Status with feature health, got {other:?}"),
    }

    let doctor = IpcMessage {
        expected_workspace: None,
        id: 13,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetDoctorReport),
    };
    match handle_request(&state, &doctor).await.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::DoctorReport { report },
        }) => {
            let finding = report
                .findings
                .iter()
                .find(|finding| finding.message.contains("Search index worker stopped"))
                .expect("doctor must surface a stopped search worker");
            assert_eq!(finding.remediation, vec!["mxr restart".to_string()]);
            assert!(
                !report.healthy,
                "an error-severity finding must make the report unhealthy"
            );
        }
        other => panic!("Expected DoctorReport, got {other:?}"),
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
        expected_workspace: None,
        id: 11,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Search {
            limit_from_config: false,
            query: "deployment".to_string(),
            limit: 5,
            offset: 0,
            account_id: None,
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
async fn dispatch_search_rejects_invalid_structured_query() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let msg = IpcMessage {
        expected_workspace: None,
        id: 12,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Search {
            limit_from_config: false,
            query: "older:30q".to_string(),
            limit: 10,
            offset: 0,
            account_id: None,
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

    let envelopes_msg = IpcMessage {
        expected_workspace: None,
        id: 11,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListEnvelopes {
            label_id: None,
            account_id: None,
            limit: 1,
            offset: 0,
            label_role: None,
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

    let body_msg = IpcMessage {
        expected_workspace: None,
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
        expected_workspace: None,
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
        expected_workspace: None,
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

#[tokio::test]
async fn status_recovery_metadata_does_not_escape_starved_reader_budget() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let held_reader = state.store.reader().acquire().await.unwrap();
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(6),
        crate::handler::diagnostics_impl::get_status(&state),
    )
    .await
    .expect("GetStatus exceeded its bounded snapshot policy")
    .unwrap();
    assert!(matches!(
        response,
        ResponseData::Status {
            degraded: true,
            recovery: None,
            recovery_supported: true,
            ..
        }
    ));
    drop(held_reader);
    state
        .shutdown_runtime_tasks(std::time::Duration::from_secs(2))
        .await;
}

async fn search_defaults_state(
    max_results: usize,
    default_sort: mxr_config::SortOrder,
) -> Arc<AppState> {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();
    let mut config = state.config_snapshot();
    config.search.max_results = max_results;
    config.search.default_sort = default_sort;
    state.set_config_for_test(config).await;
    state
}

async fn hit_dates(
    state: &Arc<AppState>,
    results: &[mxr_protocol::SearchResultItem],
) -> Vec<chrono::DateTime<chrono::Utc>> {
    let message_ids = results.iter().map(|r| r.message_id.clone()).collect();
    let msg = IpcMessage {
        expected_workspace: None,
        id: 41,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListEnvelopesByIds { message_ids }),
    };
    let envelopes = match handle_request(state, &msg).await.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Envelopes { envelopes },
        }) => envelopes,
        other => panic!("Expected Envelopes, got {other:?}"),
    };
    results
        .iter()
        .map(|r| {
            envelopes
                .iter()
                .find(|e| e.id == r.message_id)
                .expect("every hit has an envelope")
                .date
        })
        .collect()
}

async fn dispatch_search_page(
    state: &Arc<AppState>,
    limit: u32,
    sort: Option<mxr_core::types::SortOrder>,
    limit_from_config: bool,
) -> (Vec<mxr_protocol::SearchResultItem>, Option<u32>) {
    let msg = IpcMessage {
        expected_workspace: None,
        id: 40,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Search {
            query: "after:2000-01-01".to_string(),
            limit,
            offset: 0,
            account_id: None,
            mode: None,
            sort,
            explain: false,
            limit_from_config,
        }),
    };
    match handle_request(state, &msg).await.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::SearchResults { results, limit, .. },
        }) => (results, limit),
        other => panic!("Expected SearchResults, got {other:?}"),
    }
}

#[tokio::test]
async fn search_without_its_own_page_size_or_sort_uses_daemon_search_defaults() {
    let state = search_defaults_state(3, mxr_config::SortOrder::DateAsc).await;

    let (results, page_limit) = dispatch_search_page(&state, 50, None, true).await;

    assert_eq!(results.len(), 3, "max_results is the default page size");
    assert_eq!(page_limit, Some(3));
    let dates = hit_dates(&state, &results).await;
    let mut oldest_first = dates.clone();
    oldest_first.sort();
    assert_eq!(
        dates, oldest_first,
        "default_sort = date_asc orders oldest first"
    );
}
