#![expect(
    clippy::unwrap_used,
    reason = "tests unwrap fixture setup for direct failures"
)]

use super::*;
use crate::test_fixtures::*;
use chrono::{Duration, TimeZone};
use mxr_core::*;

fn test_envelope(account_id: &AccountId) -> Envelope {
    TestEnvelopeBuilder::new()
        .account_id(account_id.clone())
        .flags(MessageFlags::READ | MessageFlags::STARRED)
        .build()
}

#[tokio::test]
async fn delete_account_cascades_owned_messages() {
    let store = Store::in_memory().await.unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();
    let env = test_envelope(&account.id);
    store.upsert_envelope(&env).await.unwrap();

    let deleted = store.delete_account(&account.id).await.unwrap();

    assert_eq!(deleted, 1);
    assert!(store.get_account(&account.id).await.unwrap().is_none());
    assert!(store.get_envelope(&env.id).await.unwrap().is_none());
    assert!(store
        .list_message_ids_by_account(&account.id)
        .await
        .unwrap()
        .is_empty());
    assert_eq!(
        store.list_pending_message_forgets(10).await.unwrap(),
        vec![env.id],
        "the purged mail's files and index entries are still owed their cleanup"
    );
}

/// The upsert reads before it writes. Opened as a deferred transaction, the
/// read pins a snapshot and the write then fails at once with SQLITE_BUSY
/// (or SQLITE_BUSY_SNAPSHOT once the other writer commits) instead of
/// waiting out `busy_timeout`. Another process writing the same database,
/// such as a test or CLI command next to the daemon, must make it wait.
#[tokio::test]
async fn upsert_envelope_waits_for_another_connections_write() {
    use sqlx::Connection;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.db");
    let store = Store::new(&path).await.unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();

    let mut other = sqlx::SqliteConnection::connect_with(
        &sqlx::sqlite::SqliteConnectOptions::new().filename(&path),
    )
    .await
    .unwrap();
    sqlx::raw_sql("BEGIN IMMEDIATE")
        .execute(&mut other)
        .await
        .unwrap();
    sqlx::query("UPDATE accounts SET name = 'Other writer' WHERE id = ?")
        .bind(account.id.as_str())
        .execute(&mut other)
        .await
        .unwrap();

    let env = test_envelope(&account.id);
    let (upserted, ()) = tokio::join!(store.upsert_envelope(&env), async {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        sqlx::raw_sql("COMMIT").execute(&mut other).await.unwrap();
    });

    assert_eq!(upserted.unwrap(), env.id);
}

async fn commitment_fixture(
    store: &Store,
    account: &Account,
    by_when: Option<DateTime<Utc>>,
    source: Option<ByWhenSource>,
    id: &str,
) -> ContactCommitmentRecord {
    let mut evidence = test_envelope(&account.id);
    evidence.id = MessageId::new();
    evidence.provider_id = format!("evidence-{id}");
    store
        .upsert_envelope_with_direction(&evidence, MessageDirection::Outbound)
        .await
        .unwrap();
    ContactCommitmentRecord {
        id: id.to_string(),
        account_id: account.id.clone(),
        email: "alice@example.com".to_string(),
        thread_id: evidence.thread_id.clone(),
        direction: CommitmentDirection::Yours,
        status: CommitmentStatus::Open,
        who_owes: "me".to_string(),
        what: "Send the deck".to_string(),
        by_when,
        by_when_source: source,
        evidence_msg_id: evidence.id.clone(),
        extracted_at: Utc::now(),
        resolved_at: None,
    }
}

