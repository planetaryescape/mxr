use super::desk::{request, Fixture};
use super::*;
use chrono::{Duration, Utc};
use mxr_core::id::{DeliveryId, DraftId, MessageId, ThreadId};
use mxr_core::types::{Address, MessageBody, MessageDirection};
use mxr_protocol::{
    DeskDoneItemData, MailPlaceData, RecordEditData, RecordFilterData, RecordKindData,
    SenderKindData, TodoCatchupDecisionData, TodoStateActionData, TodoStateData,
};
use mxr_store::{
    CommitmentDirection, CommitmentStatus, ContactCommitmentRecord, UndoEntry, UndoEntrySnapshot,
    UndoableMutationKind,
};

struct Owned {
    account: mxr_core::AccountId,
    message: MessageId,
    thread: ThreadId,
    todo: String,
    record: String,
    draft: mxr_core::types::Draft,
    delivery: DeliveryId,
    commitment: String,
    undo: String,
}

async fn seed(fx: &Fixture, account: &mxr_core::AccountId, tag: &str) -> Owned {
    let thread = ThreadId::new();
    let envelope = crate::test_fixtures::TestEnvelopeBuilder::new()
        .account_id(account.clone())
        .thread_id(thread.clone())
        .provider_id(format!("scope-{tag}"))
        .sender_address("Shop", "orders@shop.example")
        .subject(format!("Your receipt {tag}"))
        .date(Utc::now() - Duration::days(1))
        .build();
    let message = envelope.id.clone();
    let store = &fx.state.store;
    store
        .upsert_envelope_with_direction(&envelope, MessageDirection::Inbound)
        .await
        .unwrap();
    store
        .insert_body(&MessageBody {
            text_plain: Some("Thanks for your order. Total: £12.50. Paid by card.".into()),
            ..crate::test_fixtures::make_empty_body(&message)
        })
        .await
        .unwrap();

    let ResponseData::TodoChange { change } = request(
        fx,
        Request::CreateTodo {
            message_id: message.clone(),
            title: format!("Check the order {tag}"),
            kind: None,
            due: None,
            time_zone: None,
            dry_run: false,
        },
    )
    .await
    else {
        panic!("expected a to-do change")
    };
    let todo = change.changed[0].id.clone();

    let ResponseData::RecordChange { change } = request(
        fx,
        Request::FileRecord {
            message_id: message.clone(),
            kind: Some(RecordKindData::Receipt),
            dry_run: false,
        },
    )
    .await
    else {
        panic!("expected a record change")
    };
    let record = change.records[0].id.clone();

    let draft = mxr_core::types::Draft {
        revision: Some(1),
        id: DraftId::new(),
        account_id: account.clone(),
        from: None,
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![Address {
            name: None,
            email: "someone@example.com".into(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: format!("Draft {tag}"),
        content: mxr_core::types::DraftContent::markdown("Body".to_string()),
        inline_assets: Vec::new(),
        managed_attachments: None,
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    store.insert_draft(&draft).await.unwrap();

    let delivery = DeliveryId::new();
    store
        .insert_delivery(&mxr_store::Delivery {
            id: delivery.clone(),
            account_id: account.clone(),
            dedup_key: format!("shop|{tag}"),
            merchant: Some("Shop".into()),
            carrier: None,
            tracking_number: None,
            tracking_url: None,
            order_number: Some(tag.into()),
            status: "ordered".into(),
            eta_from: None,
            eta_until: None,
            delivered_at: None,
            items: vec![],
            confidence: 0.9,
            source: "heuristic".into(),
            thread_id: Some(thread.clone()),
            last_event_at: Utc::now(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            resolved_at: None,
            dismissed_at: None,
        })
        .await
        .unwrap();

    let commitment = format!("c-{tag}");
    store
        .upsert_contact_commitment(&ContactCommitmentRecord {
            id: commitment.clone(),
            account_id: account.clone(),
            email: "orders@shop.example".into(),
            thread_id: thread.clone(),
            direction: CommitmentDirection::Yours,
            status: CommitmentStatus::Open,
            who_owes: "you".into(),
            what: "check the order".into(),
            by_when: Some(Utc::now() + Duration::days(2)),
            by_when_source: Some(mxr_store::ByWhenSource::Words),
            evidence_msg_id: message.clone(),
            extracted_at: Utc::now(),
            resolved_at: None,
        })
        .await
        .unwrap();

    let undo = format!("undo-{tag}");
    store
        .write_undo_entry(&UndoEntry {
            mutation_id: undo.clone(),
            kind: UndoableMutationKind::Archive,
            snapshots: vec![UndoEntrySnapshot {
                message_id: message.clone(),
                account_id: account.clone(),
                provider_id: format!("scope-{tag}"),
                prior_flags_bits: 0,
                prior_label_provider_ids: vec!["INBOX".into()],
                uncertain: false,
            }],
            desk: None,
            applied_at: Utc::now().timestamp(),
            expires_at: (Utc::now() + Duration::minutes(5)).timestamp(),
        })
        .await
        .unwrap();

    Owned {
        account: account.clone(),
        message,
        thread,
        todo,
        record,
        draft,
        delivery,
        commitment,
        undo,
    }
}

struct Scoped {
    fx: Fixture,
    own: Owned,
    other: Owned,
}

async fn scoped() -> Scoped {
    let fx = Fixture::new().await;
    let other_account = mxr_core::AccountId::new();
    fx.state
        .store
        .insert_account(&crate::test_fixtures::test_account_with_id(
            other_account.clone(),
        ))
        .await
        .unwrap();
    let own_account = fx.account.clone();
    let own = seed(&fx, &own_account, "own").await;
    let other = seed(&fx, &other_account, "other").await;

    let mut config = fx.state.config_snapshot();
    config.agent_surfaces.profiles.insert(
        "mcp".into(),
        mxr_config::AgentProfileConfig {
            safety_policy: mxr_config::SafetyPolicy::Full,
            allowed_accounts: vec![own_account.as_str()],
            allow_send: true,
            allow_destructive: true,
            allowed_destructive_actions: vec![],
        },
    );
    fx.state.set_config_for_test(config).await;
    Scoped { fx, own, other }
}

async fn check(s: &Scoped, req: &Request) -> Result<(), String> {
    let config = s.fx.state.config_snapshot();
    enforce_client_profile(&s.fx.state, &config, ::mxr_protocol::ClientKind::Mcp, req).await
}

#[tokio::test]
async fn microsoft_bulk_dispatch_is_limited_to_each_external_profiles_allowed_accounts() {
    for allow_other in [false, true] {
        let s = scoped().await;
        let (allowed, foreign) = if allow_other {
            (&s.other, &s.own)
        } else {
            (&s.own, &s.other)
        };
        let mut config = s.fx.state.config_snapshot();
        config
            .agent_surfaces
            .profiles
            .get_mut("mcp")
            .unwrap()
            .allowed_accounts = vec![allowed.account.as_str()];
        s.fx.state.set_config_for_test(config).await;
        let record =
            s.fx.state
                .store
                .get_todo(&foreign.todo)
                .await
                .unwrap()
                .unwrap();
        assert!(s
            .fx
            .state
            .store
            .claim_todo_microsoft_export(&record, "{}")
            .await
            .unwrap());
        s.fx.state
            .store
            .accept_todo_microsoft_export(
                &foreign.todo,
                "synthetic-remote-never-read",
                "operation",
                "{}",
            )
            .await
            .unwrap();
        let bulk = Request::SyncMicrosoftTodos {
            todo_id: None,
            account_id: None,
            dry_run: true,
        };
        assert!(check(&s, &bulk).await.is_ok());
        assert!(
            matches!(scoped_dispatch(&s, bulk).await, Response::Ok { data: ResponseData::MicrosoftTodoSync { sync } } if sync.results.is_empty())
        );
        let own_preview = Request::SendTodoToMicrosoft {
            todo_id: allowed.todo.clone(),
            account_id: Some(allowed.account.clone()),
            dry_run: true,
            fingerprint: None,
        };
        assert!(
            matches!(scoped_dispatch(&s, own_preview).await, Response::Ok { data: ResponseData::MicrosoftTodoSend { result } } if result.todo_id == allowed.todo)
        );
        for denied in [
            Request::SyncMicrosoftTodos {
                todo_id: None,
                account_id: Some(foreign.account.clone()),
                dry_run: true,
            },
            Request::SendTodoToMicrosoft {
                todo_id: foreign.todo.clone(),
                account_id: Some(allowed.account.clone()),
                dry_run: true,
                fingerprint: None,
            },
        ] {
            assert!(
                matches!(scoped_dispatch(&s, denied).await, Response::Error { message, .. } if message.contains("account allowlist"))
            );
        }
        assert!(
            matches!(scoped_dispatch(&s, Request::ListRecords { account_id: None, filter: RecordFilterData::default(), limit: 10, offset: 0 }).await, Response::Error { message, .. } if message.contains("account allowlist"))
        );
        let mut config = s.fx.state.config_snapshot();
        config
            .agent_surfaces
            .profiles
            .get_mut("mcp")
            .unwrap()
            .allowed_accounts = vec![allowed.account.as_str(), foreign.account.as_str()];
        s.fx.state.set_config_for_test(config).await;
        for mismatch in [
            Request::SendTodoToMicrosoft {
                todo_id: foreign.todo.clone(),
                account_id: Some(allowed.account.clone()),
                dry_run: true,
                fingerprint: None,
            },
            Request::SyncMicrosoftTodos {
                todo_id: Some(foreign.todo.clone()),
                account_id: Some(allowed.account.clone()),
                dry_run: true,
            },
        ] {
            assert!(
                matches!(scoped_dispatch(&s, mismatch).await, Response::Error { message, .. } if message.contains("does not belong"))
            );
        }
    }
}

async fn assert_scoped(s: &Scoped, build: impl Fn(&Owned) -> Request) {
    let own = build(&s.own);
    if let Err(error) = check(s, &own).await {
        panic!(
            "`{}` for the agent's own account was denied: {error}",
            request_kind(&own)
        );
    }
    let other = build(&s.other);
    match check(s, &other).await {
        Ok(()) => panic!("`{}` for another account was allowed", request_kind(&other)),
        Err(error) => assert!(
            error.contains("account allowlist"),
            "`{}` denied for the wrong reason: {error}",
            request_kind(&other)
        ),
    }
}

async fn assert_denied(s: &Scoped, req: &Request) {
    match check(s, req).await {
        Ok(()) => panic!("`{}` was allowed", request_kind(req)),
        Err(error) => assert!(
            error.contains("account allowlist"),
            "`{}` denied for the wrong reason: {error}",
            request_kind(req)
        ),
    }
}

#[tokio::test]
async fn archive_requests_stay_in_the_agents_accounts() {
    let s = scoped().await;
    assert_scoped(&s, |o| Request::ListRecords {
        account_id: Some(o.account.clone()),
        filter: RecordFilterData::default(),
        limit: 10,
        offset: 0,
    })
    .await;
    assert_scoped(&s, |o| Request::AnswerFromRecords {
        query: "how much was my order".into(),
        account_id: Some(o.account.clone()),
        fallback: false,
        limit: 4,
        list: false,
        offset: 0,
        list_limit: 200,
    })
    .await;
    assert_scoped(&s, |o| Request::AnswerFromRecords {
        query: "order".into(),
        account_id: Some(o.account.clone()),
        fallback: false,
        limit: 4,
        list: true,
        offset: 0,
        list_limit: 200,
    })
    .await;
    assert_scoped(&s, |o| Request::ExportRecords {
        account_id: Some(o.account.clone()),
        filter: RecordFilterData::default(),
        attachments_dir: None,
        dry_run: true,
    })
    .await;
    assert_scoped(&s, |o| Request::GetRecord {
        record_id: o.record.clone(),
    })
    .await;
    assert_scoped(&s, |o| Request::SetRecordField {
        record_id: o.record.clone(),
        edit: RecordEditData::ConfirmAll,
        apply_to_sender: false,
        dry_run: true,
    })
    .await;
    assert_scoped(&s, |o| Request::DismissRecord {
        record_ids: vec![o.record.clone()],
        restore: false,
        dry_run: true,
    })
    .await;
    assert_scoped(&s, |o| Request::FileRecord {
        message_id: o.message.clone(),
        kind: None,
        dry_run: true,
    })
    .await;
    assert_scoped(&s, |o| Request::SetRecordSender {
        message_id: o.message.clone(),
        verdict: Some("always".into()),
        kind: None,
        dry_run: true,
    })
    .await;
    assert_denied(
        &s,
        &Request::DismissRecord {
            record_ids: vec![s.own.record.clone(), s.other.record.clone()],
            restore: false,
            dry_run: true,
        },
    )
    .await;
}

#[tokio::test]
async fn todo_requests_stay_in_the_agents_accounts() {
    let s = scoped().await;
    assert_scoped(&s, |o| Request::GetTodoRunway {
        account_id: Some(o.account.clone()),
        mark_seen: false,
    })
    .await;
    assert_scoped(&s, |o| Request::ListTodos {
        account_id: Some(o.account.clone()),
        state: TodoStateData::Open,
        limit: 10,
    })
    .await;
    assert_scoped(&s, |o| Request::GetTodoCatchup {
        account_id: Some(o.account.clone()),
    })
    .await;
    assert_scoped(&s, |o| Request::SetTodoCatchup {
        account_id: Some(o.account.clone()),
        decision: TodoCatchupDecisionData::LetGoAll,
        dry_run: true,
    })
    .await;
    assert_scoped(&s, |o| Request::GetTodo {
        todo_id: o.todo.clone(),
    })
    .await;
    assert_scoped(&s, |o| Request::SetTodoState {
        todo_ids: vec![o.todo.clone()],
        action: TodoStateActionData::Done,
        dry_run: true,
    })
    .await;
    assert_scoped(&s, |o| Request::ScheduleTodo {
        todo_id: o.todo.clone(),
        when: Some("in 3d".into()),
        time_zone: None,
        dry_run: true,
    })
    .await;
    assert_scoped(&s, |o| Request::UpdateTodo {
        todo_id: o.todo.clone(),
        edits: vec![],
        time_zone: None,
        dry_run: true,
    })
    .await;
    assert_scoped(&s, |o| Request::CreateTodo {
        message_id: o.message.clone(),
        title: "Reply".into(),
        kind: None,
        due: None,
        time_zone: None,
        dry_run: true,
    })
    .await;
}

#[tokio::test]
async fn messages_requests_stay_in_the_agents_accounts() {
    let s = scoped().await;
    assert_scoped(&s, |o| Request::ListMessages {
        account_id: Some(o.account.clone()),
        turn: None,
        filter: Default::default(),
        limit: 10,
    })
    .await;
    assert_scoped(&s, |o| Request::GetPerson {
        account_id: Some(o.account.clone()),
        person: "orders@shop.example".into(),
        topic: None,
        conversation: true,
    })
    .await;
    assert_scoped(&s, |o| Request::AckMessage {
        thread_id: o.thread.clone(),
        dry_run: true,
        expect_text: None,
        preview_token: None,
    })
    .await;
    assert_scoped(&s, |o| Request::MergePeople {
        account_id: o.account.clone(),
        into: "a@example.com".into(),
        addresses: vec!["b@example.com".into()],
        dry_run: true,
    })
    .await;
    assert_scoped(&s, |o| Request::SplitPerson {
        account_id: o.account.clone(),
        address: "b@example.com".into(),
        dry_run: true,
    })
    .await;
    assert_scoped(&s, |o| Request::ListMergeSuggestions {
        account_id: Some(o.account.clone()),
    })
    .await;
    assert_denied(
        &s,
        &Request::GetPerson {
            account_id: Some(s.own.account.clone()),
            person: "orders@shop.example".into(),
            topic: Some(s.other.thread.clone()),
            conversation: true,
        },
    )
    .await;
}

#[tokio::test]
async fn thread_desk_and_place_requests_stay_in_the_agents_accounts() {
    let s = scoped().await;
    assert_scoped(&s, |o| Request::GetThreadContext {
        thread_id: o.thread.clone(),
    })
    .await;
    assert_scoped(&s, |o| Request::GetThreadGist {
        thread_id: o.thread.clone(),
        refresh: false,
    })
    .await;
    assert_scoped(&s, |o| Request::GetThreadGists {
        thread_ids: vec![o.thread.clone()],
        generate: false,
        automated: false,
    })
    .await;
    assert_scoped(&s, |o| Request::GetThreadBriefing {
        thread_id: o.thread.clone(),
        refresh: false,
    })
    .await;
    assert_scoped(&s, |o| Request::DeferThreads {
        thread_ids: vec![o.thread.clone()],
        until: Utc::now() + Duration::days(1),
        dry_run: true,
    })
    .await;
    assert_scoped(&s, |o| Request::DismissDeskThreads {
        thread_ids: vec![o.thread.clone()],
        dry_run: true,
    })
    .await;
    assert_scoped(&s, |o| Request::ResolveDeskItems {
        items: vec![DeskDoneItemData {
            thread_id: o.thread.clone(),
            lane: None,
            commitment_id: None,
        }],
        dry_run: true,
    })
    .await;
    assert_denied(
        &s,
        &Request::ResolveDeskItems {
            items: vec![DeskDoneItemData {
                thread_id: s.own.thread.clone(),
                lane: None,
                commitment_id: Some(s.other.commitment.clone()),
            }],
            dry_run: true,
        },
    )
    .await;
    assert_scoped(&s, |o| Request::ListPlace {
        place: MailPlaceData::PaperTrail,
        account_id: Some(o.account.clone()),
        sender_email: None,
        limit: 10,
        offset: 0,
        messages_per_bundle: 3,
        message_offset: 0,
    })
    .await;
    assert_scoped(&s, |o| Request::SweepPlace {
        place: MailPlaceData::PaperTrail,
        account_id: Some(o.account.clone()),
        sender_email: None,
        dry_run: true,
        preview_token: None,
    })
    .await;
    assert_scoped(&s, |o| Request::PinMessages {
        message_ids: vec![o.message.clone()],
        pinned: true,
    })
    .await;
    assert_scoped(&s, |o| Request::GetMessageKind {
        message_id: o.message.clone(),
    })
    .await;
    assert_scoped(&s, |o| Request::SetSenderKind {
        account_id: o.account.clone(),
        sender_email: "orders@shop.example".into(),
        kind: Some(SenderKindData::PaperTrail),
    })
    .await;
}

#[tokio::test]
async fn previously_unlisted_requests_stay_in_the_agents_accounts() {
    let s = scoped().await;
    assert_scoped(&s, |o| Request::GetSenderProfile {
        account_id: o.account.clone(),
        email: "orders@shop.example".into(),
    })
    .await;
    assert_scoped(&s, |o| Request::ListScreenerQueue {
        account_id: o.account.clone(),
        limit: 10,
        all_senders: false,
    })
    .await;
    assert_scoped(&s, |o| Request::TriageSearch {
        query: "receipt".into(),
        limit: 10,
        offset: 0,
        account_id: Some(o.account.clone()),
        mode: None,
        sort: None,
    })
    .await;
    assert_scoped(&s, |o| Request::UnsubscribePurge {
        address: "orders@shop.example".into(),
        account_id: Some(o.account.clone()),
        dry_run: true,
        archive_on_no_method: false,
        preview_token: None,
    })
    .await;
    assert_scoped(&s, |o| Request::CommitUnsubscribePreview {
        address: "orders@shop.example".into(),
        account_id: Some(o.account.clone()),
        preview_token: "tok".into(),
        archive: false,
        archive_on_no_method: false,
    })
    .await;
    assert_scoped(&s, |o| Request::ExportSearch {
        query: "receipt".into(),
        account_id: Some(o.account.clone()),
        format: ExportFormat::Json,
    })
    .await;
    assert_scoped(&s, |o| Request::SetFlags {
        message_id: o.message.clone(),
        flags: mxr_core::types::MessageFlags::READ,
    })
    .await;
    assert_scoped(&s, |o| Request::SetAutoReminder {
        sent_message_id: o.message.clone(),
        remind_at: Utc::now() + Duration::days(1),
    })
    .await;
    assert_scoped(&s, |o| Request::GetDelivery {
        delivery_id: o.delivery.clone(),
    })
    .await;
    assert_scoped(&s, |o| Request::ResolveCommitment {
        commitment_id: o.commitment.clone(),
    })
    .await;
    assert_scoped(&s, |o| Request::UndoMutation {
        mutation_id: o.undo.clone(),
    })
    .await;
    assert_scoped(&s, |o| Request::ResetOrphanedDraft {
        draft_id: o.draft.id.clone(),
    })
    .await;
    let other_inbox = crate::test_fixtures::test_label(&s.other.account, "Inbox", "INBOX");
    s.fx.state.store.upsert_label(&other_inbox).await.unwrap();
    assert_scoped(&s, |o| Request::ListEnvelopes {
        label_id: None,
        account_id: Some(o.account.clone()),
        limit: 10,
        offset: 0,
        label_role: None,
    })
    .await;
    let foreign_label = Request::ListEnvelopes {
        label_id: Some(other_inbox.id.clone()),
        account_id: Some(s.own.account.clone()),
        limit: 10,
        offset: 0,
        label_role: None,
    };
    let error = check(&s, &foreign_label).await.unwrap_err();
    assert!(error.contains("is not in account"), "{error}");
    let mut hijack = s.other.draft.clone();
    hijack.account_id = s.own.account.clone();
    assert_denied(&s, &Request::UpdateDraft { draft: hijack }).await;
}

#[tokio::test]
async fn requests_spanning_every_account_are_denied_and_unscoped_ones_allowed() {
    let s = scoped().await;
    for req in [
        Request::ListRecords {
            account_id: None,
            filter: RecordFilterData::default(),
            limit: 10,
            offset: 0,
        },
        Request::ListTodos {
            account_id: None,
            state: TodoStateData::Open,
            limit: 10,
        },
        Request::GetNow { account_id: None },
        Request::GetArrivals {
            account_id: None,
            mark_seen: false,
            since: None,
        },
        Request::ListMessages {
            account_id: None,
            turn: None,
            filter: Default::default(),
            limit: 10,
        },
        Request::ListRules,
        Request::ListActivity {
            filter: mxr_protocol::ActivityFilter::default(),
            limit: 10,
            cursor: None,
        },
        Request::GetLogs {
            limit: 10,
            level: None,
            search: None,
        },
    ] {
        assert_denied(&s, &req).await;
    }
    for req in [
        Request::Ping,
        Request::GetStatus,
        Request::GetModeGuide { mode: None },
        Request::SetHintSeen {
            hint: "todo.runway".into(),
            seen: true,
        },
        Request::ResolveTime {
            input: "tomorrow 9am".into(),
            now: None,
            time_zone: None,
        },
    ] {
        check(&s, &req)
            .await
            .unwrap_or_else(|error| panic!("`{}` was denied: {error}", request_kind(&req)));
    }
}

#[tokio::test]
async fn mcp_category_catalog_requires_full_account_scope_and_preserves_profile_gates() {
    let s = scoped().await;
    let mut config = s.fx.state.config_snapshot();
    config.accounts.insert(
        "configured-only".into(),
        mxr_config::AccountConfig {
            color: None,
            tag: None,
            name: "Configured only".into(),
            email: "configured-only@example.com".into(),
            enabled: false,
            sync: None,
            send: None,
        },
    );
    s.fx.state.set_config_for_test(config).await;

    let denied = scoped_dispatch(
        &s,
        Request::CreateCategory {
            name: "Restricted catalog".into(),
        },
    )
    .await;
    match denied {
        Response::Error { message, .. } => {
            assert!(message.contains("category catalog access"), "{message}");
        }
        other => panic!("partial account profile unexpectedly created a category: {other:?}"),
    }

    s.fx.state
        .store
        .set_account_enabled(&s.other.account, false)
        .await
        .unwrap();
    let mut config = s.fx.state.config_snapshot();
    config
        .agent_surfaces
        .profiles
        .get_mut("mcp")
        .unwrap()
        .allowed_accounts = vec![s.own.account.to_string(), "configured-only".into()];
    s.fx.state.set_config_for_test(config).await;
    assert!(matches!(
        scoped_dispatch(
            &s,
            Request::CreateCategory {
                name: "Disabled account omitted".into(),
            },
        )
        .await,
        Response::Error { message, .. } if message.contains("category catalog access")
    ));

    let mut config = s.fx.state.config_snapshot();
    let mcp = config.agent_surfaces.profiles.get_mut("mcp").unwrap();
    mcp.allowed_accounts = vec![
        s.own.account.to_string(),
        s.other.account.to_string(),
        "configured-only".into(),
    ];
    s.fx.state.set_config_for_test(config).await;

    let Response::Ok {
        data:
            ResponseData::CategoryNamePreview {
                normalized_name,
                available,
            },
    } = scoped_dispatch(
        &s,
        Request::PreviewCategoryName {
            category_id: None,
            name: "  Travel  ".into(),
        },
    )
    .await
    else {
        panic!("full account profile should be able to preview category creation")
    };
    assert_eq!(normalized_name, "Travel");
    assert!(available);
    assert!(s
        .fx
        .state
        .store
        .list_personal_categories(None)
        .await
        .unwrap()
        .is_empty());

    let mut config = s.fx.state.config_snapshot();
    config
        .agent_surfaces
        .profiles
        .get_mut("mcp")
        .unwrap()
        .safety_policy = mxr_config::SafetyPolicy::ReadOnly;
    s.fx.state.set_config_for_test(config).await;
    assert!(matches!(
        scoped_dispatch(
            &s,
            Request::CreateCategory {
                name: "Blocked by read only".into(),
            },
        )
        .await,
        Response::Error { message, .. } if message.contains("safety policy")
    ));

    let mut config = s.fx.state.config_snapshot();
    let mcp = config.agent_surfaces.profiles.get_mut("mcp").unwrap();
    mcp.safety_policy = mxr_config::SafetyPolicy::Full;
    mcp.allow_destructive = false;
    s.fx.state.set_config_for_test(config).await;
    assert!(matches!(
        scoped_dispatch(
            &s,
            Request::CreateCategory {
                name: "Blocked by destructive gate".into(),
            },
        )
        .await,
        Response::Error { message, .. } if message.contains("destructive gate")
    ));

    let mut config = s.fx.state.config_snapshot();
    config
        .agent_surfaces
        .profiles
        .get_mut("mcp")
        .unwrap()
        .allow_destructive = true;
    s.fx.state.set_config_for_test(config).await;

    let Response::Ok {
        data: ResponseData::Categories { categories },
    } = scoped_dispatch(&s, Request::ListCategories { account_id: None }).await
    else {
        panic!("full account profile should list the global category catalog")
    };
    assert!(categories.is_empty());

    let created = scoped_dispatch(
        &s,
        Request::CreateCategory {
            name: "Travel".into(),
        },
    )
    .await;
    let categories = match created {
        Response::Ok {
            data: ResponseData::Categories { categories },
        } => categories,
        other => panic!("full account profile should create a category: {other:?}"),
    };
    let category = categories.into_iter().next().unwrap();

    let Response::Ok {
        data: ResponseData::Categories { categories },
    } = scoped_dispatch(&s, Request::ListCategories { account_id: None }).await
    else {
        panic!("full account profile should see an unassigned category")
    };
    assert_eq!(categories.len(), 1);
    assert_eq!(categories[0].total_count, 0);

    let Response::Ok {
        data: ResponseData::CategoryMembershipPreview { preview },
    } = scoped_dispatch(
        &s,
        Request::PreviewCategoryMembership {
            account_id: None,
            message_ids: vec![s.own.message.clone()],
            add_category_ids: vec![category.id.clone()],
            remove_category_ids: vec![],
        },
    )
    .await
    else {
        panic!("full account profile should preview a cross-account assignment")
    };
    assert!(matches!(
        scoped_dispatch(
            &s,
            Request::ApplyCategoryMembership {
                preview_token: preview.preview_token,
            },
        )
        .await,
        Response::Ok {
            data: ResponseData::CategoryMembershipApplied {
                added: 1,
                removed: 0,
                ..
            }
        }
    ));

    let Response::Ok {
        data: ResponseData::CategoryMessages { total, .. },
    } = scoped_dispatch(
        &s,
        Request::ListCategoryMessages {
            category_id: category.id.clone(),
            account_id: None,
            limit: 50,
            offset: 0,
        },
    )
    .await
    else {
        panic!("full account profile should read category messages across accounts")
    };
    assert_eq!(total, 1);

    let Response::Ok {
        data: ResponseData::CategoryNamePreview { available, .. },
    } = scoped_dispatch(
        &s,
        Request::PreviewCategoryName {
            category_id: Some(category.id.clone()),
            name: "Trips".into(),
        },
    )
    .await
    else {
        panic!("full account profile should preview category rename")
    };
    assert!(available);
    assert!(matches!(
        scoped_dispatch(
            &s,
            Request::RenameCategory {
                category_id: category.id.clone(),
                new_name: "Trips".into(),
            },
        )
        .await,
        Response::Ok {
            data: ResponseData::Categories { .. }
        }
    ));

    let Response::Ok {
        data: ResponseData::CategoryDeletePreview { preview },
    } = scoped_dispatch(
        &s,
        Request::PreviewDeleteCategory {
            category_id: category.id.clone(),
        },
    )
    .await
    else {
        panic!("full account profile should preview category deletion")
    };
    assert_eq!(preview.affected_messages, 1);
    let mut config = s.fx.state.config_snapshot();
    config
        .agent_surfaces
        .profiles
        .get_mut("mcp")
        .unwrap()
        .allow_destructive = false;
    s.fx.state.set_config_for_test(config).await;
    assert!(matches!(
        scoped_dispatch(
            &s,
            Request::DeleteCategory {
                preview_token: preview.preview_token,
            },
        )
        .await,
        Response::Error { message, .. } if message.contains("destructive gate")
    ));

    let mut config = s.fx.state.config_snapshot();
    config
        .agent_surfaces
        .profiles
        .get_mut("mcp")
        .unwrap()
        .allow_destructive = true;
    s.fx.state.set_config_for_test(config).await;
    let Response::Ok {
        data: ResponseData::CategoryDeletePreview { preview },
    } = scoped_dispatch(
        &s,
        Request::PreviewDeleteCategory {
            category_id: category.id,
        },
    )
    .await
    else {
        panic!("full account profile should be able to obtain a fresh delete preview")
    };
    assert!(matches!(
        scoped_dispatch(
            &s,
            Request::DeleteCategory {
                preview_token: preview.preview_token,
            },
        )
        .await,
        Response::Ok {
            data: ResponseData::CategoryDeleted { .. }
        }
    ));
    let Response::Ok {
        data: ResponseData::Categories { categories },
    } = scoped_dispatch(&s, Request::ListCategories { account_id: None }).await
    else {
        panic!("full account profile should see the deleted category removed")
    };
    assert!(categories.is_empty());
}

async fn scoped_dispatch(s: &Scoped, req: Request) -> Response {
    let msg = IpcMessage {
        expected_workspace: None,
        id: 11,
        source: ::mxr_protocol::ClientKind::Mcp,
        payload: IpcPayload::Request(req),
    };
    match handle_request(&s.fx.state, &msg).await.payload {
        IpcPayload::Response(response) => response,
        other => panic!("expected a response, got {other:?}"),
    }
}

fn scoped_profile(s: &Scoped) -> mxr_config::AgentProfileConfig {
    s.fx.state
        .config_snapshot()
        .agent_surfaces
        .profiles
        .get("mcp")
        .cloned()
        .unwrap()
}

#[tokio::test]
async fn a_thread_id_shared_with_another_account_is_denied_and_filtered() {
    let s = scoped().await;
    let shared = ThreadId::new();
    for (account, tag) in [(&s.own.account, "own"), (&s.other.account, "foreign")] {
        let envelope = crate::test_fixtures::TestEnvelopeBuilder::new()
            .account_id(account.clone())
            .thread_id(shared.clone())
            .provider_id(format!("scope-shared-{tag}"))
            .subject("Shared conversation")
            .date(Utc::now())
            .build();
        s.fx.state
            .store
            .upsert_envelope_with_direction(&envelope, MessageDirection::Inbound)
            .await
            .unwrap();
    }
    let mut holders =
        super::super::account_scope::thread_accounts(&s.fx.state, std::slice::from_ref(&shared))
            .await
            .unwrap();
    holders.sort_by_key(mxr_core::AccountId::as_str);
    let mut expected = vec![s.other.account.clone(), s.own.account.clone()];
    expected.sort_by_key(mxr_core::AccountId::as_str);
    assert_eq!(holders, expected);

    for req in [
        Request::GetThread {
            thread_id: shared.clone(),
            window: None,
        },
        Request::GetThreadContext {
            thread_id: shared.clone(),
        },
        Request::GetThreadGists {
            thread_ids: vec![shared.clone()],
            generate: false,
            automated: false,
        },
        Request::ExportThread {
            thread_id: shared.clone(),
            format: ExportFormat::Json,
        },
    ] {
        assert_denied(&s, &req).await;
    }

    let thread = s.fx.state.store.get_thread(&shared).await.unwrap().unwrap();
    let messages =
        s.fx.state
            .store
            .get_thread_envelopes(&shared)
            .await
            .unwrap();
    assert_eq!(messages.len(), 2, "the fixture shares the thread id");
    let filtered = super::super::account_scope::scope_response(
        &s.fx.state,
        &scoped_profile(&s),
        ResponseData::Thread {
            thread,
            messages,
            summary: None,
        },
    )
    .await
    .unwrap();
    let ResponseData::Thread { messages, .. } = filtered else {
        panic!("expected a thread")
    };
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].account_id, s.own.account);
}

#[tokio::test]
async fn status_shows_a_scoped_client_only_its_accounts() {
    let s = scoped().await;
    let mut other = crate::test_fixtures::test_account_with_id(s.other.account.clone());
    other.name = "Other Account".into();
    s.fx.state.store.insert_account(&other).await.unwrap();

    let Response::Ok {
        data:
            ResponseData::Status {
                accounts,
                sync_statuses,
                total_messages,
                protocol_version,
                ..
            },
    } = scoped_dispatch(&s, Request::GetStatus).await
    else {
        panic!("expected status")
    };
    assert_eq!(protocol_version, IPC_PROTOCOL_VERSION);
    assert!(
        !accounts.iter().any(|name| name == "Other Account"),
        "{accounts:?}"
    );
    assert!(sync_statuses
        .iter()
        .all(|status| status.account_id == s.own.account));
    let own_count =
        s.fx.state
            .store
            .count_messages_grouped_by_account()
            .await
            .unwrap()
            .get(&s.own.account)
            .copied()
            .unwrap_or(0);
    assert_eq!(total_messages, own_count);
}

#[tokio::test]
async fn recovery_requires_full_account_coverage_and_commits_keep_safety_permissions() {
    let s = scoped().await;
    let partial = scoped_profile(&s);
    let mut full = partial.clone();
    full.allowed_accounts.extend(
        s.fx.state
            .store
            .list_all_accounts()
            .await
            .unwrap()
            .into_iter()
            .map(|account| account.id.as_str()),
    );
    full.allowed_accounts
        .extend(s.fx.state.config_snapshot().accounts.into_keys());
    let owner = mxr_protocol::ArchiveWorkspaceData {
        workspace_id: "synthetic-workspace".into(),
        recovery_generation: "synthetic-generation".into(),
    };
    let previews = [
        Request::GetRecoveryStatus,
        Request::ActivateRecovery {
            expected_workspace: owner.clone(),
            source_owner_stopped: false,
            dry_run: true,
        },
        Request::PrepareRecoveredDraft {
            draft_id: s.own.draft.id.clone(),
            expected_revision: 1,
            expected_workspace: owner.clone(),
            dry_run: true,
        },
    ];
    for request in &previews {
        assert!(super::super::account_scope::enforce_account_allowlist(
            &s.fx.state,
            "mcp",
            &partial,
            request
        )
        .await
        .is_err());
        assert!(super::super::account_scope::enforce_account_allowlist(
            &s.fx.state,
            "mcp",
            &full,
            request
        )
        .await
        .is_ok());
        assert!(
            super::super::enforce_safety_policy(mxr_config::SafetyPolicy::ReadOnly, request)
                .is_ok()
        );
    }
    for request in [
        Request::ActivateRecovery {
            expected_workspace: owner.clone(),
            source_owner_stopped: true,
            dry_run: false,
        },
        Request::PrepareRecoveredDraft {
            draft_id: s.own.draft.id.clone(),
            expected_revision: 1,
            expected_workspace: owner,
            dry_run: false,
        },
    ] {
        assert!(super::super::account_scope::enforce_account_allowlist(
            &s.fx.state,
            "mcp",
            &partial,
            &request
        )
        .await
        .is_err());
        assert!(super::super::account_scope::enforce_account_allowlist(
            &s.fx.state,
            "mcp",
            &full,
            &request
        )
        .await
        .is_ok());
        assert!(
            super::super::enforce_safety_policy(mxr_config::SafetyPolicy::ReadOnly, &request)
                .is_err()
        );
        assert!(
            super::super::enforce_safety_policy(mxr_config::SafetyPolicy::Full, &request).is_ok()
        );
    }
}

#[tokio::test]
async fn restricted_profile_cannot_inspect_or_approve_global_pairing_authority() {
    use mxr_core::{RemoteCapability, RemoteSecret};
    use mxr_protocol::{RemoteRequest, RemoteResponse};

    let s = scoped().await;
    let ResponseData::RemoteAuthority {
        response: RemoteResponse::Pairing { pairing, .. },
    } = request(
        &s.fx,
        Request::RemoteAuthority {
            request: RemoteRequest::StartPairing {
                name: "Profile boundary fixture".into(),
                capabilities: vec![RemoteCapability::MailRead],
                mail_verifier: RemoteSecret::new("a".repeat(64)),
                cleanup_verifier: RemoteSecret::new("b".repeat(64)),
            },
        },
    )
    .await
    else {
        panic!("unrestricted local CLI should start pairing")
    };
    for action in [
        RemoteRequest::Status,
        RemoteRequest::Pairings,
        RemoteRequest::Clients,
        RemoteRequest::ApprovePairing {
            request_id: pairing.request_id.clone(),
            dry_run: false,
        },
    ] {
        match scoped_dispatch(&s, Request::RemoteAuthority { request: action }).await {
            Response::Error { message, .. } => {
                assert!(message.contains("account allowlist"), "{message}");
            }
            other => panic!("restricted profile reached global authority: {other:?}"),
        }
    }
    let ResponseData::RemoteAuthority {
        response: RemoteResponse::Pairings { pairings },
    } = request(
        &s.fx,
        Request::RemoteAuthority {
            request: RemoteRequest::Pairings,
        },
    )
    .await
    else {
        panic!("unrestricted local CLI should inspect pending pairings")
    };
    assert_eq!(pairings.len(), 1);
    assert!(
        !pairings[0].approved,
        "denied approval must not mutate authority"
    );
    let ResponseData::RemoteAuthority {
        response: RemoteResponse::Pairing {
            pairing: approved, ..
        },
    } = request(
        &s.fx,
        Request::RemoteAuthority {
            request: RemoteRequest::ApprovePairing {
                request_id: pairing.request_id,
                dry_run: false,
            },
        },
    )
    .await
    else {
        panic!("unrestricted local CLI should approve the same pairing")
    };
    assert!(approved.approved);
}
