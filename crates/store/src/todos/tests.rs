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
            action_trusted: true,
            action_gate: None,
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
