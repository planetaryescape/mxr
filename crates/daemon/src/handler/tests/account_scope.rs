//! A scoped agent profile reaches only its own accounts: every request
//! family is denied for another account's data, by account id or by the id
//! of something the other account owns, and allowed for its own.

use super::desk::{request, Fixture};
use super::*;
use chrono::{Duration, Utc};
use mxr_core::id::{DeliveryId, DraftId, MessageId, ThreadId};
use mxr_core::types::{Address, MessageBody, MessageDirection};
use mxr_protocol::{
    DeskDoneItemData, MailPlaceData, ModeDoneSenderData, ModeKindData, RecordEditData,
    RecordFilterData, RecordKindData, SenderKindData, TodoCatchupDecisionData, TodoStateActionData,
    TodoStateData,
};
use mxr_store::{
    CommitmentDirection, CommitmentStatus, ContactCommitmentRecord, UndoEntry, UndoEntrySnapshot,
    UndoableMutationKind,
};

/// What one account owns, for naming in requests.
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

/// An MCP profile allowed only the default account, with every other gate
/// open so the account allowlist is the only thing that can say no.
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

/// Each request built for the scoped agent's own account is allowed and
/// the same request built for the other account is denied.
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
    // A mix of own and other records is denied as a whole.
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
async fn mode_requests_stay_in_the_agents_accounts() {
    let s = scoped().await;
    assert_scoped(&s, |o| Request::GetNow {
        account_id: Some(o.account.clone()),
    })
    .await;
    assert_scoped(&s, |o| Request::GetRail {
        account_id: Some(o.account.clone()),
    })
    .await;
    assert_scoped(&s, |o| Request::GetModeMembership {
        message_id: None,
        thread_id: Some(o.thread.clone()),
        thread_ids: vec![],
    })
    .await;
    assert_scoped(&s, |o| Request::GetModeMembership {
        message_id: Some(o.message.clone()),
        thread_id: None,
        thread_ids: vec![],
    })
    .await;
    assert_scoped(&s, |o| Request::SetModeDone {
        thread_ids: vec![o.thread.clone()],
        mode: ModeKindData::Updates,
        dry_run: true,
        todo_ids: vec![],
        sender: None,
    })
    .await;
    // To do names its rows by id; another account's row is out of scope
    // even with no thread named.
    assert_scoped(&s, |o| Request::SetModeDone {
        thread_ids: vec![],
        mode: ModeKindData::Todo,
        dry_run: true,
        todo_ids: vec![o.todo.clone()],
        sender: None,
    })
    .await;
    assert_scoped(&s, |o| Request::SetModeDone {
        thread_ids: vec![],
        mode: ModeKindData::Updates,
        dry_run: true,
        todo_ids: vec![],
        sender: Some(ModeDoneSenderData {
            account_id: o.account.clone(),
            sender_email: "orders@shop.example".into(),
        }),
    })
    .await;
}

