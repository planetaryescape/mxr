use super::*;
use crate::test_fixtures::{test_account, TestEnvelopeBuilder};
use crate::Store;
use chrono::{Duration, TimeZone};
use mxr_core::types::{Address, MessageDirection};

fn at(hours: i64) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 1, 9, 0, 0)
        .single()
        .expect("valid time")
        + Duration::hours(hours)
}

struct Fx {
    store: Store,
    account: AccountId,
}

impl Fx {
    async fn new() -> Self {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        Self {
            store,
            account: account.id,
        }
    }

    async fn message(&self, provider_id: &str, date: DateTime<Utc>) -> (MessageId, ThreadId) {
        let mut envelope = TestEnvelopeBuilder::new()
            .account_id(self.account.clone())
            .build();
        envelope.provider_id = provider_id.to_string();
        envelope.thread_id = ThreadId::new();
        envelope.date = date;
        envelope.from = Address {
            name: None,
            email: "bills@camden.gov.uk".to_string(),
        };
        self.store
            .upsert_envelope_with_direction(&envelope, MessageDirection::Inbound)
            .await
            .unwrap();
        (envelope.id, envelope.thread_id)
    }

    fn record(&self, id: &str, source: &(MessageId, ThreadId), date: DateTime<Utc>) -> TodoRecord {
        TodoRecord {
            id: id.to_string(),
            account_id: self.account.clone(),
            thread_id: Some(source.1.clone()),
            source_message_id: Some(source.0.clone()),
            source_date: Some(date),
            kind: "bill".to_string(),
            verb: "pay".to_string(),
            doc_type: Some("bill_link".to_string()),
            title: "Pay council tax".to_string(),
            counterparty: Some("Camden Council".to_string()),
            sender_domain: Some("camden.gov.uk".to_string()),
            amount_minor: Some(14200),
            currency: Some("GBP".to_string()),
            due_at: Some(at(200)),
            due_words: Some("payment due 9 October".to_string()),
            act_by_at: Some(at(200)),
            surface_at: Some(at(100)),
            scheduled_for: None,
            action_url: Some("https://www.camden.gov.uk/pay".to_string()),
            action_domain: Some("camden.gov.uk".to_string()),
            relevant_until: Some(at(500)),
            window_source: Some("rule".to_string()),
            state: TodoState::Open,
            expired_at: None,
            expired_at_birth: false,
            catchup: None,
            looks_done_message_id: None,
            looks_done_reason: None,
            origin: "rule".to_string(),
            reason: "\"payment due 9 October\" (rule)".to_string(),
            field_sources: r#"{"due_at":{"source":"rule","evidence":"payment due 9 October"}}"#
                .to_string(),
            user_edited: false,
            commitment_id: None,
            rules_version: 1,
            dedup_key: "bill|camden.gov.uk|council tax|due:2026-10-09".to_string(),
            surfaced_at: None,
            created_at: date,
            updated_at: date,
            done_at: None,
            dismissed_at: None,
        }
    }
}

#[tokio::test]
async fn a_reminder_updates_the_row_and_older_mail_never_overwrites_it() {
    let fx = Fx::new().await;
    let first = fx.message("first", at(0)).await;
    let reminder = fx.message("reminder", at(48)).await;

    let inserted = fx
        .store
        .upsert_detected_todo(&fx.record("t1", &first, at(0)))
        .await
        .unwrap();
    assert_eq!(
        inserted,
        TodoUpsert::Inserted {
            id: "t1".into(),
            state: TodoState::Open
        }
    );

    let mut newer = fx.record("t2", &reminder, at(48));
    newer.amount_minor = Some(15000);
    newer.field_sources = "{}".into();
    assert_eq!(
        fx.store.upsert_detected_todo(&newer).await.unwrap(),
        TodoUpsert::Updated {
            id: "t1".into(),
            reopened: false
        }
    );
    let row = fx.store.get_todo("t1").await.unwrap().unwrap();
    assert_eq!(row.amount_minor, Some(15000));
    assert_eq!(row.source_message_id.as_ref(), Some(&reminder.0));

    // Classifying newest first meets the first bill after the reminder.
    assert_eq!(
        fx.store
            .upsert_detected_todo(&fx.record("t3", &first, at(0)))
            .await
            .unwrap(),
        TodoUpsert::Unchanged
    );
    assert_eq!(
        fx.store.get_todo("t1").await.unwrap().unwrap().amount_minor,
        Some(15000)
    );
}

