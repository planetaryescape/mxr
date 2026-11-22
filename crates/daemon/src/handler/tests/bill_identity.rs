use super::desk::{request, Fixture, ME};
use super::promise_projection::{replace, settle};
use super::*;
use chrono::Duration;
use mxr_core::asks::{AskAssessment, AskReference, AskSpan, ASKS_POLICY_VERSION};
use mxr_core::{Envelope, MessageDirection, ThreadId};
use mxr_store::{TodoRecord, TodoState};

const PREVIOUS_POLICY: &str = "asks-r2c-17";

fn deadline(days: i64) -> chrono::NaiveDate {
    chrono::Utc::now().date_naive() + Duration::days(days)
}

fn written(day: chrono::NaiveDate) -> String {
    day.format("%-d %B %Y").to_string()
}

async fn mail(fx: &Fixture, subject: &str, body: &str, age: Duration) -> Envelope {
    let mut envelope = fx
        .message(&ThreadId::new(), "billing@example.com", ME, age, None)
        .await;
    envelope.subject = subject.into();
    fx.store_envelope(&envelope, MessageDirection::Inbound)
        .await;
    replace(fx, &envelope, body).await;
    envelope
}

async fn open_rows(fx: &Fixture) -> Vec<TodoRecord> {
    fx.state
        .store
        .list_todos_in_state(Some(&fx.account), TodoState::Open, 100)
        .await
        .unwrap()
}

async fn reconcile(fx: &Fixture) {
    super::super::assessments::reconcile_account(&fx.state, &fx.account)
        .await
        .unwrap();
}

async fn row_count(fx: &Fixture) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM todos")
        .fetch_one(fx.state.store.reader())
        .await
        .unwrap()
}

fn levy_body(due: chrono::NaiveDate) -> String {
    format!(
        "Your levy invoice is attached.\nAmount due: R953.63\nPlease pay by {}.",
        written(due)
    )
}

fn bank_body(reference: &str, amount: &str, due: chrono::NaiveDate) -> String {
    format!(
        "Pay into: FNB\nAccount number: 62012345678\nPayment reference: {reference}\nAmount due: {amount}\nPlease pay by {}.",
        written(due)
    )
}

async fn store_under_previous_policy(
    fx: &Fixture,
    envelope: &Envelope,
    forget: impl Fn(&mut AskAssessment),
) -> String {
    let mut old = fx
        .state
        .store
        .asks_for_messages(&fx.account, std::slice::from_ref(&envelope.id))
        .await
        .unwrap()
        .remove(&envelope.id)
        .expect("current answer");
    forget(&mut old);
    old.policy_version = PREVIOUS_POLICY.into();
    let (_, detection) = mxr_todo::asks::items::detections(&old, envelope)
        .into_iter()
        .next()
        .expect("old detection");
    let period = detection.due.expect("due").at.format("due:%Y-%m");
    let key = format!(
        "asks|payment|{}|{period}",
        mxr_store::assessment_fingerprint(&detection.object_key)
    );
    sqlx::query("UPDATE mail_assessments SET policy_version=?1, assessment_json=?2 WHERE message_id=?3 AND policy_version=?4")
        .bind(PREVIOUS_POLICY).bind(serde_json::to_string(&old).unwrap()).bind(envelope.id.as_str()).bind(ASKS_POLICY_VERSION)
        .execute(fx.state.store.writer()).await.unwrap();
    sqlx::query("UPDATE todos SET dedup_key=?1, field_sources=replace(field_sources, ?2, ?3) WHERE source_message_id=?4")
        .bind(&key).bind(format!("{ASKS_POLICY_VERSION}:")).bind(format!("{PREVIOUS_POLICY}:")).bind(envelope.id.as_str())
        .execute(fx.state.store.writer()).await.unwrap();
    key
}

fn without_subject_reference(old: &mut AskAssessment) {
    for unit in &mut old.units {
        if let Some(tags) = &mut unit.tags {
            tags.reference = None;
        }
        for vote in &mut unit.votes {
            vote.tags.reference = None;
        }
    }
}