#[tokio::test]
async fn messages_requests_stay_in_the_agents_accounts() {
    let s = scoped().await;
    assert_scoped(&s, |o| Request::ListMessages {
        account_id: Some(o.account.clone()),
        turn: None,
        limit: 10,
    })
    .await;
    assert_scoped(&s, |o| Request::GetPerson {
        account_id: Some(o.account.clone()),
        person: "orders@shop.example".into(),
        topic: None,
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
    // Naming your own account doesn't open another account's thread.
    assert_denied(
        &s,
        &Request::GetPerson {
            account_id: Some(s.own.account.clone()),
            person: "orders@shop.example".into(),
            topic: Some(s.other.thread.clone()),
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
    // A promise from another account under your own thread is still out.
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

/// Requests the old hand-kept list never mentioned, so a scoped agent
/// could reach any account through them.
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
    // Your own account with another account's label lists that label's
    // mail, so the label has to be in the named account.
    let other_inbox = crate::test_fixtures::test_label(&s.other.account, "Inbox", "INBOX");
    s.fx.state.store.upsert_label(&other_inbox).await.unwrap();
    assert_scoped(&s, |o| Request::ListEnvelopes {
        label_id: None,
        account_id: Some(o.account.clone()),
        limit: 10,
        offset: 0,
    })
    .await;
    let foreign_label = Request::ListEnvelopes {
        label_id: Some(other_inbox.id.clone()),
        account_id: Some(s.own.account.clone()),
        limit: 10,
        offset: 0,
    };
    let error = check(&s, &foreign_label).await.unwrap_err();
    assert!(error.contains("is not in account"), "{error}");
    // Claiming your own account on a draft body doesn't let you overwrite
    // another account's stored draft with the same id.
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
        Request::ListMessages {
            account_id: None,
            turn: None,
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
        Request::SetModeGuideSeen {
            mode: "todo".into(),
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

/// The allowlist is applied in dispatch, not only in the helper.
#[tokio::test]
async fn dispatch_denies_another_accounts_records_to_a_scoped_mcp_client() {
    let s = scoped().await;
    let ledger = |account: &mxr_core::AccountId| IpcMessage {
        id: 9,
        source: ::mxr_protocol::ClientKind::Mcp,
        payload: IpcPayload::Request(Request::ListRecords {
            account_id: Some(account.clone()),
            filter: RecordFilterData::default(),
            limit: 10,
            offset: 0,
        }),
    };
    match handle_request(&s.fx.state, &ledger(&s.own.account))
        .await
        .payload
    {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::RecordLedger { ledger },
        }) => assert_eq!(ledger.total, 1),
        other => panic!("expected the agent's own ledger, got {other:?}"),
    }
    match handle_request(&s.fx.state, &ledger(&s.other.account))
        .await
        .payload
    {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(message.contains("account allowlist"), "{message}");
        }
        other => panic!("expected another account's ledger to be denied, got {other:?}"),
    }
}

async fn scoped_dispatch(s: &Scoped, req: Request) -> Response {
    let msg = IpcMessage {
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

/// Legacy Gmail thread ids aren't account-scoped: one id can hold messages
/// from two accounts. The check must see both accounts, not whichever one
/// the thread row happens to report.
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
    // Resolution sees both accounts. A check that took the one account the
    // thread row reports would pass or fail on which row SQLite picked.
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
        },
        Request::GetThreadContext {
            thread_id: shared.clone(),
        },
        Request::GetThreadGists {
            thread_ids: vec![shared.clone()],
            generate: false,
        },
        Request::ExportThread {
            thread_id: shared.clone(),
            format: ExportFormat::Json,
        },
    ] {
        assert_denied(&s, &req).await;
    }

    // Second safety net: a thread load answered to a scoped client keeps
    // only the allowed accounts' messages.
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

/// Status stays allowed as the version handshake, but a scoped client sees
/// only its own accounts' names, sync health and counts.
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

/// Signatures bound to an excluded account are hidden; unbound ones and
/// the agent's own stay. Snippets aren't tied to an account, so a scoped
/// client gets none.
#[tokio::test]
async fn signatures_bound_elsewhere_are_hidden_and_snippets_denied() {
    let s = scoped().await;
    for name in ["theirs", "unbound", "mine"] {
        request(
            &s.fx,
            Request::SetSignature {
                name: name.into(),
                body: format!("-- {name}"),
            },
        )
        .await;
    }
    for (name, account) in [("theirs", &s.other.account), ("mine", &s.own.account)] {
        request(
            &s.fx,
            Request::SetSignatureDefault {
                name: name.into(),
                kind: mxr_protocol::SignatureContextData::New,
                account_id: Some(account.clone()),
                from_email: None,
            },
        )
        .await;
    }
    let Response::Ok {
        data: ResponseData::Signatures { signatures },
    } = scoped_dispatch(&s, Request::ListSignatures).await
    else {
        panic!("expected signatures")
    };
    let mut names: Vec<_> = signatures.iter().map(|sig| sig.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(names, ["mine", "unbound"]);

    for req in [
        Request::ListSnippets,
        Request::SetSnippet {
            name: "x".into(),
            body: "y".into(),
            vars: vec![],
        },
        Request::DeleteSnippet { name: "x".into() },
    ] {
        assert_denied(&s, &req).await;
    }
}