#[tokio::test]
async fn re_running_the_same_evidence_changes_nothing() {
    let fx = Fx::new().await;
    let source = fx.message("bill", at(0)).await;
    fx.store
        .upsert_detected_todo(&fx.record("t1", &source, at(0)))
        .await
        .unwrap();
    assert_eq!(
        fx.store
            .upsert_detected_todo(&fx.record("t2", &source, at(0)))
            .await
            .unwrap(),
        TodoUpsert::Unchanged
    );
}

#[tokio::test]
async fn a_row_the_user_touched_keeps_its_fields_and_newer_evidence_reopens_an_expired_one() {
    let fx = Fx::new().await;
    let first = fx.message("first", at(0)).await;
    let reminder = fx.message("reminder", at(48)).await;
    let mut row = fx.record("t1", &first, at(0));
    row.state = TodoState::Expired;
    row.expired_at = Some(at(1));
    row.expired_at_birth = true;
    fx.store.upsert_detected_todo(&row).await.unwrap();

    let mut final_notice = fx.record("t2", &reminder, at(48));
    final_notice.field_sources = "{}".into();
    assert_eq!(
        fx.store.upsert_detected_todo(&final_notice).await.unwrap(),
        TodoUpsert::Updated {
            id: "t1".into(),
            reopened: true
        }
    );
    let reopened = fx.store.get_todo("t1").await.unwrap().unwrap();
    assert_eq!(reopened.state, TodoState::Open);
    assert_eq!(reopened.expired_at, None);

    let mut edited = reopened.clone();
    edited.title = "Pay the council".into();
    fx.store.update_todo_by_user(&edited, at(50)).await.unwrap();
    let later = fx.message("later", at(72)).await;
    let mut newest = fx.record("t3", &later, at(72));
    newest.field_sources = r#"{"x":1}"#.into();
    assert_eq!(
        fx.store.upsert_detected_todo(&newest).await.unwrap(),
        TodoUpsert::Unchanged
    );
    assert_eq!(
        fx.store.get_todo("t1").await.unwrap().unwrap().title,
        "Pay the council"
    );
}

#[tokio::test]
async fn a_row_is_claimed_once() {
    let fx = Fx::new().await;
    let source = fx.message("bill", at(0)).await;
    fx.store
        .upsert_detected_todo(&fx.record("t1", &source, at(0)))
        .await
        .unwrap();
    assert!(
        fx.store
            .claim_surfaced_todos(at(99))
            .await
            .unwrap()
            .is_empty(),
        "not yet"
    );
    assert_eq!(
        fx.store.claim_surfaced_todos(at(100)).await.unwrap(),
        vec!["t1".to_string()]
    );
    assert!(
        fx.store
            .claim_surfaced_todos(at(101))
            .await
            .unwrap()
            .is_empty(),
        "never twice"
    );
    // A re-run keeps the claim.
    let mut rerun = fx.record("t2", &source, at(0));
    rerun.rules_version = 2;
    fx.store.upsert_detected_todo(&rerun).await.unwrap();
    assert_eq!(
        fx.store.get_todo("t1").await.unwrap().unwrap().surfaced_at,
        Some(at(100))
    );
}

