use crate::test_fixtures::*;
use mxr_core::id::*;
use mxr_core::types::*;

async fn insert_message(
    store: &crate::Store,
    account: &AccountId,
    provider_id: &str,
    from: &str,
    direction: Option<MessageDirection>,
) -> MessageId {
    let mut envelope = TestEnvelopeBuilder::new()
        .account_id(account.clone())
        .build();
    envelope.provider_id = provider_id.into();
    envelope.from = Address {
        name: None,
        email: from.into(),
    };
    match direction {
        Some(direction) => store
            .upsert_envelope_with_direction(&envelope, direction)
            .await
            .unwrap(),
        None => store.upsert_envelope(&envelope).await.unwrap(),
    };
    envelope.id
}

async fn direction_of(store: &crate::Store, id: &MessageId) -> String {
    sqlx::query_scalar("SELECT direction FROM messages WHERE id = ?")
        .bind(id.as_str())
        .fetch_one(store.reader())
        .await
        .unwrap()
}

#[tokio::test]
async fn reclassify_account_directions_follows_the_owned_set() {
    let store = crate::Store::in_memory().await.unwrap();
    let account = test_account();
    let other = test_account();
    store.insert_account(&account).await.unwrap();
    store.insert_account(&other).await.unwrap();
    let owned = OwnedAddresses::new(Some("me@example.com"), ["alias@example.com"]);

    let alias_inbound = insert_message(
        &store,
        &account.id,
        "alias-in",
        "alias@example.com",
        Some(MessageDirection::Inbound),
    )
    .await;
    let me_cased_inbound = insert_message(
        &store,
        &account.id,
        "me-in",
        "ME@Example.com",
        Some(MessageDirection::Inbound),
    )
    .await;
    let removed_outbound = insert_message(
        &store,
        &account.id,
        "old-out",
        "old@example.com",
        Some(MessageDirection::Outbound),
    )
    .await;
    let bob_inbound = insert_message(
        &store,
        &account.id,
        "bob-in",
        "bob@example.com",
        Some(MessageDirection::Inbound),
    )
    .await;
    let me_outbound = insert_message(
        &store,
        &account.id,
        "me-out",
        "me@example.com",
        Some(MessageDirection::Outbound),
    )
    .await;
    let unknown_alias = insert_message(
        &store,
        &account.id,
        "alias-unknown",
        "alias@example.com",
        None,
    )
    .await;
    let other_account_alias = insert_message(
        &store,
        &other.id,
        "other-alias-in",
        "alias@example.com",
        Some(MessageDirection::Inbound),
    )
    .await;

    let changed = store
        .reclassify_account_directions(&account.id, &owned, true)
        .await
        .unwrap();

    assert_eq!(changed.to_outbound, 2);
    assert_eq!(changed.to_inbound, 1);
    assert_eq!(direction_of(&store, &alias_inbound).await, "outbound");
    assert_eq!(direction_of(&store, &me_cased_inbound).await, "outbound");
    assert_eq!(direction_of(&store, &removed_outbound).await, "inbound");
    assert_eq!(direction_of(&store, &bob_inbound).await, "inbound");
    assert_eq!(direction_of(&store, &me_outbound).await, "outbound");
    assert_eq!(direction_of(&store, &unknown_alias).await, "unknown");
    assert_eq!(direction_of(&store, &other_account_alias).await, "inbound");

    let again = store
        .reclassify_account_directions(&account.id, &owned, true)
        .await
        .unwrap();
    assert_eq!((again.to_outbound, again.to_inbound), (0, 0));
}