fn on_bank_account(old: &mut AskAssessment) {
    let zone = old.units.iter().find(|u| u.tags.is_some()).unwrap().zone;
    let text = &old.facts.zones[zone].text;
    let start = text.find("Account number").unwrap();
    let end = text.find("62012345678").unwrap() + "62012345678".len();
    old.facts.references = vec![AskReference {
        zone,
        span: AskSpan { start, end },
        kind: "account".into(),
        value: "62012345678".into(),
    }];
    for unit in &mut old.units {
        if let Some(tags) = &mut unit.tags {
            tags.reference = Some(0);
        }
        for vote in &mut unit.votes {
            vote.tags.reference = Some(0);
        }
    }
}

#[tokio::test]
async fn a_payment_reference_leaves_the_amount_due_on_the_row() {
    let fx = Fixture::new().await;
    let body = format!(
        "Amount due: R953.63\nPayment reference: R123456\nPlease pay by {}.",
        written(deadline(9))
    );
    let bill = mail(&fx, "Your levy", &body, Duration::minutes(5)).await;
    settle(&fx, &bill).await;
    let rows = open_rows(&fx).await;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].amount_minor, Some(95363));
    assert_eq!(rows[0].currency.as_deref(), Some("ZAR"));
}

#[tokio::test]
async fn two_accounts_named_only_in_the_subject_keep_two_rows() {
    let fx = Fixture::new().await;
    let body = levy_body(deadline(9));
    for subject in [
        "Levy invoice - Account A1234",
        "Levy invoice - Account B5678",
    ] {
        let bill = mail(&fx, subject, &body, Duration::minutes(5)).await;
        settle(&fx, &bill).await;
    }
    reconcile(&fx).await;
    assert_eq!(open_rows(&fx).await.len(), 2);
}

#[tokio::test]
async fn bills_paid_into_one_bank_account_keep_their_own_row_and_amount() {
    let fx = Fixture::new().await;
    let due = deadline(9);
    for (reference, amount) in [("A1234", "R953.63"), ("B5678", "R1,200.00")] {
        let bill = mail(
            &fx,
            "Your levy",
            &bank_body(reference, amount, due),
            Duration::minutes(5),
        )
        .await;
        settle(&fx, &bill).await;
    }
    reconcile(&fx).await;
    let rows = open_rows(&fx).await;
    assert_eq!(rows.len(), 2, "{rows:?}");
    let mut amounts: Vec<_> = rows.iter().filter_map(|r| r.amount_minor).collect();
    amounts.sort_unstable();
    assert_eq!(amounts, [95363, 120000]);
}

#[tokio::test]
async fn sub_accounts_with_dotted_references_keep_two_rows() {
    let fx = Fixture::new().await;
    let due = deadline(9);
    for reference in ["A1234.01", "A1234.02"] {
        let bill = mail(
            &fx,
            "Your levy",
            &bank_body(reference, "R953.63", due),
            Duration::minutes(5),
        )
        .await;
        settle(&fx, &bill).await;
    }
    reconcile(&fx).await;
    assert_eq!(open_rows(&fx).await.len(), 2);
}

#[tokio::test]
async fn an_undated_overdue_notice_keeps_the_due_date_and_the_row_overdue() {
    let fx = Fixture::new().await;
    let due = deadline(-3);
    let first = mail(
        &fx,
        "Your account",
        &format!("Account #A123 amount due £25 by {}.", written(due)),
        Duration::days(10),
    )
    .await;
    settle(&fx, &first).await;
    let dated = open_rows(&fx).await.remove(0);
    assert!(dated.due_at.is_some());
    let notice = mail(
        &fx,
        "Your account",
        "Account #A123 amount due £25 is now overdue.",
        Duration::minutes(5),
    )
    .await;
    settle(&fx, &notice).await;
    let rows = open_rows(&fx).await;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].id, dated.id);
    assert_eq!(rows[0].source_message_id.as_ref(), Some(&notice.id));
    assert_eq!(rows[0].due_at, dated.due_at);
    assert_eq!(rows[0].due_words, dated.due_words);
    let ResponseData::Todos { todos } = request(
        &fx,
        Request::ListTodos {
            account_id: Some(fx.account.clone()),
            state: mxr_protocol::TodoStateData::Open,
            limit: 100,
        },
    )
    .await
    else {
        panic!("list");
    };
    assert!(todos[0].overdue, "{todos:?}");
    replace(
        &fx,
        &notice,
        "Account #A123 amount due £25 is now seriously overdue.",
    )
    .await;
    settle(&fx, &notice).await;
    let rows = open_rows(&fx).await;
    assert_eq!(rows[0].due_at, dated.due_at, "a rescan keeps the date");
}

