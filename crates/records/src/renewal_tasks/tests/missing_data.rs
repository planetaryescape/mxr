use super::*;

#[tokio::test]
async fn missing_amount_currency_and_insufficient_history_are_honest() {
    let mut fx = Fx::new().await;
    let mut ids = Vec::new();
    for year in [2024, 2025] {
        ids.push(
            fx.mail(
                ("Acme", "billing@acme.example"),
                "Your Acme Premium receipt",
                &format!("Thanks for your payment.\nReceipt number: P-{year}"),
                None,
                at(year, 10, 20),
            )
            .await,
        );
    }
    pass::scan_messages(
        &fx.store,
        &PassConfig {
            now: at(2026, 10, 11),
            tz: Utc,
        },
        &ids[..1],
    )
    .await
    .expect("fixture");
    assert!(fx.plan(at(2026, 10, 11)).await.is_empty());
    pass::scan_messages(
        &fx.store,
        &PassConfig {
            now: at(2026, 10, 11),
            tz: Utc,
        },
        &ids[1..],
    )
    .await
    .expect("fixture");
    let candidates = fx.plan(at(2026, 10, 11)).await;
    assert_eq!(candidates.len(), 1);
    assert!(candidates[0].task.amount_minor.is_none());
    assert!(candidates[0].task.currency.is_none());
    assert!(FieldSources::from_json(&candidates[0].task.field_sources)
        .get("amount")
        .is_none());
}

#[tokio::test]
async fn records_without_source_mail_do_not_create_orphan_tasks() {
    let fx = Fx::new().await;
    for year in [2024, 2025] {
        let id = format!("source-less-{year}");
        let fields = [
            ("issuer", Some("Acme".to_string()), None),
            ("title", Some("Acme Premium receipt".to_string()), None),
            ("issued_at", None, Some(at(year, 10, 20).timestamp())),
        ]
        .into_iter()
        .map(
            |(field, value_text, value_int)| mxr_store::RecordFieldValue {
                field: field.into(),
                source_key: "user".into(),
                message_id: None,
                source: "user".into(),
                rank: 4,
                value_text,
                value_int,
                checked: true,
                evidence: None,
                observed_at: at(2026, 10, 11),
            },
        )
        .collect();
        fx.store
            .file_record(&mxr_store::RecordFiling {
                id: id.clone(),
                account_id: fx.account.clone(),
                dedup_key: id,
                kind: "receipt".into(),
                origin: "manual".into(),
                reason: "fixture".into(),
                rules_version: 1,
                links: Vec::new(),
                fields,
                now: at(2026, 10, 11),
            })
            .await
            .expect("fixture");
    }
    let detected = subscriptions::load(
        &fx.store,
        Some(std::slice::from_ref(&fx.account)),
        at(2026, 10, 11),
        &Utc,
    )
    .await
    .expect("fixture");
    assert_eq!(detected.detection.subscriptions.len(), 1);
    assert!(fx.plan(at(2026, 10, 11)).await.is_empty());
}
