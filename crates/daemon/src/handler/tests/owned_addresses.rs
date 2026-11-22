use crate::state::AppState;

#[tokio::test]
async fn dry_run_remove_reports_the_real_counts_and_changes_nothing() {
    use mxr_core::types::MessageDirection;
    use mxr_protocol::ResponseData;

    let state = AppState::in_memory().await.unwrap();
    let account_id = state.default_account_id();
    crate::handler::diagnostics_impl::add_account_address(
        &state,
        &account_id,
        "alias@example.com",
        false,
        false,
    )
    .await
    .unwrap();
    let sent_from_alias = store_mail_from(
        &state,
        &account_id,
        "alias@example.com",
        MessageDirection::Outbound,
        "alias-out-dry",
    )
    .await;
    let arrivals_before = arrival_rows(&state).await;
    let mut events = state.event_tx.subscribe();

    let preview = crate::handler::diagnostics_impl::remove_account_address(
        &state,
        &account_id,
        "alias@example.com",
        true,
    )
    .await
    .unwrap();

    assert!(matches!(
        preview,
        ResponseData::AccountAddressChanged {
            to_outbound: 0,
            to_inbound: 1
        }
    ));
    assert!(state
        .store
        .list_account_addresses(&account_id)
        .await
        .unwrap()
        .iter()
        .any(|address| address.email == "alias@example.com"));
    assert_eq!(direction_of(&state, &sent_from_alias).await, "outbound");
    assert_eq!(arrival_rows(&state).await, arrivals_before);
    assert!(events.try_recv().is_err());

    let applied = crate::handler::diagnostics_impl::remove_account_address(
        &state,
        &account_id,
        "alias@example.com",
        false,
    )
    .await
    .unwrap();
    assert_eq!(
        serde_json::to_value(&preview).unwrap(),
        serde_json::to_value(&applied).unwrap()
    );
    assert_eq!(direction_of(&state, &sent_from_alias).await, "inbound");
}

#[tokio::test]
async fn alias_registered_through_the_handler_counts_in_every_owned_reader_at_once() {
    let state = AppState::in_memory().await.unwrap();
    let account_id = state.default_account_id();
    let alias = "Alias@Example.com";
    assert!(!crate::handler::desk::self_matcher(&state, &account_id)
        .await
        .unwrap()(alias));
    assert!(
        !crate::handler::thread_context::owned_addresses(&state, &account_id)
            .await
            .unwrap()
            .contains("alias@example.com")
    );

    crate::handler::diagnostics_impl::add_account_address(&state, &account_id, alias, false, false)
        .await
        .unwrap();

    assert!(crate::handler::desk::self_matcher(&state, &account_id)
        .await
        .unwrap()("alias@example.com"));
    assert!(
        crate::handler::thread_context::owned_addresses(&state, &account_id)
            .await
            .unwrap()
            .contains("alias@example.com")
    );
    assert!(state
        .owned_addresses(&account_id)
        .await
        .unwrap()
        .contains("  ALIAS@example.com "));
}

#[tokio::test]
async fn removing_the_primary_through_the_handler_is_refused_and_keeps_it_owned() {
    let state = AppState::in_memory().await.unwrap();
    let account_id = state.default_account_id();
    let primary = state
        .store
        .get_account(&account_id)
        .await
        .unwrap()
        .unwrap()
        .email;

    let result = crate::handler::diagnostics_impl::remove_account_address(
        &state,
        &account_id,
        &primary,
        false,
    )
    .await;
    assert!(result.is_err());
    assert!(state
        .owned_addresses(&account_id)
        .await
        .unwrap()
        .contains(&primary));
}

async fn store_mail_from(
    state: &AppState,
    account_id: &mxr_core::AccountId,
    from: &str,
    direction: mxr_core::types::MessageDirection,
    provider_id: &str,
) -> mxr_core::MessageId {
    let envelope = crate::test_fixtures::TestEnvelopeBuilder::new()
        .account_id(account_id.clone())
        .provider_id(provider_id)
        .sender_address("Alias", from)
        .build();
    state
        .store
        .upsert_envelope_with_direction(&envelope, direction)
        .await
        .unwrap();
    envelope.id
}

async fn direction_of(state: &AppState, id: &mxr_core::MessageId) -> String {
    sqlx::query_scalar("SELECT direction FROM messages WHERE id = ?")
        .bind(id.as_str())
        .fetch_one(state.store.reader())
        .await
        .unwrap()
}

#[tokio::test]
async fn adding_an_alias_reclassifies_stored_mail_from_it_and_reports_the_count() {
    use mxr_core::types::MessageDirection;
    use mxr_protocol::ResponseData;

    let state = AppState::in_memory().await.unwrap();
    let account_id = state.default_account_id();
    let sent_before_registering = store_mail_from(
        &state,
        &account_id,
        "Alias@Example.com",
        MessageDirection::Inbound,
        "alias-before",
    )
    .await;
    let from_someone_else = store_mail_from(
        &state,
        &account_id,
        "bob@example.com",
        MessageDirection::Inbound,
        "bob-before",
    )
    .await;

    let response = crate::handler::diagnostics_impl::add_account_address(
        &state,
        &account_id,
        "alias@example.com",
        false,
        false,
    )
    .await
    .unwrap();

    assert!(matches!(
        response,
        ResponseData::AccountAddressChanged {
            to_outbound: 1,
            to_inbound: 0
        }
    ));
    assert_eq!(
        direction_of(&state, &sent_before_registering).await,
        "outbound"
    );
    assert_eq!(direction_of(&state, &from_someone_else).await, "inbound");
}

async fn arrival_rows(state: &AppState) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM arrivals")
        .fetch_one(state.store.reader())
        .await
        .unwrap()
}

#[tokio::test]
async fn dry_run_add_reports_the_real_counts_and_changes_nothing() {
    use mxr_core::types::MessageDirection;
    use mxr_protocol::ResponseData;

    let state = AppState::in_memory().await.unwrap();
    let account_id = state.default_account_id();
    let alias_mail = store_mail_from(
        &state,
        &account_id,
        "alias@example.com",
        MessageDirection::Inbound,
        "alias-dry",
    )
    .await;
    let arrivals_before = arrival_rows(&state).await;
    let mut events = state.event_tx.subscribe();

    let preview = crate::handler::diagnostics_impl::add_account_address(
        &state,
        &account_id,
        "alias@example.com",
        false,
        true,
    )
    .await
    .unwrap();

    assert!(matches!(
        preview,
        ResponseData::AccountAddressChanged {
            to_outbound: 1,
            to_inbound: 0
        }
    ));
    assert!(!state
        .store
        .list_account_addresses(&account_id)
        .await
        .unwrap()
        .iter()
        .any(|address| address.email == "alias@example.com"));
    assert_eq!(direction_of(&state, &alias_mail).await, "inbound");
    assert_eq!(arrival_rows(&state).await, arrivals_before);
    assert!(events.try_recv().is_err());

    let applied = crate::handler::diagnostics_impl::add_account_address(
        &state,
        &account_id,
        "alias@example.com",
        false,
        false,
    )
    .await
    .unwrap();
    assert!(matches!(
        applied,
        ResponseData::AccountAddressChanged { .. }
    ));
    assert_eq!(
        serde_json::to_value(&preview).unwrap(),
        serde_json::to_value(&applied).unwrap()
    );
    assert_eq!(direction_of(&state, &alias_mail).await, "outbound");
}