#[tokio::test]
async fn a_corrected_mail_that_drops_its_date_does_not_keep_the_old_date() {
    let fx = Fixture::new().await;
    let bill = mail(
        &fx,
        "Your account",
        &format!("Account #A123 amount due £25 by {}.", written(deadline(9))),
        Duration::minutes(5),
    )
    .await;
    settle(&fx, &bill).await;
    replace(&fx, &bill, "Account #A123 amount due £25 is now overdue.").await;
    settle(&fx, &bill).await;
    let rows = open_rows(&fx).await;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].due_at, None);
}

#[tokio::test]
async fn clear_debt_notices_create_a_to_do() {
    for body in [
        "Please settle the current arrears of R12 500.00 immediately.",
        "Your account is overdue. Amount outstanding: R953.63.",
    ] {
        let fx = Fixture::new().await;
        let notice = mail(&fx, "Your account", body, Duration::minutes(5)).await;
        settle(&fx, &notice).await;
        let rows = open_rows(&fx).await;
        assert_eq!(rows.len(), 1, "{body}: {rows:?}");
        assert_eq!(rows[0].kind, "bill");
    }
}

#[tokio::test]
async fn a_row_under_the_old_subject_identity_is_adopted_on_upgrade() {
    for closed in [false, true] {
        let fx = Fixture::new().await;
        let bill = mail(
            &fx,
            "Levy invoice - Account A1234",
            &levy_body(deadline(9)),
            Duration::minutes(5),
        )
        .await;
        settle(&fx, &bill).await;
        let original = open_rows(&fx).await.remove(0);
        if closed {
            request(
                &fx,
                Request::SetTodoState {
                    todo_ids: vec![original.id.clone()],
                    action: mxr_protocol::TodoStateActionData::Done,
                    dry_run: false,
                },
            )
            .await;
        }
        let old_key = store_under_previous_policy(&fx, &bill, without_subject_reference).await;
        settle(&fx, &bill).await;
        assert_eq!(row_count(&fx).await, 1, "closed={closed}");
        let kept = fx
            .state
            .store
            .get_todo(&original.id)
            .await
            .unwrap()
            .unwrap();
        if closed {
            assert_eq!(kept.state, TodoState::Done);
            assert_eq!(kept.dedup_key, old_key, "Undo keeps its exact snapshot");
            assert!(open_rows(&fx).await.is_empty());
        } else {
            assert_eq!(kept.state, TodoState::Open);
            assert_ne!(kept.dedup_key, old_key);
        }
    }
}

#[tokio::test]
async fn undoing_a_correction_after_upgrade_restores_one_live_row() {
    let fx = Fixture::new().await;
    let bill = mail(
        &fx,
        "Levy invoice - Account A1234",
        &levy_body(deadline(9)),
        Duration::minutes(5),
    )
    .await;
    settle(&fx, &bill).await;
    let original = open_rows(&fx).await.remove(0);
    let old_key = store_under_previous_policy(&fx, &bill, without_subject_reference).await;
    let ResponseData::TodoChange { change } = request(
        &fx,
        Request::CorrectTodo {
            todo_id: original.id.clone(),
            reason: mxr_protocol::TodoCorrectionData::AlreadyDone,
            edits: vec![],
            time_zone: None,
            dry_run: false,
        },
    )
    .await
    else {
        panic!("correct");
    };
    let token = change.correction_undo.unwrap();
    let corrected = fx
        .state
        .store
        .get_todo(&original.id)
        .await
        .unwrap()
        .unwrap();
    settle(&fx, &bill).await;
    let kept = fx
        .state
        .store
        .get_todo(&original.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(kept, corrected, "the closed row keeps its exact snapshot");
    assert_eq!(kept.dedup_key, old_key);
    request(
        &fx,
        Request::UndoTodoCorrection {
            todo_id: original.id.clone(),
            correction_id: token,
            dry_run: false,
        },
    )
    .await;
    settle(&fx, &bill).await;
    assert_eq!(row_count(&fx).await, 1);
    let rows = open_rows(&fx).await;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].id, original.id);
    assert_ne!(rows[0].dedup_key, old_key);
    let links: i64 = sqlx::query_scalar("SELECT count(*) FROM asks_legacy_links WHERE todo_id=?")
        .bind(&original.id)
        .fetch_one(fx.state.store.reader())
        .await
        .unwrap();
    assert_eq!(links, 0, "the reopened row takes later reminders");
}

