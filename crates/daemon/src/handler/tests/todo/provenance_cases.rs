//! Structured fields cannot overwrite authored dates or upgrade inferred certainty.
use super::*;

#[tokio::test]
async fn authenticated_schema_dates_must_agree_with_the_authored_bill_date() {
    for (offset, ambiguous) in [(0, false), (1, false), (0, true)] {
        let fx = Fixture::new().await;
        configure_receiver(&fx).await;
        let now = Utc::now();
        let due = now.date_naive() + Duration::days(3);
        let quote = if ambiguous {
            format!(
                "Your council tax payment is due on {} or {}.",
                due.format("%-d %B %Y"),
                (due + Duration::days(2)).format("%-d %B %Y")
            )
        } else {
            format!(
                "Your council tax payment is due on {}.",
                due.format("%-d %B %Y")
            )
        };
        let mut mail = Mail::new(
            ("Council", "tax@camden.gov.uk"),
            "Council tax bill",
            &quote,
            now,
        );
        mail.auth = Some(CAMDEN_AUTH);
        mail.html = Some(invoice_html(
            (due + Duration::days(offset))
                .and_hms_opt(12, 0, 0)
                .unwrap()
                .and_utc(),
            "https://camden.gov.uk/pay",
        ));
        let id = put(&fx, mail).await;
        scan(&fx, std::slice::from_ref(&id), now).await;
        let answer = fx
            .state
            .store
            .asks_for_messages(&fx.account, std::slice::from_ref(&id))
            .await
            .unwrap()
            .remove(&id)
            .unwrap();
        if ambiguous {
            assert_eq!(
                answer
                    .facts
                    .dates
                    .iter()
                    .filter(
                        |d| answer.facts.zones[d.zone].kind == mxr_core::asks::ZoneKind::Authored
                    )
                    .count(),
                2,
                "both alternatives belong to canonical facts"
            );
        }
        let rows = fx
            .state
            .store
            .list_todos_in_state(Some(&fx.account), mxr_store::TodoState::Open, 100)
            .await
            .unwrap();
        if offset != 0 || ambiguous {
            assert!(rows.is_empty(), "conflicting or ambiguous authored date must not become schema-authoritative: {answer:?} {rows:?}");
        } else {
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].reason, quote);
            assert_eq!(rows[0].due_at.unwrap().date_naive(), due);
        }
    }
}

#[tokio::test]
async fn an_inferred_ask_date_stays_unchecked_in_the_stored_task_and_why() {
    let fx = Fixture::new().await;
    let now = Utc::now();
    let quote = "Invoice amount due £20 by 12 November.";
    let id = put(
        &fx,
        Mail::new(("Accounts", "accounts@example.com"), "Invoice", quote, now),
    )
    .await;
    scan(&fx, std::slice::from_ref(&id), now).await;
    let answer = fx
        .state
        .store
        .asks_for_messages(&fx.account, std::slice::from_ref(&id))
        .await
        .unwrap()
        .remove(&id)
        .unwrap();
    let unit = answer
        .units
        .iter()
        .find(|u| u.answer == mxr_core::asks::AskAnswer::AsksYou)
        .unwrap();
    let due = &answer.facts.dates[unit.tags.as_ref().unwrap().due.unwrap()];
    assert!(!due.checked, "yearless date is inferred");
    let row = fx
        .state
        .store
        .list_todos_in_state(Some(&fx.account), mxr_store::TodoState::Open, 100)
        .await
        .unwrap()
        .remove(0);
    let fields = mxr_todo::provenance::FieldSources::from_json(&row.field_sources);
    assert_eq!(fields.get("due_at").unwrap().checked, due.checked);
    assert_eq!(row.due_at, Some(due.at));
    let why = todos::get_todo(&fx.state, &row.id).await.unwrap();
    let json = serde_json::to_value(why).unwrap();
    assert!(json.to_string().contains("\"checked\":false"), "{json}");
}