#[tokio::test]
async fn the_sweep_expires_detected_rows_but_not_bills_promises_or_yours() {
    let fx = Fx::new().await;
    let mut ids = Vec::new();
    for (id, kind, origin, edited) in [
        ("verify", "verify", "rule", false),
        ("bill", "bill", "rule", false),
        ("promise", "promise", "model", false),
        ("mine", "other", "manual", false),
        ("edited", "verify", "rule", true),
    ] {
        let source = fx.message(id, at(0)).await;
        let mut row = fx.record(id, &source, at(0));
        row.kind = kind.into();
        row.origin = origin.into();
        row.user_edited = edited;
        row.dedup_key = id.into();
        row.relevant_until = Some(at(10));
        fx.store.upsert_detected_todo(&row).await.unwrap();
        ids.push(id);
    }
    assert_eq!(fx.store.expire_lapsed_todos(at(11)).await.unwrap(), 1);
    for id in ids {
        let state = fx.store.get_todo(id).await.unwrap().unwrap().state;
        let expected = if id == "verify" {
            TodoState::Expired
        } else {
            TodoState::Open
        };
        assert_eq!(state, expected, "{id}");
    }
    assert_eq!(
        fx.store
            .count_todos_expired_since(None, at(0))
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn the_catch_up_keeps_the_cap_across_accounts_by_act_by_then_newest() {
    let fx = Fx::new().await;
    let other = test_account();
    fx.store.insert_account(&other).await.unwrap();
    for n in 0..6_i64 {
        let source = fx.message(&format!("m{n}"), at(n)).await;
        let mut row = fx.record(&format!("t{n}"), &source, at(n));
        row.dedup_key = format!("k{n}");
        row.catchup = Some(TodoCatchup::Pending);
        // This test ranks the current first run, not pending legacy upgrades.
        row.rules_version = 2;
        row.act_by_at = (n == 0).then(|| at(300));
        if n % 2 == 1 {
            row.account_id = other.id.clone();
        }
        fx.store.upsert_detected_todo(&row).await.unwrap();
    }
    assert_eq!(fx.store.trim_todo_catchup(3, at(10)).await.unwrap(), 3);
    let kept: Vec<String> = fx
        .store
        .list_catchup_todos(None)
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.id)
        .collect();
    // Dated first, then the newest undated ones.
    assert_eq!(kept, vec!["t0", "t5", "t4"]);
    assert_eq!(fx.store.count_catchup_overflow(None).await.unwrap(), 3);
    assert_eq!(
        fx.store
            .count_todos_expired_since(None, at(0))
            .await
            .unwrap(),
        0,
        "overflow never counts as expired since you last looked"
    );
}

#[tokio::test]
async fn state_changes_apply_only_from_the_states_given() {
    let fx = Fx::new().await;
    let source = fx.message("bill", at(0)).await;
    fx.store
        .upsert_detected_todo(&fx.record("t1", &source, at(0)))
        .await
        .unwrap();
    let ids = vec!["t1".to_string()];
    assert_eq!(
        fx.store
            .set_todos_state(&ids, &[TodoState::Open], TodoState::Done, at(5))
            .await
            .unwrap(),
        ids
    );
    assert!(fx
        .store
        .set_todos_state(&ids, &[TodoState::Open], TodoState::Done, at(6))
        .await
        .unwrap()
        .is_empty());
    let reopened = fx
        .store
        .set_todos_state(&ids, &[TodoState::Done], TodoState::Open, at(7))
        .await
        .unwrap();
    assert_eq!(reopened, ids);
    let row = fx.store.get_todo("t1").await.unwrap().unwrap();
    assert!(
        row.user_edited,
        "a reopened row is the user's and never expires"
    );
    assert_eq!(row.done_at, None);
}

#[tokio::test]
async fn deleting_the_email_deletes_a_detected_to_do_and_strips_one_the_user_edited() {
    let fx = Fx::new().await;
    let detected = fx.message("detected", at(0)).await;
    let edited = fx.message("edited", at(1)).await;
    fx.store
        .upsert_detected_todo(&fx.record("t1", &detected, at(0)))
        .await
        .unwrap();
    let mut mine = fx.record("t2", &edited, at(1));
    mine.dedup_key = "mine".into();
    mine.user_edited = true;
    mine.field_sources =
        r#"{"title":{"source":"user","evidence":"you set it"},"due_at":{"source":"rule","evidence":"payment due 9 October"}}"#
            .into();
    fx.store.insert_todo(&mine).await.unwrap();

    fx.store
        .delete_messages_and_derived(&fx.account, &["detected".to_string(), "edited".to_string()])
        .await
        .unwrap();
    assert_eq!(fx.store.get_todo("t1").await.unwrap(), None);
    let kept = fx
        .store
        .get_todo("t2")
        .await
        .unwrap()
        .expect("the user's to-do stays");
    assert_eq!(kept.title, "Pay council tax");
    assert_eq!(kept.source_message_id, None);
    assert_eq!(kept.due_words, None);
    assert_eq!(kept.action_url, None);
    assert_eq!(kept.reason, "Its email was deleted.");
    assert!(
        !kept.field_sources.contains("payment due"),
        "{}",
        kept.field_sources
    );
    assert!(
        kept.field_sources.contains("\"user\""),
        "{}",
        kept.field_sources
    );
}

#[tokio::test]
async fn a_prefix_finds_the_row_literally() {
    let fx = Fx::new().await;
    let source = fx.message("bill", at(0)).await;
    fx.store
        .upsert_detected_todo(&fx.record("todo_ab%cd", &source, at(0)))
        .await
        .unwrap();
    assert_eq!(
        fx.store
            .find_todo_ids_by_prefix("todo_ab%", 2)
            .await
            .unwrap(),
        vec!["todo_ab%cd"]
    );
    assert!(fx
        .store
        .find_todo_ids_by_prefix("todo_a_", 2)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn a_scheduled_row_survives_its_emails_deletion_and_an_edit_announces_again() {
    let fx = Fx::new().await;
    let source = fx.message("bill", at(0)).await;
    fx.store
        .upsert_detected_todo(&fx.record("t1", &source, at(0)))
        .await
        .unwrap();
    assert_eq!(
        fx.store.claim_surfaced_todos(at(100)).await.unwrap(),
        vec!["t1".to_string()]
    );

    let mut edited = fx.store.get_todo("t1").await.unwrap().unwrap();
    edited.surface_at = Some(at(150));
    fx.store
        .update_todo_by_user(&edited, at(101))
        .await
        .unwrap();
    assert_eq!(
        fx.store.get_todo("t1").await.unwrap().unwrap().surfaced_at,
        None
    );

    let other = fx.message("other", at(1)).await;
    let mut scheduled = fx.record("t2", &other, at(1));
    scheduled.dedup_key = "scheduled".into();
    fx.store.upsert_detected_todo(&scheduled).await.unwrap();
    fx.store
        .schedule_todo("t2", Some(at(200)), at(2))
        .await
        .unwrap();
    fx.store
        .delete_messages_and_derived(&fx.account, &["other".to_string()])
        .await
        .unwrap();
    let kept = fx
        .store
        .get_todo("t2")
        .await
        .unwrap()
        .expect("a scheduled row is the user's");
    assert_eq!(kept.source_message_id, None);
    assert_eq!(kept.due_words, None);
}

#[tokio::test]
async fn a_new_due_date_on_the_same_evidence_updates_the_row() {
    let fx = Fx::new().await;
    let source = fx.message("promise", at(0)).await;
    fx.store
        .upsert_detected_todo(&fx.record("t1", &source, at(0)))
        .await
        .unwrap();
    let mut moved = fx.record("t2", &source, at(0));
    moved.due_at = Some(at(300));
    assert!(matches!(
        fx.store.upsert_detected_todo(&moved).await.unwrap(),
        TodoUpsert::Updated { .. }
    ));
    assert_eq!(
        fx.store.get_todo("t1").await.unwrap().unwrap().due_at,
        Some(at(300))
    );
}

#[tokio::test]
async fn newer_evidence_never_clears_the_date_you_chose() {
    let fx = Fx::new().await;
    let first = fx.message("first", at(0)).await;
    let reminder = fx.message("reminder", at(48)).await;
    fx.store
        .upsert_detected_todo(&fx.record("t1", &first, at(0)))
        .await
        .unwrap();
    fx.store
        .schedule_todo("t1", Some(at(400)), at(1))
        .await
        .unwrap();
    let mut newer = fx.record("t2", &reminder, at(48));
    newer.amount_minor = Some(15000);
    newer.updated_at = at(48);
    fx.store.upsert_detected_todo(&newer).await.unwrap();
    let row = fx.store.get_todo("t1").await.unwrap().unwrap();
    assert_eq!(row.scheduled_for, Some(at(400)));
    assert_eq!(row.amount_minor, Some(15000), "the facts still update");
}

#[tokio::test]
async fn correction_restores_every_prior_field_and_rejects_same_second_newer_work() {
    let fx = Fx::new().await;
    let source = fx.message("correction", at(0)).await;
    let before = fx.record("corrected", &source, at(0));
    fx.store.insert_todo(&before).await.unwrap();
    let mut edited = before.clone();
    edited.title = "Pay the right account".into();
    edited.amount_minor = Some(4500);
    edited.user_edited = true;
    edited.state = TodoState::Done;
    edited.done_at = Some(at(1));
    edited.field_sources =
        r#"{"correction":{"source":"user","evidence":"wrong_details:one"}}"#.into();
    edited.updated_at = at(1);
    fx.store
        .correct_todo("one", "wrong_details", &before, &edited, true, at(1))
        .await
        .unwrap();
    assert_eq!(
        fx.store.get_todo(&before.id).await.unwrap(),
        Some(before.clone())
    );
    fx.store
        .correct_todo("one", "wrong_details", &before, &edited, false, at(1))
        .await
        .unwrap();
    let restored = fx
        .store
        .undo_todo_correction(&before.id, "one", false, at(2))
        .await
        .unwrap();
    assert_eq!(restored, before);
    fx.store
        .correct_todo("two", "not_mine", &before, &edited, false, at(1))
        .await
        .unwrap();
    let mut newer = edited.clone();
    newer.title = "Keep my newer title".into();
    fx.store.update_todo_by_user(&newer, at(1)).await.unwrap();
    let error = fx
        .store
        .undo_todo_correction(&before.id, "two", false, at(2))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("newer work was kept"));
    assert_eq!(
        fx.store.get_todo(&before.id).await.unwrap().unwrap().title,
        newer.title
    );
}

#[tokio::test]
async fn all_correction_reasons_survive_and_source_quote_cannot_be_edited() {
    let fx = Fx::new().await;
    let source = fx.message("reasons", at(0)).await;
    for reason in ["not_a_todo", "not_mine", "already_done", "wrong_details"] {
        let before = fx.record(reason, &source, at(0));
        fx.store
            .insert_todo(&TodoRecord {
                dedup_key: reason.into(),
                ..before.clone()
            })
            .await
            .unwrap();
        let before = fx.store.get_todo(reason).await.unwrap().unwrap();
        let mut after = before.clone();
        after.user_edited = true;
        fx.store
            .correct_todo(reason, reason, &before, &after, false, at(1))
            .await
            .unwrap();
        let recorded: String =
            sqlx::query_scalar("SELECT reason FROM todo_corrections WHERE id = ?")
                .bind(reason)
                .fetch_one(fx.store.reader())
                .await
                .unwrap();
        assert_eq!(recorded, reason);
        let mut wrong = after.clone();
        wrong.reason = "edited quote".into();
        assert!(fx
            .store
            .correct_todo("bad", reason, &after, &wrong, false, at(2))
            .await
            .is_err());
    }
}

#[tokio::test]
async fn ambiguous_legacy_preserves_closed_manual_history_without_freezing_unrelated_tasks() {
    use mxr_core::asks::*;
    let fx = Fx::new().await;
    let first = fx.message("legacy", at(0)).await;
    let other = fx.message("unrelated", at(1)).await;
    let make_answer = |id: MessageId| AskAssessment {
        message_id: id,
        account_id: fx.account.clone(),
        policy_version: ASKS_POLICY_VERSION.into(),
        input_fingerprint: "stable".into(),
        pending_body: false,
        context_token: crate::asks_context_token().unwrap(),
        calendar: None,
        html: None,
        facts: AskFacts {
            zones: vec![],
            recipient_role: RecipientRole::AloneTo,
            owner_authored: false,
            owner_names: vec![],
            owner_addresses: vec![],
            predecessor: None,
            owner_replied_afterwards: false,
            amounts: vec![],
            dates: vec![],
            references: vec![],
            template_key: "x".into(),
        },
        units: vec![],
    };
    let ask = make_answer(first.0.clone());
    let other_ask = make_answer(other.0.clone());
    let revisions = fx
        .store
        .assessment_thread_revisions(&fx.account, &[first.1.clone(), other.1.clone()])
        .await
        .unwrap();
    fx.store
        .save_asks_batch(&fx.account, &revisions, &[ask.clone(), other_ask.clone()])
        .await
        .unwrap();
    let mut legacy = fx.record("legacy", &first, at(0));
    legacy.state = TodoState::Done;
    legacy.done_at = Some(at(1));
    legacy.user_edited = true;
    fx.store.insert_todo(&legacy).await.unwrap();
    let mut unrelated = fx.record("unrelated", &other, at(1));
    unrelated.origin = "rule".into();
    unrelated.rules_version = 2;
    unrelated.dedup_key = "asks|other".into();
    assert!(matches!(
        fx.store
            .upsert_asks_todo(&unrelated, &other_ask)
            .await
            .unwrap(),
        TodoUpsert::Inserted { .. }
    ));
    let keys = vec!["asks|candidate-a".into(), "asks|candidate-b".into()];
    assert!(fx
        .store
        .upgrade_asks_todo(&legacy, None, Some(&ask), &keys)
        .await
        .unwrap());
    let after = fx.store.get_todo("legacy").await.unwrap().unwrap();
    assert_eq!(after.state, TodoState::Done);
    assert_eq!(after.done_at, legacy.done_at);
    assert_eq!(after.title, legacy.title);
    assert_eq!(after.field_sources, legacy.field_sources);
    assert_eq!(after.dedup_key, legacy.dedup_key);
    for (n, key) in keys.iter().enumerate() {
        let mut candidate = fx.record(&format!("candidate-{n}"), &first, at(0));
        candidate.origin = "rule".into();
        candidate.rules_version = 2;
        candidate.dedup_key = key.clone();
        assert_eq!(
            fx.store.upsert_asks_todo(&candidate, &ask).await.unwrap(),
            TodoUpsert::Unchanged
        );
    }
    assert!(fx
        .store
        .legacy_asks_todos(&fx.account)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn missing_body_legacy_rows_cannot_starve_ready_sources_beyond_the_page() {
    let fx = Fx::new().await;
    for index in 0..101 {
        let source = fx.message(&format!("source-{index}"), at(0)).await;
        let mut row = fx.record(&format!("legacy-{index:03}"), &source, at(0));
        row.dedup_key = format!("legacy-{index}");
        fx.store.insert_todo(&row).await.unwrap();
        if index == 100 {
            fx.store
                .insert_body(&mxr_core::MessageBody {
                    message_id: source.0,
                    text_plain: Some("Please sign the contract.".into()),
                    text_html: None,
                    attachments: vec![],
                    fetched_at: at(0),
                    metadata: Default::default(),
                })
                .await
                .unwrap();
        }
    }
    let ready = fx.store.legacy_asks_todos(&fx.account).await.unwrap();
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].id, "legacy-100");
}

#[tokio::test]
async fn pending_legacy_catchup_keeps_history_until_source_reconciliation() {
    let fx = Fx::new().await;
    let source = fx.message("legacy", at(0)).await;
    let mut row = fx.record("legacy", &source, at(0));
    row.catchup = Some(TodoCatchup::Pending);
    fx.store.upsert_detected_todo(&row).await.unwrap();
    assert!(fx.store.list_catchup_todos(None).await.unwrap().is_empty());
    assert_eq!(fx.store.get_todo(&row.id).await.unwrap(), Some(row));
}

/// Current stored answers for `sources`, so asks writes pass their authority check.
async fn saved_asks(
    fx: &Fx,
    sources: &[&(MessageId, ThreadId)],
) -> Vec<mxr_core::asks::AskAssessment> {
    use mxr_core::asks::*;
    let answers: Vec<_> = sources
        .iter()
        .map(|source| AskAssessment {
            message_id: source.0.clone(),
            account_id: fx.account.clone(),
            policy_version: ASKS_POLICY_VERSION.into(),
            input_fingerprint: "stable".into(),
            pending_body: false,
            context_token: crate::asks_context_token().unwrap(),
            calendar: None,
            html: None,
            facts: AskFacts {
                zones: vec![],
                recipient_role: RecipientRole::AloneTo,
                owner_authored: false,
                owner_names: vec![],
                owner_addresses: vec![],
                predecessor: None,
                owner_replied_afterwards: false,
                amounts: vec![],
                dates: vec![],
                references: vec![],
                template_key: "x".into(),
            },
            units: vec![],
        })
        .collect();
    let threads: Vec<_> = sources.iter().map(|source| source.1.clone()).collect();
    let revisions = fx
        .store
        .assessment_thread_revisions(&fx.account, &threads)
        .await
        .unwrap();
    fx.store
        .save_asks_batch(&fx.account, &revisions, &answers)
        .await
        .unwrap();
    answers
}

#[tokio::test]
async fn a_redated_item_keeps_its_emails_row_and_never_takes_a_sibling_or_another_emails() {
    let fx = Fx::new().await;
    let source = fx.message("bill", at(0)).await;
    let next = fx.message("next-bill", at(1)).await;
    let answers = saved_asks(&fx, &[&source, &next]).await;
    let (ask, next_ask) = (&answers[0], &answers[1]);
    let key = |period: &str| format!("asks|payment|fp|{period}");
    let keyed = |id: &str, source: &(MessageId, ThreadId), period: &str| TodoRecord {
        dedup_key: key(period),
        rules_version: 2,
        ..fx.record(id, source, at(0))
    };
    let key_of = |id: &'static str| {
        let store = &fx.store;
        async move { store.get_todo(id).await.unwrap().unwrap().dedup_key }
    };
    fx.store
        .insert_todo(&keyed("october", &source, "due:2026-10"))
        .await
        .unwrap();
    fx.store
        .insert_todo(&keyed("theirs", &next, "due:2026-12"))
        .await
        .unwrap();
    let november = keyed("rescan", &source, "due:2026-11");

    // This email still has an item due in October: that row is its sibling.
    fx.store
        .adopt_asks_same_source(&november, ask, &[key("due:2026-10"), key("due:2026-11")])
        .await
        .unwrap();
    assert_eq!(key_of("october").await, key("due:2026-10"));

    // A stale answer moves nothing.
    let mut stale = ask.clone();
    stale.input_fingerprint = "older".into();
    fx.store
        .adopt_asks_same_source(&november, &stale, &[key("due:2026-11")])
        .await
        .unwrap();
    assert_eq!(key_of("october").await, key("due:2026-10"));

    fx.store
        .adopt_asks_same_source(&november, ask, &[key("due:2026-11")])
        .await
        .unwrap();
    assert_eq!(key_of("october").await, key("due:2026-11"));
    assert_eq!(key_of("theirs").await, key("due:2026-12"));

    // Another email's row for the same item is next month's bill, not this one.
    let next_rescan = keyed("next-rescan", &next, "due:2027-01");
    fx.store
        .adopt_asks_same_source(&next_rescan, next_ask, &[key("due:2027-01")])
        .await
        .unwrap();
    assert_eq!(key_of("october").await, key("due:2026-11"));
    assert_eq!(key_of("theirs").await, key("due:2027-01"));

    // A key some row already holds is never taken a second time.
    let back = keyed("back", &source, "due:2027-01");
    fx.store
        .adopt_asks_same_source(&back, ask, &[key("due:2027-01")])
        .await
        .unwrap();
    assert_eq!(key_of("october").await, key("due:2026-11"));
}