#[tokio::test]
async fn a_row_under_the_old_bank_account_identity_is_adopted_on_upgrade() {
    let fx = Fixture::new().await;
    let bill = mail(
        &fx,
        "Your levy",
        &bank_body("A1234", "R953.63", deadline(9)),
        Duration::minutes(5),
    )
    .await;
    settle(&fx, &bill).await;
    let original = open_rows(&fx).await.remove(0);
    let old_key = store_under_previous_policy(&fx, &bill, on_bank_account).await;
    settle(&fx, &bill).await;
    let rows = open_rows(&fx).await;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].id, original.id);
    assert_ne!(rows[0].dedup_key, old_key);
}

#[tokio::test]
async fn one_merged_row_splits_into_each_accounts_own_bill_on_upgrade() {
    for newest_first in [false, true] {
        let fx = Fixture::new().await;
        let body = levy_body(deadline(9));
        let first = mail(
            &fx,
            "Levy invoice - Account A1234",
            &body,
            Duration::minutes(9),
        )
        .await;
        let second = mail(
            &fx,
            "Levy invoice - Account B5678",
            &body,
            Duration::minutes(5),
        )
        .await;
        settle(&fx, &first).await;
        settle(&fx, &second).await;
        sqlx::query("DELETE FROM todos WHERE source_message_id=?")
            .bind(first.id.as_str())
            .execute(fx.state.store.writer())
            .await
            .unwrap();
        let survivor = open_rows(&fx).await.remove(0).id;
        let old_first = store_under_previous_policy(&fx, &first, without_subject_reference).await;
        let old_second = store_under_previous_policy(&fx, &second, without_subject_reference).await;
        assert_eq!(old_first, old_second);
        for bill in if newest_first {
            [&second, &first]
        } else {
            [&first, &second]
        } {
            settle(&fx, bill).await;
        }
        let rows = open_rows(&fx).await;
        assert_eq!(rows.len(), 2, "newest_first={newest_first}: {rows:?}");
        let sources: std::collections::HashSet<_> = rows
            .iter()
            .filter_map(|r| r.source_message_id.clone())
            .collect();
        assert_eq!(sources.len(), 2, "{rows:?}");
        assert!(rows
            .iter()
            .any(|r| r.id == survivor && r.source_message_id.as_ref() == Some(&second.id)));
    }
}

#[tokio::test]
async fn a_reminder_for_one_account_stays_one_row_after_upgrade() {
    for newest_first in [false, true] {
        let fx = Fixture::new().await;
        let body = levy_body(deadline(9));
        let subject = "Levy invoice - Account A1234";
        let first = mail(&fx, subject, &body, Duration::minutes(9)).await;
        let reminder = mail(&fx, subject, &body, Duration::minutes(5)).await;
        settle(&fx, &first).await;
        settle(&fx, &reminder).await;
        assert_eq!(row_count(&fx).await, 1);
        store_under_previous_policy(&fx, &first, without_subject_reference).await;
        store_under_previous_policy(&fx, &reminder, without_subject_reference).await;
        for bill in if newest_first {
            [&reminder, &first]
        } else {
            [&first, &reminder]
        } {
            settle(&fx, bill).await;
        }
        let rows = open_rows(&fx).await;
        assert_eq!(rows.len(), 1, "newest_first={newest_first}: {rows:?}");
        assert_eq!(rows[0].source_message_id.as_ref(), Some(&reminder.id));
    }
}
