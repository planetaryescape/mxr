//! Same email, same to-do: a rescan that moves a bill's due date into
//! another month keeps the row, while a new email makes a new one.
use super::*;

fn month_end() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 29, 12, 0, 0).unwrap()
}

async fn bill(fx: &Fixture, text: &str) -> MessageId {
    put(
        fx,
        Mail::new(
            ("Acme Billing", "bills@example.com"),
            "Your invoice",
            text,
            month_end() - Duration::hours(1),
        ),
    )
    .await
}

async fn rewrite(fx: &Fixture, id: &MessageId, text: &str) {
    let mut body = fx.state.store.get_body(id).await.unwrap().unwrap();
    body.text_plain = Some(text.into());
    fx.state.store.insert_body(&body).await.unwrap();
    scan(fx, std::slice::from_ref(id), month_end()).await;
}

/// Every row, whatever its state, so a retired duplicate can't hide.
async fn all_rows(fx: &Fixture) -> Vec<mxr_store::TodoRecord> {
    let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM todos")
        .fetch_all(fx.state.store.reader())
        .await
        .unwrap();
    let mut rows = fx.state.store.get_todos(&ids).await.unwrap();
    // Every scan runs at one pinned clock, so order by due date, not creation.
    rows.sort_by_key(|row| row.due_at);
    rows
}

async fn only_row(fx: &Fixture) -> mxr_store::TodoRecord {
    let mut rows = all_rows(fx).await;
    assert_eq!(rows.len(), 1, "{rows:#?}");
    rows.remove(0)
}

fn due_day(row: &mxr_store::TodoRecord) -> String {
    row.due_at
        .unwrap()
        .with_timezone(&Local)
        .format("%Y-%m-%d")
        .to_string()
}

#[tokio::test]
async fn a_bill_redated_into_next_month_keeps_its_to_do_and_snooze() {
    let fx = Fixture::new().await;
    let id = bill(&fx, "Invoice #A123 amount due £25 by 31 October 2026.").await;
    scan(&fx, std::slice::from_ref(&id), month_end()).await;
    let before = only_row(&fx).await;
    assert_eq!(due_day(&before), "2026-10-31");
    let snooze = month_end() + Duration::days(1);
    assert!(fx
        .state
        .store
        .schedule_todo(&before.id, Some(snooze), month_end())
        .await
        .unwrap());

    rewrite(&fx, &id, "Invoice #A123 amount due £25 by 1 November 2026.").await;
    let after = only_row(&fx).await;
    assert_eq!(after.id, before.id);
    assert_eq!(after.state, mxr_store::TodoState::Open);
    assert_eq!(after.scheduled_for, Some(snooze));
    assert_eq!(due_day(&after), "2026-11-01");
}

#[tokio::test]
async fn an_edited_bill_redated_into_next_month_stays_one_to_do_with_your_edit() {
    let fx = Fixture::new().await;
    let id = bill(&fx, "Invoice #A123 amount due £25 by 31 October 2026.").await;
    scan(&fx, std::slice::from_ref(&id), month_end()).await;
    let mut edited = only_row(&fx).await;
    edited.title = "Pay Acme from the joint account".into();
    assert!(fx
        .state
        .store
        .update_todo_by_user(&edited, month_end())
        .await
        .unwrap());

    rewrite(&fx, &id, "Invoice #A123 amount due £25 by 1 November 2026.").await;
    let after = only_row(&fx).await;
    assert_eq!(after.id, edited.id);
    assert_eq!(after.state, mxr_store::TodoState::Open);
    assert_eq!(after.title, "Pay Acme from the joint account");
    assert!(after.user_edited);
}

#[tokio::test]
async fn next_months_bill_in_a_new_email_is_a_new_to_do() {
    let fx = Fixture::new().await;
    let october = bill(&fx, "Invoice #A123 amount due £25 by 31 October 2026.").await;
    scan(&fx, std::slice::from_ref(&october), month_end()).await;
    let november = bill(&fx, "Invoice #A123 amount due £25 by 30 November 2026.").await;
    scan(&fx, std::slice::from_ref(&november), month_end()).await;
    let rows = all_rows(&fx).await;
    assert_eq!(rows.len(), 2, "{rows:#?}");
    assert_eq!(rows[0].source_message_id.as_ref(), Some(&october));
    assert_eq!(due_day(&rows[0]), "2026-10-31");
    assert_eq!(rows[1].source_message_id.as_ref(), Some(&november));
    assert_eq!(due_day(&rows[1]), "2026-11-30");
    assert!(rows
        .iter()
        .all(|row| row.state == mxr_store::TodoState::Open));
}

#[tokio::test]
async fn one_email_with_two_bills_a_month_apart_makes_two_to_dos_on_every_scan() {
    let fx = Fixture::new().await;
    let text = "Invoice #A123 amount due £25 by 31 October 2026.\n\nInvoice #A123 amount due £25 by 30 November 2026.";
    let id = bill(&fx, text).await;
    scan(&fx, std::slice::from_ref(&id), month_end()).await;
    let first = all_rows(&fx).await;
    let days: Vec<_> = first.iter().map(due_day).collect();
    assert_eq!(days, ["2026-10-31", "2026-11-30"], "{first:#?}");
    // One item identity a month apart: the case where a rescan could take
    // the sibling's row for its own.
    let identity =
        |row: &mxr_store::TodoRecord| row.dedup_key.rsplit_once('|').unwrap().0.to_string();
    assert_eq!(identity(&first[0]), identity(&first[1]));

    rewrite(&fx, &id, &format!("{text}\n\nThank you.")).await;
    let again = all_rows(&fx).await;
    let ids =
        |rows: &[mxr_store::TodoRecord]| rows.iter().map(|r| r.id.clone()).collect::<Vec<_>>();
    assert_eq!(ids(&again), ids(&first));
    assert_eq!(again.iter().map(due_day).collect::<Vec<_>>(), days);
    assert!(again
        .iter()
        .all(|row| row.state == mxr_store::TodoState::Open));
}
