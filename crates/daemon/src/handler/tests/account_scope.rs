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
    CommitmentDirection, CommitmentStatus, ContactCommitmentRecord, NewCorrection, UndoEntry,
    UndoEntrySnapshot, UndoableMutationKind,
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
    correction: i64,
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

    let correction = store
        .insert_correction(&NewCorrection {
            account_id: account.clone(),
            scope: "email".into(),
            message_id: Some(message.clone()),
            sender_email: "orders@shop.example".into(),
            from_mode: "updates".into(),
            to_mode: "messages".into(),
            rule: None,
            source: "move".into(),
            created_at: Utc::now(),
            prior_moved_to: None,
            prior_moved_at: None,
            prior_disposition: None,
            prior_decided_at: None,
            aspect_id: None,
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
        correction,
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

#[tokio::test]
async fn scoped_profiles_only_see_and_use_categories_visible_in_the_named_account() {
    let s = scoped().await;
    let visible =
        s.fx.state
            .store
            .create_personal_category("Visible")
            .await
            .unwrap();
    let hidden =
        s.fx.state
            .store
            .create_personal_category("Hidden")
            .await
            .unwrap();
    for (message_id, category_id) in [
        (&s.own.message, &visible.id),
        (&s.other.message, &hidden.id),
    ] {
        let before =
            s.fx.state
                .store
                .category_membership_versions(
                    None,
                    std::slice::from_ref(message_id),
                    std::slice::from_ref(category_id),
                )
                .await
                .unwrap()
                .remove(0);
        s.fx.state
            .store
            .apply_category_membership_changes(&[mxr_store::CategoryMembershipChange {
                before,
                assigned: true,
            }])
            .await
            .unwrap();
    }

    let scoped_catalog =
        s.fx.state
            .store
            .list_personal_categories(Some(&s.own.account))
            .await
            .unwrap();
    let profile = s.fx.state.config_snapshot().agent_surfaces.profiles["mcp"].clone();
    let ResponseData::Categories { categories } = super::super::account_scope::scope_response(
        &s.fx.state,
        &profile,
        ResponseData::Categories {
            categories: scoped_catalog,
        },
    )
    .await
    .unwrap() else {
        panic!("expected scoped category catalog")
    };
    assert_eq!(categories.len(), 1);
    assert_eq!(categories[0].name, "Visible");

    assert!(check(
        &s,
        &Request::ListCategories {
            account_id: Some(s.own.account.clone())
        }
    )
    .await
    .is_ok());
    assert!(check(&s, &Request::ListCategories { account_id: None })
        .await
        .unwrap_err()
        .contains("every stored and configured account"));
    assert!(check(
        &s,
        &Request::ListCategoryMessages {
            category_id: hidden.id.clone(),
            account_id: Some(s.own.account.clone()),
            limit: 10,
            offset: 0,
        }
    )
    .await
    .is_err());
    assert!(check(
        &s,
        &Request::GetCategoryMemberships {
            account_id: None,
            message_ids: vec![s.own.message.clone()],
            category_ids: vec![visible.id.clone()],
        }
    )
    .await
    .unwrap_err()
    .contains("must name an account"));
    assert!(check(
        &s,
        &Request::GetCategoryMemberships {
            account_id: Some(s.own.account.clone()),
            message_ids: vec![s.own.message.clone()],
            category_ids: vec![hidden.id.clone()],
        }
    )
    .await
    .is_err());
    assert!(check(
        &s,
        &Request::PreviewCategoryMembership {
            account_id: Some(s.own.account.clone()),
            message_ids: vec![s.own.message.clone()],
            add_category_ids: vec![visible.id.clone()],
            remove_category_ids: vec![],
        }
    )
    .await
    .is_ok());
    assert!(check(
        &s,
        &Request::PreviewCategoryMembership {
            account_id: Some(s.own.account.clone()),
            message_ids: vec![s.own.message.clone()],
            add_category_ids: vec![hidden.id],
            remove_category_ids: vec![],
        }
    )
    .await
    .is_err());
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
    // "Show all" lists matches from the same scope as the answer.
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
async fn arrivals_requests_stay_in_the_agents_accounts() {
    let s = scoped().await;
    assert_scoped(&s, |o| Request::ListArrivals {
        account_id: Some(o.account.clone()),
        bucket: None,
        since: None,
        until: None,
        limit: 100,
    })
    .await;
    assert_scoped(&s, |o| Request::ListCorrections {
        account_id: Some(o.account.clone()),
        limit: 50,
    })
    .await;
    assert_scoped(&s, |o| Request::GetArrivalModes {
        message_ids: vec![o.message.clone()],
    })
    .await;
    assert_scoped(&s, |o| Request::MoveMessage {
        message_id: o.message.clone(),
        mode: ModeKindData::Messages,
        sender: false,
        dry_run: true,
        source: None,
    })
    .await;
    assert_scoped(&s, |o| Request::UndoMove {
        correction_id: o.correction,
    })
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
        Request::GetArrivals {
            account_id: None,
            mark_seen: false,
            since: None,
        },
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

/// The allowlist is applied in dispatch, not only in the helper.
#[tokio::test]
async fn dispatch_denies_another_accounts_records_to_a_scoped_mcp_client() {
    let s = scoped().await;
    let ledger = |account: &mxr_core::AccountId| IpcMessage {
        expected_workspace: None,
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
            automated: false,
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

/// Freshness for every account stays allowed (it is the status bar's
/// poll), but a scoped client sees only its own accounts' arrivals and sync
/// health, never another account's senders or errors. Naming the other
/// account is denied.
#[tokio::test]
async fn freshness_shows_a_scoped_client_only_its_accounts() {
    let s = scoped().await;
    s.fx.state
        .store
        .upsert_sync_runtime_status(
            &s.other.account,
            &mxr_store::SyncRuntimeStatusUpdate {
                last_error: Some(Some("Provider error: oauth token revoked".into())),
                failure_class: Some(Some("auth".into())),
                consecutive_failures: Some(1),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let Response::Ok {
        data: ResponseData::Freshness { freshness },
    } = scoped_dispatch(
        &s,
        Request::GetFreshness {
            account_id: None,
            limit: None,
        },
    )
    .await
    else {
        panic!("expected freshness")
    };
    assert!(freshness
        .accounts
        .iter()
        .all(|account| account.account_id == s.own.account));
    assert!(freshness
        .accounts
        .iter()
        .all(|account| account.last_sync_error.is_none()));
    assert!(!freshness.arrivals.is_empty());
    assert!(freshness
        .arrivals
        .iter()
        .all(|arrival| arrival.account_id == s.own.account));
    assert_ne!(freshness.worst_account_id, Some(s.other.account.clone()));

    assert_scoped(&s, |owned| Request::GetFreshness {
        account_id: Some(owned.account.clone()),
        limit: None,
    })
    .await;
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

/// A thread id holding the agent's message and another account's, whose
/// subject sorts first so the aggregate thread row takes it.
async fn share_thread(s: &Scoped) -> ThreadId {
    let shared = ThreadId::new();
    for (account, tag, subject, sender) in [
        (
            &s.own.account,
            "own",
            "Shared conversation",
            "friend@example.com",
        ),
        (
            &s.other.account,
            "foreign",
            "Aaa their secret",
            "secret@foreign.example",
        ),
    ] {
        let envelope = crate::test_fixtures::TestEnvelopeBuilder::new()
            .account_id(account.clone())
            .thread_id(shared.clone())
            .provider_id(format!("scope-share-{tag}"))
            .subject(subject)
            .sender_address(tag, sender)
            .snippet(format!("snippet {tag}"))
            .date(Utc::now())
            .build();
        s.fx.state
            .store
            .upsert_envelope_with_direction(&envelope, MessageDirection::Inbound)
            .await
            .unwrap();
    }
    shared
}

fn assert_only_own_part(thread: &mxr_core::types::Thread, s: &Scoped) {
    assert_eq!(thread.subject, "Shared conversation");
    assert_eq!(thread.account_id, s.own.account);
    assert_eq!(thread.message_count, 1);
    assert_eq!(thread.message_ids.len(), 1);
    assert_eq!(thread.snippet, "snippet own");
    assert!(
        thread
            .participants
            .iter()
            .all(|participant| participant.email != "secret@foreign.example"),
        "{:?}",
        thread.participants
    );
}

/// The thread list for the agent's own account hydrates every thread from
/// all its messages; a scoped client sees only its own part of a shared one.
#[tokio::test]
async fn list_threads_shows_a_scoped_client_only_its_part_of_a_shared_thread() {
    let s = scoped().await;
    let shared = share_thread(&s).await;
    let Response::Ok {
        data: ResponseData::Threads { threads },
    } = scoped_dispatch(
        &s,
        Request::ListThreads {
            account_id: Some(s.own.account.clone()),
            label_id: None,
            limit: 50,
            offset: 0,
            sort: None,
        },
    )
    .await
    else {
        panic!("expected threads")
    };
    let thread = threads
        .iter()
        .find(|thread| thread.id == shared)
        .expect("the shared thread is listed for its own account");
    assert_only_own_part(thread, &s);
}

/// The subject comes from the kept messages, not the aggregate that may
/// have taken it from a hidden one.
#[tokio::test]
async fn a_scoped_thread_takes_its_subject_from_the_kept_messages() {
    let s = scoped().await;
    let shared = share_thread(&s).await;
    let store = &s.fx.state.store;
    let thread = store.get_thread(&shared).await.unwrap().unwrap();
    assert_eq!(
        thread.subject, "Aaa their secret",
        "fixture: aggregate subject"
    );
    let messages = store.get_thread_envelopes(&shared).await.unwrap();
    let ResponseData::Thread { thread, .. } = super::super::account_scope::scope_response(
        &s.fx.state,
        &scoped_profile(&s),
        ResponseData::Thread {
            thread,
            messages,
            summary: None,
        },
    )
    .await
    .unwrap() else {
        panic!("expected a thread")
    };
    assert_only_own_part(&thread, &s);
}

#[tokio::test]
async fn sorting_rule_scope_checks_previous_account_before_retargeting() {
    let s = scoped().await;
    let mut form = mxr_protocol::RuleFormData {
        id: None,
        account_id: Some(s.own.account.clone()),
        name: "Scoped sorting".into(),
        condition: "from:editor@example.com".into(),
        action: "treatment:reading".into(),
        priority: 10,
        enabled: true,
    };
    assert!(check(
        &s,
        &Request::RuleTreatment {
            form: form.clone(),
            preview_token: None
        }
    )
    .await
    .is_ok());
    form.account_id = Some(s.other.account.clone());
    assert!(check(
        &s,
        &Request::RuleTreatment {
            form: form.clone(),
            preview_token: None
        }
    )
    .await
    .is_err());
    let saved = request(
        &s.fx,
        Request::UpsertRuleForm {
            existing_rule: None,
            account_id: form.account_id.clone(),
            name: form.name.clone(),
            condition: form.condition.clone(),
            action: form.action.clone(),
            priority: form.priority,
            enabled: form.enabled,
        },
    )
    .await;
    let id = match saved {
        ResponseData::RuleData { rule } => rule["id"].as_str().unwrap().to_string(),
        other => panic!("{other:?}"),
    };
    form.id = Some(id.clone());
    form.account_id = Some(s.own.account.clone());
    assert!(check(
        &s,
        &Request::RuleTreatment {
            form,
            preview_token: None
        }
    )
    .await
    .is_err());
    assert!(check(&s, &Request::GetRuleForm { rule: id }).await.is_err());
}

#[tokio::test]
async fn category_rules_global_scope_requires_full_coverage_and_can_be_narrowed() {
    let s = scoped().await;
    let category =
        s.fx.state
            .store
            .create_personal_category("Travel")
            .await
            .unwrap();
    let mut form = mxr_protocol::RuleFormData {
        id: None,
        account_id: None,
        name: "Global category".into(),
        condition: "from:orders@shop.example".into(),
        action: format!("add-category:{}", category.id),
        priority: 10,
        enabled: true,
    };
    assert!(matches!(
        scoped_dispatch(
            &s,
            Request::RuleCategories {
                form: form.clone(),
                preview_token: None,
                delete: false
            }
        )
        .await,
        Response::Error { .. }
    ));
    let mut config = s.fx.state.config_snapshot();
    let profile = config.agent_surfaces.profiles.get_mut("mcp").unwrap();
    profile.allowed_accounts = vec![s.own.account.to_string(), s.other.account.to_string()];
    for (key, account) in &config.accounts {
        profile.allowed_accounts.push(key.clone());
        profile.allowed_accounts.push(account.email.clone());
    }
    s.fx.state.set_config_for_test(config).await;
    let token = match scoped_dispatch(
        &s,
        Request::RuleCategories {
            form: form.clone(),
            preview_token: None,
            delete: false,
        },
    )
    .await
    {
        Response::Ok {
            data: ResponseData::RuleCategoriesResult { preview },
        } => preview.token.unwrap(),
        other => panic!("unexpected {other:?}"),
    };
    let id = match scoped_dispatch(
        &s,
        Request::RuleCategories {
            form: form.clone(),
            preview_token: Some(token),
            delete: false,
        },
    )
    .await
    {
        Response::Ok {
            data: ResponseData::RuleCategoriesResult { preview },
        } => preview.rule_id,
        other => panic!("unexpected {other:?}"),
    };
    assert!(matches!(
        scoped_dispatch(&s, Request::GetRuleForm { rule: id.clone() }).await,
        Response::Ok { .. }
    ));
    form.id = Some(id);
    form.account_id = Some(s.own.account.clone());
    let token = match scoped_dispatch(
        &s,
        Request::RuleCategories {
            form: form.clone(),
            preview_token: None,
            delete: false,
        },
    )
    .await
    {
        Response::Ok {
            data: ResponseData::RuleCategoriesResult { preview },
        } => preview.token.unwrap(),
        other => panic!("unexpected {other:?}"),
    };
    assert!(matches!(
        scoped_dispatch(
            &s,
            Request::RuleCategories {
                form,
                preview_token: Some(token),
                delete: false
            }
        )
        .await,
        Response::Ok { .. }
    ));
    let sources = s.fx.state.store.category_rule_sources().await.unwrap();
    assert!(sources.iter().any(|r| r.message_id == s.own.message));
    assert!(!sources.iter().any(|r| r.message_id == s.other.message));
}

#[tokio::test]
async fn recovery_status_filters_held_subjects_and_retired_evidence_to_the_allowed_account() {
    let s = scoped().await;
    for owned in [&s.own, &s.other] {
        sqlx::query("INSERT INTO restored_draft_holds VALUES(?,?,?,?,NULL)")
            .bind(owned.draft.id.as_str())
            .bind(owned.account.as_str())
            .bind(1)
            .bind(&owned.draft.subject)
            .execute(s.fx.state.store.writer())
            .await
            .unwrap();
        sqlx::query("INSERT INTO restored_intentions VALUES(?,?,?,?,?,?)")
            .bind(owned.draft.id.as_str())
            .bind("scheduled-send")
            .bind(owned.account.as_str())
            .bind("retired")
            .bind("original-operation")
            .bind("{}")
            .execute(s.fx.state.store.writer())
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO restored_intentions VALUES('mixed-archive','mutation-job',NULL,'unknown',NULL,'{}')").execute(s.fx.state.store.writer()).await.unwrap();
    let status = super::super::admin::get_status(&s.fx.state).await.unwrap();
    let status =
        super::super::account_scope::scope_response(&s.fx.state, &scoped_profile(&s), status)
            .await
            .unwrap();
    let ResponseData::Status {
        recovery: Some(recovery),
        ..
    } = status
    else {
        panic!("missing recovery status")
    };
    assert_eq!(recovery.held_drafts.len(), 1);
    assert_eq!(recovery.held_drafts[0].account_id, s.own.account);
    assert_eq!(recovery.retained_intentions.len(), 1);
    assert_eq!(
        recovery.retained_intentions[0].account_id,
        Some(s.own.account.clone())
    );
    assert!(
        check(&s, &Request::GetRecoveryStatus).await.is_err(),
        "full inventory requires all accounts"
    );
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