#[tokio::test]
async fn desk_due_commitments_keep_exact_boundaries_provenance_and_active_evidence_rules() {
    let store = Store::in_memory().await.unwrap();
    let account = test_account();
    let other = test_account();
    store.insert_account(&account).await.unwrap();
    store.insert_account(&other).await.unwrap();
    let floor = Utc.with_ymd_and_hms(2026, 10, 1, 8, 0, 0).unwrap();
    let horizon = floor + Duration::days(7);
    for (name, date, source, direction, status) in [
        (
            "floor",
            Some(floor),
            Some(ByWhenSource::Words),
            CommitmentDirection::Yours,
            CommitmentStatus::Open,
        ),
        (
            "horizon",
            Some(horizon),
            Some(ByWhenSource::User),
            CommitmentDirection::Yours,
            CommitmentStatus::Open,
        ),
        (
            "too_old",
            Some(floor - Duration::seconds(1)),
            Some(ByWhenSource::Words),
            CommitmentDirection::Yours,
            CommitmentStatus::Open,
        ),
        (
            "too_new",
            Some(horizon + Duration::seconds(1)),
            Some(ByWhenSource::Words),
            CommitmentDirection::Yours,
            CommitmentStatus::Open,
        ),
        (
            "model",
            Some(floor),
            Some(ByWhenSource::ModelDate),
            CommitmentDirection::Yours,
            CommitmentStatus::Open,
        ),
        (
            "unverified",
            Some(floor),
            None,
            CommitmentDirection::Yours,
            CommitmentStatus::Open,
        ),
        (
            "undated",
            None,
            Some(ByWhenSource::Words),
            CommitmentDirection::Yours,
            CommitmentStatus::Open,
        ),
        (
            "theirs",
            Some(floor),
            Some(ByWhenSource::Words),
            CommitmentDirection::Theirs,
            CommitmentStatus::Open,
        ),
        (
            "resolved",
            Some(floor),
            Some(ByWhenSource::Words),
            CommitmentDirection::Yours,
            CommitmentStatus::Resolved,
        ),
        (
            "expired",
            Some(floor),
            Some(ByWhenSource::Words),
            CommitmentDirection::Yours,
            CommitmentStatus::Expired,
        ),
        (
            "retired_evidence",
            Some(floor),
            Some(ByWhenSource::Words),
            CommitmentDirection::Yours,
            CommitmentStatus::Open,
        ),
    ] {
        let mut record = commitment_fixture(&store, &account, date, source, name).await;
        record.what = name.into();
        record.direction = direction;
        record.status = status;
        if name != "retired_evidence" {
            record.id = format!("{}::promise::{name}", record.evidence_msg_id);
        }
        store.upsert_contact_commitment(&record).await.unwrap();
    }
    let mut foreign = commitment_fixture(
        &store,
        &other,
        Some(floor),
        Some(ByWhenSource::User),
        "foreign",
    )
    .await;
    foreign.what = "foreign".into();
    store.upsert_contact_commitment(&foreign).await.unwrap();
    let mut due = store
        .desk_due_commitments(&account.id, floor, horizon)
        .await
        .unwrap();
    due.sort_by(|a, b| a.what.cmp(&b.what));
    assert_eq!(
        due.iter()
            .map(|record| record.what.as_str())
            .collect::<Vec<_>>(),
        ["floor", "horizon"]
    );
    assert!(store
        .desk_due_commitments(&account.id, horizon, floor)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn envelope_upsert_preserves_existing_provider_identity_when_generated_id_changes() {
    let store = Store::in_memory().await.unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();

    let legacy_id = MessageId::from_provider_id("gmail", "msg-001");
    let mut legacy_envelope = test_envelope(&account.id);
    legacy_envelope.id = legacy_id.clone();
    legacy_envelope.provider_id = "msg-001".to_string();
    store.upsert_envelope(&legacy_envelope).await.unwrap();

    let label = Label {
        id: LabelId::from_provider_id("gmail", "INBOX"),
        account_id: account.id.clone(),
        name: "Inbox".to_string(),
        kind: LabelKind::System,
        color: None,
        provider_id: "INBOX".to_string(),
        unread_count: 1,
        total_count: 1,
        role: None,
    };
    store.upsert_label(&label).await.unwrap();
    store
        .set_message_labels(
            &legacy_id,
            std::slice::from_ref(&label.id),
            EventSource::Sync,
        )
        .await
        .unwrap();

    let mut regenerated = test_envelope(&account.id);
    regenerated.id = MessageId::from_scoped_provider_id(&account.id, "gmail", "msg-001");
    regenerated.provider_id = "msg-001".to_string();
    regenerated.subject = "Updated subject".to_string();
    assert_eq!(
        store.upsert_envelope(&regenerated).await.unwrap(),
        legacy_id
    );

    let list = store
        .list_envelopes_by_account(&account.id, 100, 0)
        .await
        .unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, legacy_id);
    assert_eq!(list[0].subject, "Updated subject");

    let by_label = store
        .list_envelopes_by_label(&label.id, 100, 0)
        .await
        .unwrap();
    assert_eq!(by_label.len(), 1);
    assert_eq!(by_label[0].id, legacy_id);
}

#[tokio::test]
async fn get_thread_populates_message_ids_date_ordered() {
    let store = Store::in_memory().await.unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();

    let thread_id = ThreadId::new();
    let base = chrono::Utc::now();
    let mut envs: Vec<Envelope> = (0..3)
        .map(|i| {
            let mut env = test_envelope(&account.id);
            env.provider_id = format!("phase-f-{i}");
            env.thread_id = thread_id.clone();
            env.date = base - chrono::Duration::hours(i);
            if i == 0 {
                env.flags = MessageFlags::empty();
            }
            env
        })
        .collect();
    for env in &envs {
        store.upsert_envelope(env).await.unwrap();
    }

    let thread = store.get_thread(&thread_id).await.unwrap().unwrap();
    assert_eq!(thread.message_count, 3);
    assert_eq!(thread.unread_count, 1);
    assert_eq!(
        thread.message_ids.len(),
        3,
        "expected three members, got {:?}",
        thread.message_ids
    );
    envs.sort_by_key(|e| e.date);
    assert_eq!(
        thread.message_ids,
        envs.iter().map(|e| e.id.clone()).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn update_draft_edits_in_place_and_preserves_created_at() {
    let store = Store::in_memory().await.unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();

    let created = chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap();
    let draft = Draft {
        revision: Some(1),
        id: DraftId::new(),
        account_id: account.id.clone(),
        from: None,
        reply_headers: None,
        intent: DraftIntent::New,
        to: vec![Address {
            name: None,
            email: "bob@example.com".to_string(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Original".to_string(),
        content: DraftContent::markdown("first"),
        managed_attachments: None,
        attachments: vec![],
        inline_assets: vec![],
        inline_calendar_reply: None,
        created_at: created,
        updated_at: created,
    };
    store.insert_draft(&draft).await.unwrap();

    let later = chrono::DateTime::from_timestamp(1_700_000_500, 0).unwrap();
    let edited = Draft {
        subject: "Edited".to_string(),
        content: DraftContent::markdown("second"),
        updated_at: later,
        ..draft.clone()
    };
    let updated = store.update_draft(&edited).await.unwrap();
    assert!(updated, "an editable draft should report a row updated");

    let drafts = store.list_drafts(&account.id).await.unwrap();
    assert_eq!(drafts.len(), 1);
    let stored = store.get_draft(&draft.id).await.unwrap().unwrap();
    assert_eq!(stored.subject, "Edited");
    assert_eq!(stored.content.analysis_text(), "second");
    assert_eq!(
        stored.created_at, created,
        "created_at must survive the edit"
    );
    assert_eq!(stored.updated_at, later);

    let ghost = Draft {
        revision: Some(1),
        id: DraftId::new(),
        ..edited
    };
    assert!(!store.update_draft(&ghost).await.unwrap());
}

fn draft_with(account_id: &AccountId, content: DraftContent, at: i64) -> Draft {
    Draft {
        revision: Some(1),
        id: DraftId::new(),
        account_id: account_id.clone(),
        from: None,
        reply_headers: None,
        intent: DraftIntent::New,
        to: vec![Address {
            name: None,
            email: "bob@example.com".to_string(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Subject".to_string(),
        content,
        managed_attachments: None,
        attachments: vec![],
        inline_assets: vec![],
        inline_calendar_reply: None,
        created_at: chrono::DateTime::from_timestamp(at, 0).unwrap(),
        updated_at: chrono::DateTime::from_timestamp(at, 0).unwrap(),
    }
}

#[tokio::test]
async fn html_draft_survives_a_round_trip_byte_for_byte() {
    let store = Store::in_memory().await.unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();

    let html = r#"<div class="x"><p>Hi&nbsp;— <img src="cid:logo"></p></div>"#;
    let mut draft = draft_with(
        &account.id,
        DraftContent::html(html, Some("Hi —".to_string())),
        1_700_000_000,
    );
    draft.inline_assets = vec![InlineAsset {
        cid: "logo".to_string(),
        path: std::path::PathBuf::from("/tmp/logo.png"),
    }];
    store.insert_draft(&draft).await.unwrap();

    let fetched = store.get_draft(&draft.id).await.unwrap().unwrap();
    assert_eq!(
        fetched.content,
        DraftContent::html(html, Some("Hi —".to_string()))
    );
    assert_eq!(fetched.inline_assets, draft.inline_assets);
}

#[tokio::test]
async fn a_pre_049_draft_row_still_decodes_as_markdown() {
    let store = Store::in_memory().await.unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();

    let id = DraftId::new();
    sqlx::query(
        "INSERT INTO drafts (id, account_id, subject, body_markdown, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(id.as_str())
    .bind(account.id.as_str())
    .bind("Legacy subject")
    .bind("# Written before 049")
    .bind(1_600_000_000_i64)
    .bind(1_600_000_000_i64)
    .execute(store.writer())
    .await
    .unwrap();

    let kind: String = sqlx::query_scalar("SELECT content_kind FROM drafts WHERE id = ?")
        .bind(id.as_str())
        .fetch_one(store.reader())
        .await
        .unwrap();
    assert_eq!(kind, "markdown");

    let fetched = store.get_draft(&id).await.unwrap().unwrap();
    assert_eq!(
        fetched.content,
        DraftContent::markdown("# Written before 049")
    );
    assert_eq!(fetched.subject, "Legacy subject");
    assert!(fetched.inline_assets.is_empty());
    let listed = store.list_drafts(&account.id).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(
        listed[0].content,
        DraftContent::markdown("# Written before 049")
    );
}

#[tokio::test]
async fn an_interrupted_migration_049_finishes_on_the_next_open() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("mxr.db");

    let store = Store::new(&db_path).await.unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();

    sqlx::query("ALTER TABLE drafts DROP COLUMN body_text")
        .execute(store.writer())
        .await
        .unwrap();
    sqlx::query("DELETE FROM schema_migrations WHERE version = 49")
        .execute(store.writer())
        .await
        .unwrap();
    drop(store);

    let store = Store::new(&db_path).await.unwrap();
    let draft = draft_with(
        &account.id,
        DraftContent::html("<p>resumed</p>", Some("resumed".to_string())),
        1_700_000_000,
    );
    store.insert_draft(&draft).await.unwrap();

    let fetched = store.get_draft(&draft.id).await.unwrap().unwrap();
    assert_eq!(
        fetched.content,
        DraftContent::html("<p>resumed</p>", Some("resumed".to_string()))
    );
}

#[tokio::test]
async fn event_log_insert_and_query() {
    let store = Store::in_memory().await.unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();
    let env = test_envelope(&account.id);
    let env_id = env.id.as_str();
    store.upsert_envelope(&env).await.unwrap();

    store
        .insert_event("info", "sync", "Sync completed", Some(&account.id), None)
        .await
        .unwrap();
    store
        .insert_event(
            "error",
            "sync",
            "Sync failed",
            Some(&account.id),
            Some("timeout"),
        )
        .await
        .unwrap();
    store
        .insert_event("info", "rule", "Rule applied", None, None)
        .await
        .unwrap();
    store
        .insert_event_refs(
            "info",
            "mutation",
            "Archived a message",
            EventLogRefs {
                account_id: Some(&account.id),
                message_id: Some(env_id.as_str()),
                rule_id: None,
            },
            Some("from=test@example.com"),
        )
        .await
        .unwrap();

    let all = store.list_events(10, None, None).await.unwrap();
    assert_eq!(all.len(), 4);

    let errors = store.list_events(10, Some("error"), None).await.unwrap();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].summary, "Sync failed");

    let sync_events = store.list_events(10, None, Some("sync")).await.unwrap();
    assert_eq!(sync_events.len(), 2);

    let mutation_events = store.list_events(10, None, Some("mutation")).await.unwrap();
    assert_eq!(mutation_events.len(), 1);
    assert_eq!(
        mutation_events[0].message_id.as_deref(),
        Some(env_id.as_str())
    );

    let latest_sync = store
        .latest_event_timestamp("sync", Some("Sync"))
        .await
        .unwrap();
    assert!(latest_sync.is_some());
}

#[tokio::test]
async fn label_mutations_do_not_leak_between_accounts() {
    let store = Store::in_memory().await.unwrap();
    let account_a = test_account();
    store.insert_account(&account_a).await.unwrap();
    let mut account_b = test_account();
    account_b.id = AccountId::new();
    account_b.email = "b@example.com".to_string();
    account_b.name = "B".to_string();
    store.insert_account(&account_b).await.unwrap();

    let label_a = Label {
        id: LabelId::from_provider_id("imap", &format!("A-{}", account_a.id.as_str())),
        account_id: account_a.id.clone(),
        name: "WorkA".to_string(),
        kind: LabelKind::User,
        color: None,
        provider_id: "WorkA".to_string(),
        unread_count: 0,
        total_count: 0,
        role: None,
    };
    let label_b = Label {
        id: LabelId::from_provider_id("imap", &format!("B-{}", account_b.id.as_str())),
        account_id: account_b.id.clone(),
        name: "WorkB".to_string(),
        kind: LabelKind::User,
        color: None,
        provider_id: "WorkB".to_string(),
        unread_count: 0,
        total_count: 0,
        role: None,
    };
    store.upsert_label(&label_a).await.unwrap();
    store.upsert_label(&label_b).await.unwrap();

    let env_a = test_envelope(&account_a.id);
    store.upsert_envelope(&env_a).await.unwrap();
    let mut env_b = test_envelope(&account_b.id);
    env_b.id = mxr_core::MessageId::from_scoped_provider_id(&account_b.id, "imap", "msg-b");
    env_b.provider_id = "msg-b".to_string();
    store.upsert_envelope(&env_b).await.unwrap();

    store
        .set_message_labels(
            &env_a.id,
            std::slice::from_ref(&label_a.id),
            EventSource::User,
        )
        .await
        .unwrap();
    store
        .set_message_labels(
            &env_b.id,
            std::slice::from_ref(&label_b.id),
            EventSource::User,
        )
        .await
        .unwrap();

    let by_a = store
        .list_envelopes_by_label(&label_a.id, 100, 0)
        .await
        .unwrap();
    assert_eq!(by_a.len(), 1);
    assert_eq!(by_a[0].account_id, account_a.id);

    let by_b = store
        .list_envelopes_by_label(&label_b.id, 100, 0)
        .await
        .unwrap();
    assert_eq!(by_b.len(), 1);
    assert_eq!(by_b[0].account_id, account_b.id);

    store
        .set_message_labels(&env_a.id, &[], EventSource::User)
        .await
        .unwrap();
    let by_a_after = store
        .list_envelopes_by_label(&label_a.id, 100, 0)
        .await
        .unwrap();
    assert!(by_a_after.is_empty());
    let by_b_after = store
        .list_envelopes_by_label(&label_b.id, 100, 0)
        .await
        .unwrap();
    assert_eq!(by_b_after.len(), 1, "account B unaffected");
}

#[tokio::test]
async fn set_message_labels_is_atomic_under_constraint_violation() {
    let store = Store::in_memory().await.unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();

    let label_a = Label {
        id: LabelId::from_provider_id("imap", "A"),
        account_id: account.id.clone(),
        name: "A".to_string(),
        kind: LabelKind::User,
        color: None,
        provider_id: "A".to_string(),
        unread_count: 0,
        total_count: 0,
        role: None,
    };
    let label_b = Label {
        id: LabelId::from_provider_id("imap", "B"),
        account_id: account.id.clone(),
        name: "B".to_string(),
        kind: LabelKind::User,
        color: None,
        provider_id: "B".to_string(),
        unread_count: 0,
        total_count: 0,
        role: None,
    };
    store.upsert_label(&label_a).await.unwrap();
    store.upsert_label(&label_b).await.unwrap();

    let env = test_envelope(&account.id);
    store.upsert_envelope(&env).await.unwrap();
    store
        .set_message_labels(
            &env.id,
            &[label_a.id.clone(), label_b.id.clone()],
            EventSource::User,
        )
        .await
        .unwrap();

    let result = store
        .set_message_labels(
            &env.id,
            &[label_a.id.clone(), label_a.id.clone()],
            EventSource::User,
        )
        .await;
    assert!(
        result.is_err(),
        "set_message_labels with duplicate label_ids should fail (got {result:?})"
    );

    let by_a = store
        .list_envelopes_by_label(&label_a.id, 100, 0)
        .await
        .unwrap();
    assert_eq!(by_a.len(), 1, "label A should still be associated");
    let by_b = store
        .list_envelopes_by_label(&label_b.id, 100, 0)
        .await
        .unwrap();
    assert_eq!(by_b.len(), 1, "label B must survive the failed mutation");
}

#[tokio::test]
async fn mutations_emit_typed_message_events_with_source() {
    let store = Store::in_memory().await.unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();

    let env = TestEnvelopeBuilder::new()
        .account_id(account.id.clone())
        .build();
    store.upsert_envelope(&env).await.unwrap();

    let label = mxr_core::Label {
        id: LabelId::new(),
        account_id: account.id.clone(),
        name: "Important".to_string(),
        kind: LabelKind::User,
        color: None,
        provider_id: "provider-Important".to_string(),
        unread_count: 0,
        total_count: 0,
        role: None,
    };
    store.upsert_label(&label).await.unwrap();

    store
        .set_starred(&env.id, true, EventSource::RuleEngine)
        .await
        .unwrap();
    store
        .move_to_trash(&env.id, EventSource::User)
        .await
        .unwrap();
    store
        .add_message_label(&env.id, &label.id, EventSource::Sync)
        .await
        .unwrap();
    store
        .remove_message_label(&env.id, &label.id, EventSource::User)
        .await
        .unwrap();

    let events = store.list_message_events(&env.id).await.unwrap();
    let types: Vec<MessageEventType> = events.iter().map(|e| e.event_type).collect();
    let sources: Vec<EventSource> = events.iter().map(|e| e.source).collect();

    assert_eq!(
        types,
        vec![
            MessageEventType::Starred,
            MessageEventType::Trashed,
            MessageEventType::Labeled,
            MessageEventType::Unlabeled,
        ],
    );
    assert_eq!(
        sources,
        vec![
            EventSource::RuleEngine,
            EventSource::User,
            EventSource::Sync,
            EventSource::User,
        ],
    );
    assert_eq!(events[0].label_id, None);
    assert_eq!(events[1].label_id, None);
    assert_eq!(events[2].label_id, Some(label.id.clone()));
    assert_eq!(events[3].label_id, Some(label.id));
}

#[tokio::test]
async fn list_response_time_returns_clock_and_business_percentiles() {
    let store = Store::in_memory().await.unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();

    let monday: chrono::DateTime<chrono::Utc> =
        chrono::Utc.with_ymd_and_hms(2026, 5, 4, 10, 0, 0).unwrap();
    for (i, latency_h) in [1i64, 2, 3].iter().enumerate() {
        let mut parent = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        parent.id = MessageId::new();
        parent.provider_id = format!("parent-{i}");
        parent.message_id_header = Some(format!("<parent-{i}@x>"));
        parent.from = Address {
            name: None,
            email: format!("alice-{i}@example.com"),
        };
        parent.date = monday;
        store
            .upsert_envelope_with_direction(&parent, MessageDirection::Inbound)
            .await
            .unwrap();

        let mut reply = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        reply.id = MessageId::new();
        reply.provider_id = format!("reply-{i}");
        reply.from = Address {
            name: None,
            email: "me@example.com".into(),
        };
        reply.to = vec![Address {
            name: None,
            email: format!("alice-{i}@example.com"),
        }];
        reply.in_reply_to = Some(format!("<parent-{i}@x>"));
        reply.date = monday + chrono::Duration::hours(*latency_h);
        store
            .upsert_envelope_with_direction(&reply, MessageDirection::Outbound)
            .await
            .unwrap();
        assert!(store.try_create_reply_pair(&reply).await.unwrap());
    }
    let backfilled = store.backfill_business_hours_latency().await.unwrap();
    assert_eq!(backfilled, 0);

    let summary = store
        .list_response_time(None, ResponseTimeDirection::IReplied, None, None)
        .await
        .unwrap();
    assert_eq!(summary.sample_count, 3);
    assert_eq!(summary.clock_p50_seconds, 7200);
    assert_eq!(summary.clock_p90_seconds, 10_800);
    assert_eq!(summary.business_hours_p50_seconds, Some(7200));
    assert_eq!(summary.business_hours_p90_seconds, Some(10_800));
}

#[tokio::test]
async fn refresh_contacts_aggregates_inbound_and_outbound_per_email() {
    let store = Store::in_memory().await.unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();
    let now = chrono::Utc::now();

    async fn seed(
        store: &Store,
        account_id: &AccountId,
        sender: &str,
        direction: MessageDirection,
        count: usize,
        now: chrono::DateTime<chrono::Utc>,
    ) {
        for i in 0..count {
            let mut env = TestEnvelopeBuilder::new()
                .account_id(account_id.clone())
                .build();
            env.id = MessageId::new();
            let dir_label = match direction {
                MessageDirection::Inbound => "in",
                MessageDirection::Outbound => "out",
                MessageDirection::Unknown => "unk",
            };
            env.provider_id = format!("p-{sender}-{dir_label}-{i}");
            env.from = match direction {
                MessageDirection::Inbound => Address {
                    name: Some(sender.into()),
                    email: format!("{sender}@example.com"),
                },
                _ => Address {
                    name: None,
                    email: "me@example.com".into(),
                },
            };
            env.to = match direction {
                MessageDirection::Inbound => vec![Address {
                    name: None,
                    email: "me@example.com".into(),
                }],
                _ => vec![Address {
                    name: None,
                    email: format!("{sender}@example.com"),
                }],
            };
            env.date = now - chrono::Duration::days(i as i64);
            store
                .upsert_envelope_with_direction(&env, direction)
                .await
                .unwrap();
        }
    }

    seed(
        &store,
        &account.id,
        "alice",
        MessageDirection::Inbound,
        10,
        now,
    )
    .await;
    seed(
        &store,
        &account.id,
        "alice",
        MessageDirection::Outbound,
        1,
        now,
    )
    .await;
    seed(
        &store,
        &account.id,
        "bob",
        MessageDirection::Inbound,
        5,
        now,
    )
    .await;
    seed(
        &store,
        &account.id,
        "bob",
        MessageDirection::Outbound,
        5,
        now,
    )
    .await;
    seed(
        &store,
        &account.id,
        "carol",
        MessageDirection::Outbound,
        8,
        now,
    )
    .await;

    store.refresh_contacts().await.unwrap();

    let asym = store.list_contact_asymmetry(None, 1, 50).await.unwrap();
    let by_email: std::collections::HashMap<_, _> =
        asym.iter().map(|r| (r.email.clone(), r)).collect();
    let alice = by_email.get("alice@example.com").unwrap();
    assert_eq!(alice.total_inbound, 10);
    assert_eq!(alice.total_outbound, 1);
    assert!((alice.asymmetry - 0.9).abs() < 1e-6);

    let bob = by_email.get("bob@example.com").unwrap();
    assert_eq!(bob.total_inbound, 5);
    assert_eq!(bob.total_outbound, 5);
    assert!(bob.asymmetry.abs() < 1e-6);

    assert!(!by_email.contains_key("carol@example.com"));

    assert_eq!(asym[0].email, "alice@example.com");
}

#[tokio::test]
async fn try_create_reply_pair_links_outbound_reply_to_inbound_parent() {
    let store = Store::in_memory().await.unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();

    let inbound_at: chrono::DateTime<chrono::Utc> =
        chrono::Utc.with_ymd_and_hms(2026, 5, 1, 9, 0, 0).unwrap();
    let outbound_at = inbound_at + chrono::Duration::seconds(3600);

    let mut inbound = TestEnvelopeBuilder::new()
        .account_id(account.id.clone())
        .build();
    inbound.from = Address {
        name: Some("Alice".into()),
        email: "alice@example.com".into(),
    };
    inbound.message_id_header = Some("<parent@example.com>".to_string());
    inbound.date = inbound_at;
    store
        .upsert_envelope_with_direction(&inbound, MessageDirection::Inbound)
        .await
        .unwrap();

    let mut outbound = TestEnvelopeBuilder::new()
        .account_id(account.id.clone())
        .build();
    outbound.id = MessageId::new();
    outbound.provider_id = "outbound-1".into();
    outbound.from = Address {
        name: None,
        email: "me@example.com".into(),
    };
    outbound.to = vec![Address {
        name: None,
        email: "alice@example.com".into(),
    }];
    outbound.in_reply_to = Some("<parent@example.com>".to_string());
    outbound.date = outbound_at;
    store
        .upsert_envelope_with_direction(&outbound, MessageDirection::Outbound)
        .await
        .unwrap();

    let created = store.try_create_reply_pair(&outbound).await.unwrap();
    assert!(created);

    let row: (String, String, i64, String) = sqlx::query_as(
        "SELECT direction, counterparty_email, latency_seconds, parent_message_id
             FROM reply_pairs WHERE reply_message_id = ?",
    )
    .bind(outbound.id.as_str())
    .fetch_one(store.reader())
    .await
    .unwrap();
    assert_eq!(row.0, "i_replied");
    assert_eq!(row.1, "alice@example.com");
    assert_eq!(row.2, 3600);
    assert_eq!(row.3, inbound.id.as_str());
}

#[tokio::test]
async fn list_subscriptions_computes_open_and_archive_counts_per_sender() {
    let store = Store::in_memory().await.unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();

    let mut next_id = 0u32;
    let mut make_msg = |sender: &str, flags: MessageFlags| {
        next_id += 1;
        let mut env = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .flags(flags)
            .build();
        env.from = Address {
            name: Some(sender.into()),
            email: format!("{sender}@list.example.com"),
        };
        env.provider_id = format!("msg-{next_id}");
        env.unsubscribe = UnsubscribeMethod::HttpLink {
            url: "https://example.com/unsub".into(),
        };
        env
    };

    for i in 0..3 {
        let flags = if i == 0 {
            MessageFlags::READ
        } else {
            MessageFlags::empty()
        };
        store
            .upsert_envelope(&make_msg("alice", flags))
            .await
            .unwrap();
    }
    for _ in 0..5 {
        store
            .upsert_envelope(&make_msg("bob", MessageFlags::READ))
            .await
            .unwrap();
    }
    for i in 0..4 {
        let flags = if i < 3 {
            MessageFlags::ARCHIVED
        } else {
            MessageFlags::empty()
        };
        store
            .upsert_envelope(&make_msg("carol", flags))
            .await
            .unwrap();
    }

    let mut subs = store.list_subscriptions(None, 100).await.unwrap();
    subs.sort_by(|a, b| a.sender_email.cmp(&b.sender_email));
    assert_eq!(subs.len(), 3);

    let by_email = |email: &str| {
        subs.iter()
            .find(|s| s.sender_email == email)
            .unwrap_or_else(|| panic!("missing sender {email}"))
    };

    let alice = by_email("alice@list.example.com");
    assert_eq!(alice.message_count, 3);
    assert_eq!(alice.opened_count, 1);
    assert_eq!(alice.archived_unread_count, 0);
    assert_eq!(alice.replied_count, 0);

    let bob = by_email("bob@list.example.com");
    assert_eq!(bob.message_count, 5);
    assert_eq!(bob.opened_count, 5);
    assert_eq!(bob.archived_unread_count, 0);

    let carol = by_email("carol@list.example.com");
    assert_eq!(carol.message_count, 4);
    assert_eq!(carol.opened_count, 0);
    assert_eq!(carol.archived_unread_count, 3);
}

/// One undo id is claimed once: two overlapping `take_undo_entry` calls can't
/// both get the entry, so a concurrent double-undo can't apply twice. A
/// different id is untouched.
#[tokio::test]
async fn take_undo_entry_claims_an_id_once() {
    use crate::undo::{UndoEntry, UndoEntrySnapshot, UndoableMutationKind};
    let store = Store::in_memory().await.unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();

    let entry = UndoEntry {
        mutation_id: "mut-claim".into(),
        kind: UndoableMutationKind::Archive,
        snapshots: vec![UndoEntrySnapshot {
            message_id: MessageId::new(),
            account_id: account.id.clone(),
            provider_id: "fake-msg-claim".into(),
            prior_flags_bits: MessageFlags::READ.bits(),
            prior_label_provider_ids: vec!["INBOX".into()],
            uncertain: false,
        }],
        desk: None,
        applied_at: 1_700_000_000,
        expires_at: 1_700_000_060,
    };
    store.write_undo_entry(&entry).await.unwrap();

    let (first, second) = tokio::join!(
        store.take_undo_entry("mut-claim"),
        store.take_undo_entry("mut-claim"),
    );
    let claims = [first.unwrap(), second.unwrap()];
    assert_eq!(
        claims.iter().filter(|claim| claim.is_some()).count(),
        1,
        "exactly one caller wins the claim"
    );
    let claimed = claims
        .into_iter()
        .flatten()
        .next()
        .expect("the winner has the entry");
    assert_eq!(claimed.mutation_id, "mut-claim");
    assert_eq!(claimed.snapshots.len(), 1);
    assert!(
        store.read_undo_entry("mut-claim").await.unwrap().is_none(),
        "the claim consumes the entry"
    );

    store
        .write_undo_entry(&UndoEntry {
            mutation_id: "mut-other".into(),
            ..entry
        })
        .await
        .unwrap();
    assert!(
        store.take_undo_entry("mut-other").await.unwrap().is_some(),
        "a different id is not affected"
    );
}

/// A job id is taken by one insert: of any number of callers with the same id,
/// one wins and the row is the winner's.
#[tokio::test]
async fn claim_mutation_job_is_won_by_exactly_one_caller() {
    let store = Store::in_memory().await.unwrap();
    let (a, b, c) = tokio::join!(
        store.claim_mutation_job("job", "mutation.archive", "queued", 1, r#"{"by":"a"}"#),
        store.claim_mutation_job("job", "mutation.archive", "queued", 2, r#"{"by":"b"}"#),
        store.claim_mutation_job("job", "mutation.archive", "queued", 3, r#"{"by":"c"}"#),
    );
    let results = [a.unwrap(), b.unwrap(), c.unwrap()];
    assert_eq!(results.iter().filter(|won| **won).count(), 1);
    let winner = ["a", "b", "c"][results.iter().position(|won| *won).unwrap()];
    assert_eq!(
        store.get_mutation_job("job").await.unwrap().unwrap(),
        format!(r#"{{"by":"{winner}"}}"#)
    );
}

/// Phase D: dedup log starts empty, was_mutation_applied returns false
/// for an unseen key, true after record. Same key recorded twice is a
/// no-op (single row).
#[tokio::test]
async fn mutation_dedup_records_apply_and_detects_replay() {
    let store = Store::in_memory().await.unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();

    assert!(
        !store
            .was_mutation_applied("mut-1", &account.id, "msg-1")
            .await
            .unwrap(),
        "unrecorded key must report not-applied"
    );

    store
        .record_mutation_applied("mut-1", "msg-1", &account.id, 100)
        .await
        .unwrap();

    assert!(
        store
            .was_mutation_applied("mut-1", &account.id, "msg-1")
            .await
            .unwrap(),
        "recorded key must report applied"
    );

    store
        .record_mutation_applied("mut-1", "msg-1", &account.id, 200)
        .await
        .unwrap();
    assert_eq!(store.count_mutation_dedup_rows().await.unwrap(), 1);

    store
        .record_mutation_applied("mut-1", "msg-2", &account.id, 100)
        .await
        .unwrap();
    assert!(store
        .was_mutation_applied("mut-1", &account.id, "msg-2")
        .await
        .unwrap());
    assert!(
        !store
            .was_mutation_applied("mut-other", &account.id, "msg-1")
            .await
            .unwrap(),
        "different mutation_id must not collide"
    );
}

#[tokio::test]
async fn draft_revision_survives_restart_and_sql_cas_accepts_only_one_editor() {
    let temp = tempfile::tempdir().unwrap();
    let database = temp.path().join("drafts.db");
    let account = test_account();
    let original = draft_with(
        &account.id,
        DraftContent::markdown("durable original"),
        1_700_000_000,
    );
    {
        let store = Store::new(&database).await.unwrap();
        store.insert_account(&account).await.unwrap();
        store.insert_draft(&original).await.unwrap();
    }
    let store = Store::new(&database).await.unwrap();
    let stored = store.get_draft(&original.id).await.unwrap().unwrap();
    assert_eq!(stored.revision, Some(1));
    assert_eq!(stored.content.analysis_text(), "durable original");
    let mut first = stored.clone();
    first.content = DraftContent::markdown("first editor");
    let mut second = stored.clone();
    second.content = DraftContent::markdown("second editor");
    let (a, b) = tokio::join!(store.update_draft(&first), store.update_draft(&second));
    assert_ne!(a.unwrap(), b.unwrap());
    let current = store.get_draft(&original.id).await.unwrap().unwrap();
    assert_eq!(current.revision, Some(2));
    let mut legacy = current.clone();
    legacy.revision = None;
    assert!(!store.update_draft(&legacy).await.unwrap());
    assert!(!store
        .delete_draft_at_revision(&current.id, 1)
        .await
        .unwrap());
    assert!(store.get_draft(&current.id).await.unwrap().is_some());
}

#[tokio::test]
async fn reindex_keyset_pages_visit_every_row_once_across_ties_and_mid_scan_inserts() {
    let store = Store::in_memory().await.unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();

    let shared_date = chrono::Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
    let mut originals = Vec::new();
    for i in 0..10 {
        let mut env = test_envelope(&account.id);
        env.provider_id = format!("keyset-{i}");
        env.date = shared_date;
        store.upsert_envelope(&env).await.unwrap();
        originals.push(env.id);
    }

    let mut seen = Vec::new();
    let first = store.list_envelopes_for_reindex(None, 3).await.unwrap();
    assert_eq!(first.len(), 3);
    seen.extend(first.iter().map(|env| env.id.clone()));
    let cursor = first.last().unwrap().id.clone();

    let mut above = test_envelope(&account.id);
    above.provider_id = "above".to_string();
    above.id = MessageId::from_uuid(uuid::Uuid::from_u128(u128::MAX));
    store.upsert_envelope(&above).await.unwrap();
    let mut below = test_envelope(&account.id);
    below.provider_id = "below".to_string();
    below.id = MessageId::from_uuid(uuid::Uuid::from_u128(0));
    store.upsert_envelope(&below).await.unwrap();
    assert!(above.id.as_uuid() > cursor.as_uuid() && below.id.as_uuid() < cursor.as_uuid());

    let mut after = Some(cursor);
    loop {
        let page = store
            .list_envelopes_for_reindex(after.as_ref(), 3)
            .await
            .unwrap();
        let Some(last) = page.last() else { break };
        after = Some(last.id.clone());
        seen.extend(page.iter().map(|env| env.id.clone()));
    }

    let unique: std::collections::HashSet<_> = seen.iter().collect();
    assert_eq!(unique.len(), seen.len(), "a row appeared on two pages");
    for id in &originals {
        assert_eq!(seen.iter().filter(|seen_id| *seen_id == id).count(), 1);
    }
    assert_eq!(seen.iter().filter(|id| **id == above.id).count(), 1);
    assert!(!seen.contains(&below.id));
    assert_eq!(seen.len(), originals.len() + 1);
}

/// Regression for issue #107: the per-message dedup/existence lookup
/// (`WHERE account_id = ? AND message_id_header = ?`) must resolve through the
/// composite index `idx_messages_account_msgidhdr` instead of scanning every
/// row of the account. Migration 047 adds that index.
#[tokio::test]
async fn dedup_lookup_uses_message_id_header_index() {
    let store = Store::in_memory().await.unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();

    let mut env = TestEnvelopeBuilder::new()
        .account_id(account.id.clone())
        .build();
    env.message_id_header = Some("<dedup@example.com>".to_string());
    store.upsert_envelope(&env).await.unwrap();

    let hits = store
        .list_envelopes_by_message_id_header(&account.id, "<dedup@example.com>")
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);

    let plan: Vec<(i64, i64, i64, String)> = sqlx::query_as(
        "EXPLAIN QUERY PLAN \
         SELECT id FROM messages \
         WHERE account_id = ? AND message_id_header = ? LIMIT 1",
    )
    .bind(account.id.as_str())
    .bind("<dedup@example.com>")
    .fetch_all(store.reader())
    .await
    .unwrap();

    let detail = plan
        .iter()
        .map(|(_, _, _, d)| d.as_str())
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(
        detail.contains("idx_messages_account_msgidhdr"),
        "dedup lookup should use idx_messages_account_msgidhdr, got plan: {detail}"
    );
    assert!(
        !detail.contains("SCAN messages"),
        "dedup lookup should not full-scan messages, got plan: {detail}"
    );
}
