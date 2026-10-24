//! Native account trust, structured provenance and cached-source invalidation.
use super::*;

#[tokio::test]
async fn structured_fields_retain_exact_authored_quote_and_revoke_with_account_trust() {
    let fx = Fixture::new().await;
    let now = Utc::now();
    configure_receiver(&fx).await;
    let quote = "Your council tax payment is due soon.";
    let mut bill = Mail::new(
        ("Council", "tax@camden.gov.uk"),
        "Council tax bill",
        quote,
        now,
    );
    bill.auth = Some(CAMDEN_AUTH);
    bill.html = Some(invoice_html(
        now + Duration::days(2),
        "https://www.camden.gov.uk/pay",
    ));
    let id = put(&fx, bill).await;
    scan(&fx, std::slice::from_ref(&id), now).await;
    let read = || async {
        fx.state
            .store
            .asks_for_messages(&fx.account, std::slice::from_ref(&id))
            .await
            .unwrap()
            .remove(&id)
            .unwrap()
    };
    let first = read().await;
    let html = first.html.as_ref().unwrap();
    let alignment = html.alignment.as_ref().unwrap();
    assert_eq!(alignment.authserv_id, "mx.google.com");
    assert_eq!(alignment.method, "dkim");
    assert_eq!(first.facts.zones[alignment.header_zone].text, CAMDEN_AUTH);
    let unit = first
        .units
        .iter()
        .find(|u| u.answer == mxr_core::asks::AskAnswer::AsksYou)
        .unwrap();
    assert_eq!(first.quote(unit), Some(quote));
    let tags = unit.tags.as_ref().unwrap();
    assert_eq!(first.facts.amounts[tags.amount.unwrap()].minor, 14200);
    let due = &first.facts.dates[tags.due.unwrap()];
    assert_eq!(due.zone, html.zone);
    assert!(first.facts.zones[due.zone].text[due.span.start..due.span.end].contains('-'));
    let row = fx
        .state
        .store
        .list_todos_in_state(Some(&fx.account), mxr_store::TodoState::Open, 100)
        .await
        .unwrap()
        .remove(0);
    assert_eq!(row.amount_minor, Some(14200));
    assert_eq!(row.due_at, Some(due.at));
    assert_eq!(row.reason, quote);
    let fields = mxr_todo::provenance::FieldSources::from_json(&row.field_sources);
    assert_eq!(
        fields.get("amount").unwrap().source,
        mxr_todo::provenance::FieldSource::Schema
    );
    assert_eq!(
        fields.get("due_at").unwrap().source,
        mxr_todo::provenance::FieldSource::Schema
    );
    assert!(row.field_sources.contains(&first.input_fingerprint));
    let mut body = fx.state.store.get_body(&id).await.unwrap().unwrap();
    body.text_html = Some(invoice_html(
        now + Duration::days(3),
        "https://www.camden.gov.uk/new-pay",
    ));
    fx.state.store.insert_body(&body).await.unwrap();
    assert!(
        fx.state
            .store
            .asks_for_messages(&fx.account, std::slice::from_ref(&id))
            .await
            .unwrap()
            .is_empty(),
        "HTML-only source changes invalidate the old generation"
    );
    scan(&fx, std::slice::from_ref(&id), now).await;
    let changed = read().await;
    assert_ne!(changed.input_fingerprint, first.input_fingerprint);
    let rows = fx
        .state
        .store
        .list_todos_in_state(Some(&fx.account), mxr_store::TodoState::Open, 100)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, row.id);
    assert_ne!(rows[0].due_at, row.due_at);
    let mut config = fx.state.config_snapshot();
    config.todo.trusted_authentication_services.clear();
    fx.state.set_config_for_test(config).await;
    assert!(fx
        .state
        .store
        .asks_for_messages(&fx.account, std::slice::from_ref(&id))
        .await
        .unwrap()
        .is_empty());
    scan(&fx, std::slice::from_ref(&id), now).await;
    assert!(read().await.html.as_ref().unwrap().alignment.is_none());
    assert!(fx
        .state
        .store
        .list_todos_in_state(Some(&fx.account), mxr_store::TodoState::Open, 100)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn unknown_receiver_forgery_failure_and_unaligned_results_never_supply_schema_fields() {
    for (configure, auth) in [
        (false, CAMDEN_AUTH),
        (true, "attacker.invalid; dkim=pass header.d=camden.gov.uk"),
        (true, "mx.google.com; dkim=fail header.d=camden.gov.uk"),
        (true, "mx.google.com; dkim=pass header.d=other.example"),
        (
            true,
            "mx.google.com; dkim=pass header.d=camden.gov.uk (broken",
        ),
    ] {
        let fx = Fixture::new().await;
        let now = Utc::now();
        if configure {
            configure_receiver(&fx).await;
        }
        let mut bill = Mail::new(
            ("Council", "tax@camden.gov.uk"),
            "Council tax bill",
            "Your council tax payment is due soon.",
            now,
        );
        bill.auth = Some(auth);
        bill.html = Some(invoice_html(
            now + Duration::days(1),
            "https://camden.gov.uk/pay",
        ));
        let id = put(&fx, bill).await;
        scan(&fx, std::slice::from_ref(&id), now).await;
        assert!(
            fx.state
                .store
                .list_todos_in_state(Some(&fx.account), mxr_store::TodoState::Open, 100)
                .await
                .unwrap()
                .is_empty(),
            "{auth}"
        );
    }
}

#[tokio::test]
async fn receiver_trust_is_account_scoped_and_cannot_turn_an_offer_into_a_bill() {
    for wrong_account in [true, false] {
        let fx = Fixture::new().await;
        let now = Utc::now();
        if wrong_account {
            let mut config = fx.state.config_snapshot();
            config
                .todo
                .trusted_authentication_services
                .insert("another-account".into(), vec!["mx.google.com".into()]);
            fx.state.set_config_for_test(config).await;
        } else {
            configure_receiver(&fx).await;
        }
        let text = if wrong_account {
            "Your council tax payment is due soon."
        } else {
            "Apply for our optional payment offer today."
        };
        let mut bill = Mail::new(
            ("Council", "tax@camden.gov.uk"),
            "Payment information",
            text,
            now,
        );
        bill.auth = Some(CAMDEN_AUTH);
        bill.html = Some(invoice_html(
            now + Duration::days(1),
            "https://camden.gov.uk/pay",
        ));
        let id = put(&fx, bill).await;
        scan(&fx, std::slice::from_ref(&id), now).await;
        assert!(fx
            .state
            .store
            .list_todos_in_state(Some(&fx.account), mxr_store::TodoState::Open, 100)
            .await
            .unwrap()
            .is_empty());
    }
}
