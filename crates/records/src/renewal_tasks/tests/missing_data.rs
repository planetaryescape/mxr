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