#[tokio::test]
async fn reclassify_account_directions_moves_reply_pairs_with_the_reply() {
    let store = crate::Store::in_memory().await.unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();

    let mut parent = TestEnvelopeBuilder::new()
        .account_id(account.id.clone())
        .build();
    parent.provider_id = "parent".into();
    parent.message_id_header = Some("<parent@example.com>".into());
    parent.from = Address {
        name: None,
        email: "bob@example.com".into(),
    };
    parent.to = vec![Address {
        name: None,
        email: "me@example.com".into(),
    }];
    store
        .upsert_envelope_with_direction(&parent, MessageDirection::Inbound)
        .await
        .unwrap();

    // Sent from the alias before it was registered, so it was stored Inbound
    // and paired as they_replied with the primary as counterparty.
    let mut reply = TestEnvelopeBuilder::new()
        .account_id(account.id.clone())
        .build();
    reply.provider_id = "reply".into();
    reply.from = Address {
        name: None,
        email: "alias@example.com".into(),
    };
    reply.in_reply_to = Some("<parent@example.com>".into());
    store
        .upsert_envelope_with_direction(&reply, MessageDirection::Inbound)
        .await
        .unwrap();
    assert!(store.try_create_reply_pair(&reply).await.unwrap());

    sqlx::query(
        "INSERT INTO reply_pair_pending (reply_message_id, in_reply_to_header, account_id, created_at)
         VALUES (?, '<parent@example.com>', ?, 0)",
    )
    .bind(reply.id.as_str())
    .bind(account.id.as_str())
    .execute(store.writer())
    .await
    .unwrap();

    let owned = OwnedAddresses::new(Some("me@example.com"), ["alias@example.com"]);
    store
        .reclassify_account_directions(&account.id, &owned, true)
        .await
        .unwrap();

    let pending: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM reply_pair_pending WHERE reply_message_id = ?")
            .bind(reply.id.as_str())
            .fetch_one(store.reader())
            .await
            .unwrap();
    assert_eq!(pending, 0, "a resolved reply leaves the pending queue");

    let (direction, counterparty): (String, String) = sqlx::query_as(
        "SELECT direction, counterparty_email FROM reply_pairs WHERE reply_message_id = ?",
    )
    .bind(reply.id.as_str())
    .fetch_one(store.reader())
    .await
    .unwrap();
    assert_eq!(direction, "i_replied");
    assert_eq!(counterparty, "bob@example.com");

    let (latency, parent_at, replied_at): (Option<i64>, i64, i64) = sqlx::query_as(
        "SELECT business_hours_latency_seconds, parent_received_at, replied_at
         FROM reply_pairs WHERE reply_message_id = ?",
    )
    .bind(reply.id.as_str())
    .fetch_one(store.reader())
    .await
    .unwrap();
    assert_eq!(
        latency,
        Some(crate::reply_pairs::compute_business_hours_seconds(
            parent_at, replied_at
        )),
        "a rebuilt pair carries its business-hours latency"
    );
}

#[tokio::test]
async fn reclassify_account_directions_leaves_arrival_history_alone() {
    let store = crate::Store::in_memory().await.unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();
    let alias_mail = insert_message(
        &store,
        &account.id,
        "alias-in",
        "alias@example.com",
        Some(MessageDirection::Inbound),
    )
    .await;
    let arrivals_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM arrivals")
        .fetch_one(store.reader())
        .await
        .unwrap();
    assert_eq!(arrivals_before, 1);

    let owned = OwnedAddresses::new(Some("me@example.com"), ["alias@example.com"]);
    store
        .reclassify_account_directions(&account.id, &owned, true)
        .await
        .unwrap();

    assert_eq!(direction_of(&store, &alias_mail).await, "outbound");
    let arrivals_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM arrivals")
        .fetch_one(store.reader())
        .await
        .unwrap();
    assert_eq!(arrivals_after, arrivals_before);
}

#[tokio::test]
async fn dry_run_reports_the_counts_the_real_run_applies_and_writes_nothing() {
    let store = crate::Store::in_memory().await.unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();
    let alias_mail = insert_message(
        &store,
        &account.id,
        "alias-in",
        "alias@example.com",
        Some(MessageDirection::Inbound),
    )
    .await;
    let owned = OwnedAddresses::new(Some("me@example.com"), ["alias@example.com"]);

    let previewed = store
        .reclassify_account_directions(&account.id, &owned, false)
        .await
        .unwrap();

    assert_eq!(previewed.to_outbound, 1);
    assert_eq!(previewed.to_inbound, 0);
    assert_eq!(direction_of(&store, &alias_mail).await, "inbound");
    let arrivals: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM arrivals")
        .fetch_one(store.reader())
        .await
        .unwrap();
    assert_eq!(arrivals, 1);

    let applied = store
        .reclassify_account_directions(&account.id, &owned, true)
        .await
        .unwrap();
    assert_eq!(applied, previewed);
    assert_eq!(direction_of(&store, &alias_mail).await, "outbound");
}

