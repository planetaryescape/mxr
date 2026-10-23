use crate::test_fixtures::*;
use mxr_core::id::*;
use mxr_core::types::*;

async fn insert_outbound_from(
    store: &crate::Store,
    account: &AccountId,
    provider_id: &str,
    from: &str,
) {
    let mut envelope = TestEnvelopeBuilder::new()
        .account_id(account.clone())
        .build();
    envelope.provider_id = provider_id.into();
    envelope.from = Address {
        name: None,
        email: from.into(),
    };
    envelope.to = vec![Address {
        name: None,
        email: "bob@example.com".into(),
    }];
    store
        .upsert_envelope_with_direction(&envelope, MessageDirection::Outbound)
        .await
        .unwrap();
}

async fn bob_outbound_total(store: &crate::Store, account: &AccountId) -> i64 {
    sqlx::query_scalar(
        "SELECT total_outbound FROM contacts WHERE account_id = ? AND email = 'bob@example.com'",
    )
    .bind(account.as_str())
    .fetch_one(store.reader())
    .await
    .unwrap()
}

#[tokio::test]
async fn refresh_zeroes_a_contact_whose_mail_changed_direction() {
    // Bob is named by mail that is no longer outbound. The aggregate stops
    // producing him, the prune keeps him because the mail still names him,
    // and his outbound total must not survive the refresh.
    let store = crate::Store::in_memory().await.unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();
    insert_outbound_from(&store, &account.id, "alias-sent", "alias@example.com").await;

    store.refresh_contacts().await.unwrap();
    assert_eq!(bob_outbound_total(&store, &account.id).await, 1);

    let owned = OwnedAddresses::new(Some("me@example.com"), []);
    store
        .reclassify_account_directions(&account.id, &owned, true)
        .await
        .unwrap();
    store.refresh_contacts().await.unwrap();
    store.refresh_contacts().await.unwrap();

    assert_eq!(bob_outbound_total(&store, &account.id).await, 0);
}

#[tokio::test]
async fn zeroing_spares_a_contact_whose_mail_arrived_after_the_snapshot() {
    // Bob was nominated by the snapshot, and then new outbound mail to him
    // arrives before the reset runs. The reset must not clear his counters.
    let store = crate::Store::in_memory().await.unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();
    insert_outbound_from(&store, &account.id, "alias-sent", "alias@example.com").await;
    store.refresh_contacts().await.unwrap();
    let owned = OwnedAddresses::new(Some("me@example.com"), []);
    store
        .reclassify_account_directions(&account.id, &owned, true)
        .await
        .unwrap();
    insert_outbound_from(&store, &account.id, "new-sent", "me@example.com").await;

    store
        .zero_direction_counters(&[(
            account.id.as_str(),
            "bob@example.com".to_string(),
        )])
        .await
        .unwrap();

    assert_eq!(bob_outbound_total(&store, &account.id).await, 1);
}
