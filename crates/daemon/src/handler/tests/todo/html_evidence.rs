//! Native account trust, structured provenance and cached-source invalidation.
use super::*;

#[tokio::test]
async fn structured_fields_retain_exact_authored_quote_and_revoke_with_account_trust() {
    // The rescan below moves the due date from now+2 to now+3. On 26 Oct
    // both fall in one month; on 29 Oct the move crosses into November,
    // which the due month in a bill's key once turned into a second row.
    for now in [
        Utc.with_ymd_and_hms(2026, 10, 26, 12, 0, 0).unwrap(),
        Utc.with_ymd_and_hms(2026, 10, 29, 12, 0, 0).unwrap(),
    ] {
        structured_fields_retain_quote_and_revoke_at(now).await;
    }
}

async fn structured_fields_retain_quote_and_revoke_at(now: DateTime<Utc>) {
    let fx = Fixture::new().await;
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

#[tokio::test]
async fn receiver_revocation_waits_for_preparation_then_retires_only_its_authenticated_projection()
{
    use crate::handler::assessments;
    use std::sync::Arc;
    use tokio::sync::Semaphore;

    let fx = Fixture::new().await;
    let now = Utc::now();
    configure_receiver(&fx).await;
    let mut bill = Mail::new(
        ("Council", "tax@camden.gov.uk"),
        "Council bill",
        "Your council tax payment is due soon.",
        now,
    );
    bill.auth = Some(CAMDEN_AUTH);
    bill.html = Some(invoice_html(
        now + Duration::days(2),
        "https://camden.gov.uk/pay",
    ));
    let id = put(&fx, bill).await;
    let source = fx.state.store.get_envelope(&id).await.unwrap().unwrap();
    let mut other_account = crate::test_fixtures::test_account();
    other_account.email = super::super::desk::ME.into();
    fx.state.store.insert_account(&other_account).await.unwrap();
    let mut other = source.clone();
    other.id = MessageId::new();
    other.thread_id = ThreadId::new();
    other.account_id = other_account.id.clone();
    other.provider_id = format!("other-{}", other.id);
    fx.state
        .store
        .upsert_envelope_with_direction(&other, MessageDirection::Inbound)
        .await
        .unwrap();
    let mut body = fx.state.store.get_body(&id).await.unwrap().unwrap();
    body.message_id = other.id.clone();
    fx.state.store.insert_body(&body).await.unwrap();
    let mut config = fx.state.config_snapshot();
    config
        .todo
        .trusted_authentication_services
        .insert(other_account.id.as_str(), vec!["mx.google.com".into()]);
    fx.state.set_config_for_test(config).await;
    assessments::reconcile_messages_at(
        &fx.state,
        &other_account.id,
        std::slice::from_ref(&other.id),
        now,
    )
    .await
    .unwrap();
    let other_before = fx
        .state
        .store
        .asks_for_messages(&other_account.id, std::slice::from_ref(&other.id))
        .await
        .unwrap();
    let other_rows = fx
        .state
        .store
        .list_todos_in_state(Some(&other_account.id), mxr_store::TodoState::Open, 10)
        .await
        .unwrap();
    assert_eq!(other_rows.len(), 1);

    let prepared = Arc::new(Semaphore::new(0));
    let release = Arc::new(Semaphore::new(0));
    let state = fx.state.clone();
    let account = fx.account.clone();
    let thread = source.thread_id.clone();
    let pause = prepared.clone();
    let resume = release.clone();
    let materialize = tokio::spawn(async move {
        assessments::materialize_threads(&state, &account, &[thread], || {
            let pause = pause.clone();
            let resume = resume.clone();
            async move {
                pause.add_permits(1);
                resume
                    .acquire()
                    .await
                    .expect("release prepared assessment")
                    .forget();
                Ok(())
            }
        })
        .await
        .unwrap()
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), prepared.acquire())
        .await
        .unwrap()
        .unwrap()
        .forget();
    assert!(
        fx.state.assessment_config_gate.try_write().is_err(),
        "materialization holds the config read gate"
    );
    let mut revoked = fx.state.config_snapshot();
    revoked
        .todo
        .trusted_authentication_services
        .remove(&fx.account.as_str());
    let writer_reached_gate = Arc::new(Semaphore::new(0));
    let reached = writer_reached_gate.clone();
    let observer: Arc<dyn Fn() + Send + Sync> = Arc::new(move || reached.add_permits(1));
    let state = fx.state.clone();
    let writer = tokio::spawn(async move {
        assessments::BEFORE_AUTHORITY_WRITE
            .scope(observer, state.set_config_for_test(revoked))
            .await;
    });
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        writer_reached_gate.acquire(),
    )
    .await
    .unwrap()
    .unwrap()
    .forget();
    assert!(
        !writer.is_finished(),
        "revocation must wait for the current preparation/publication"
    );
    assert!(fx
        .state
        .config_snapshot()
        .todo
        .trusted_authentication_services
        .contains_key(&fx.account.as_str()));
    release.add_permits(1);
    materialize.await.unwrap();
    writer.await.unwrap();
    assert!(
        fx.state
            .store
            .asks_for_messages(&fx.account, std::slice::from_ref(&id))
            .await
            .unwrap()
            .is_empty(),
        "the old authenticated answer is pending after revocation"
    );
    assert!(
        fx.state
            .store
            .list_todos_in_state(Some(&fx.account), mxr_store::TodoState::Open, 10)
            .await
            .unwrap()
            .is_empty(),
        "no stale authenticated projection is actionable"
    );
    assert_eq!(
        fx.state
            .store
            .asks_for_messages(&other_account.id, std::slice::from_ref(&other.id))
            .await
            .unwrap(),
        other_before
    );
    assert_eq!(
        fx.state
            .store
            .list_todos_in_state(Some(&other_account.id), mxr_store::TodoState::Open, 10)
            .await
            .unwrap(),
        other_rows
    );
    scan(&fx, std::slice::from_ref(&id), now).await;
    let current = fx
        .state
        .store
        .asks_for_messages(&fx.account, std::slice::from_ref(&id))
        .await
        .unwrap()
        .remove(&id)
        .unwrap();
    assert!(current.html.unwrap().alignment.is_none());
    assert!(fx
        .state
        .store
        .list_todos_in_state(Some(&fx.account), mxr_store::TodoState::Open, 10)
        .await
        .unwrap()
        .is_empty());
}