#[tokio::test]
async fn a_late_stale_pair_write_is_rebuilt_by_the_next_reconcile() {
    // The repair lands, then a sync write puts the pair back with the
    // direction it computed before the repair. The message is already correct,
    // so only the account-wide reconcile can fix the pair.
    let store = crate::Store::in_memory().await.unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();
    let mut parent = TestEnvelopeBuilder::new()
        .account_id(account.id.clone())
        .build();
    parent.provider_id = "parent".into();
    parent.message_id_header = Some("<parent@example.com>".into());
    parent.from = Address {
        name: None,
        email: "bob@example.com".into(),
    };
    parent.to = vec![Address {
        name: None,
        email: "me@example.com".into(),
    }];
    store
        .upsert_envelope_with_direction(&parent, MessageDirection::Inbound)
        .await
        .unwrap();
    let mut reply = TestEnvelopeBuilder::new()
        .account_id(account.id.clone())
        .build();
    reply.provider_id = "reply".into();
    reply.from = Address {
        name: None,
        email: "alias@example.com".into(),
    };
    reply.in_reply_to = Some("<parent@example.com>".into());
    store
        .upsert_envelope_with_direction(&reply, MessageDirection::Inbound)
        .await
        .unwrap();
    store.try_create_reply_pair(&reply).await.unwrap();

    let owned = OwnedAddresses::new(Some("me@example.com"), ["alias@example.com"]);
    store
        .reclassify_account_directions(&account.id, &owned, true)
        .await
        .unwrap();
    sqlx::query(
        "INSERT OR REPLACE INTO reply_pairs (
            reply_message_id, parent_message_id, account_id, counterparty_email, direction,
            parent_received_at, replied_at, latency_seconds, business_hours_latency_seconds, created_at)
         SELECT reply_message_id, parent_message_id, account_id, 'me@example.com', 'they_replied',
            parent_received_at, replied_at, 0, NULL, created_at
         FROM reply_pairs WHERE reply_message_id = ?",
    )
    .bind(reply.id.as_str())
    .execute(store.writer())
    .await
    .unwrap();

    store
        .reclassify_account_directions(&account.id, &owned, true)
        .await
        .unwrap();

    let (direction, counterparty, latency, business, parent_at, replied_at): (
        String,
        String,
        i64,
        Option<i64>,
        i64,
        i64,
    ) = sqlx::query_as(
        "SELECT direction, counterparty_email, latency_seconds, business_hours_latency_seconds,
                parent_received_at, replied_at
         FROM reply_pairs WHERE reply_message_id = ?",
    )
    .bind(reply.id.as_str())
    .fetch_one(store.reader())
    .await
    .unwrap();
    assert_eq!(direction, "i_replied");
    assert_eq!(counterparty, "bob@example.com");
    assert_eq!(latency, (replied_at - parent_at).max(0));
    assert_eq!(
        business,
        Some(crate::reply_pairs::compute_business_hours_seconds(
            parent_at, replied_at
        ))
    );
}

#[tokio::test]
async fn the_reply_pair_lookup_for_one_reply_uses_the_message_index() {
    // A per-flip reseat must not scan every message. The plan is checked
    // against the query the reseat runs.
    let store = crate::Store::in_memory().await.unwrap();
    // A constant from this crate, not input: the wrapper only marks it safe.
    let plan_sql = sqlx::AssertSqlSafe(format!(
        "EXPLAIN QUERY PLAN {}",
        crate::reply_pairs::PAIR_FOR_REPLY_SQL
    ));
    let plan: Vec<(i64, i64, i64, String)> = sqlx::query_as(plan_sql)
        .bind("some-reply")
        .fetch_all(store.reader())
        .await
        .unwrap();
    let details: Vec<&str> = plan.iter().map(|row| row.3.as_str()).collect();
    assert!(
        details.iter().any(|detail| detail.contains("SEARCH reply")),
        "plan does not search the reply by key: {details:?}"
    );
    assert!(
        !details
            .iter()
            .any(|detail| detail.starts_with("SCAN reply")),
        "plan scans every message for one reply: {details:?}"
    );
}
