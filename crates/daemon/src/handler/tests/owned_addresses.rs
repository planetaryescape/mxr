//! One owned-address set for every daemon feature: the primary plus every
//! registered alias, and the same answer from each caller.

use crate::state::AppState;
use mxr_core::types::AccountAddressLookup;

#[tokio::test]
async fn owned_set_keeps_the_primary_when_its_address_row_is_missing() {
    // Before `remove_account_address` refused the primary, removing it left the
    // account without a primary row. The accounts row still names the mailbox,
    // so the owned set must keep it.
    let state = AppState::in_memory().await.unwrap();
    let account_id = state.default_account_id();
    let primary = state
        .store
        .get_account(&account_id)
        .await
        .unwrap()
        .unwrap()
        .email;
    sqlx::query("DELETE FROM account_addresses WHERE account_id = ? AND is_primary = 1")
        .bind(account_id.as_str())
        .execute(state.store.writer())
        .await
        .unwrap();

    let owned = crate::handler::thread_context::owned_addresses(&state, &account_id)
        .await
        .unwrap();
    assert!(owned.contains(&primary.to_ascii_lowercase()));
}

#[tokio::test]
async fn alias_registered_through_the_handler_counts_in_every_owned_reader_at_once() {
    // The CLI path is `Request::AddAccountAddress`, which refreshes the
    // registry. Readers that used to go to the store and readers that read the
    // registry must both see the alias straight away, with no restart.
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

    crate::handler::diagnostics_impl::add_account_address(&state, &account_id, alias, false)
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
async fn draft_voice_lists_the_primary_first_and_then_the_aliases() {
    let state = AppState::in_memory().await.unwrap();
    let account_id = state.default_account_id();
    let primary = state
        .store
        .get_account(&account_id)
        .await
        .unwrap()
        .unwrap()
        .email;
    crate::handler::diagnostics_impl::add_account_address(
        &state,
        &account_id,
        "zeta@example.com",
        false,
    )
    .await
    .unwrap();
    crate::handler::diagnostics_impl::add_account_address(
        &state,
        &account_id,
        "alpha@example.com",
        false,
    )
    .await
    .unwrap();

    let me = crate::handler::draft_voice::me(&state, &account_id).await;
    assert_eq!(me.emails.first(), Some(&primary));
    assert_eq!(
        me.emails[1..],
        [
            "alpha@example.com".to_string(),
            "zeta@example.com".to_string()
        ]
    );
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

    let result =
        crate::handler::diagnostics_impl::remove_account_address(&state, &account_id, &primary)
            .await;
    assert!(result.is_err());
    assert!(state
        .owned_addresses(&account_id)
        .await
        .unwrap()
        .contains(&primary));
}

#[tokio::test]
async fn unicode_spellings_of_an_alias_both_validate_as_from() {
    // The From check folds ASCII only, so both spellings must survive the set
    // and each must resolve to itself.
    let state = AppState::in_memory().await.unwrap();
    let account_id = state.default_account_id();
    for email in ["user@BÜCHER.example", "user@bücher.example"] {
        crate::handler::diagnostics_impl::add_account_address(&state, &account_id, email, false)
            .await
            .unwrap();
    }
    for email in ["user@BÜCHER.example", "user@bücher.example"] {
        let requested = mxr_core::types::Address {
            name: None,
            email: email.to_string(),
        };
        let from =
            crate::handler::mutations::resolve_from_address(&state, &account_id, Some(&requested))
                .await
                .unwrap();
        assert_eq!(from.email, email);
    }
}

#[tokio::test]
async fn sync_registry_keeps_the_configured_email_after_it_stops_being_primary() {
    // After set-primary and removal of the old configured address, sync and the
    // handlers must still agree that the configured address is the account's.
    let state = AppState::in_memory().await.unwrap();
    let account_id = state.default_account_id();
    let configured = state
        .store
        .get_account(&account_id)
        .await
        .unwrap()
        .unwrap()
        .email;
    crate::handler::diagnostics_impl::add_account_address(
        &state,
        &account_id,
        "alias@example.com",
        false,
    )
    .await
    .unwrap();
    crate::handler::diagnostics_impl::set_primary_account_address(
        &state,
        &account_id,
        "alias@example.com",
    )
    .await
    .unwrap();
    crate::handler::diagnostics_impl::remove_account_address(&state, &account_id, &configured)
        .await
        .unwrap();

    assert!(state
        .owned_addresses(&account_id)
        .await
        .unwrap()
        .contains(&configured));
    assert!(state
        .account_addresses
        .is_account_address(&account_id, &configured));
}

#[tokio::test]
async fn registry_refresh_includes_an_account_inserted_after_startup() {
    // Provider setup inserts configured accounts after the registry is first
    // built, so a refresh has to pick up accounts that appeared since.
    let state = AppState::in_memory().await.unwrap();
    let account = crate::test_fixtures::test_account_with_id(mxr_core::AccountId::new());
    state.store.insert_account(&account).await.unwrap();
    assert!(!state
        .account_addresses
        .is_account_address(&account.id, &account.email));

    state.refresh_account_addresses().await;

    assert!(state
        .account_addresses
        .is_account_address(&account.id, &account.email));
}

#[tokio::test]
async fn sync_registry_keeps_a_disabled_accounts_addresses() {
    // Analytics repair reclassifies stored mail through the registry, including
    // a disabled account's. Dropping its set would persist its own mail as
    // inbound, and re-enabling could not repair those rows.
    let state = AppState::in_memory().await.unwrap();
    let account_id = state.default_account_id();
    let primary = state
        .store
        .get_account(&account_id)
        .await
        .unwrap()
        .unwrap()
        .email;
    state
        .store
        .set_account_enabled(&account_id, false)
        .await
        .unwrap();

    state.refresh_account_addresses().await;

    assert!(state
        .account_addresses
        .is_account_address(&account_id, &primary));
}

#[tokio::test]
async fn adding_an_alias_tells_clients_that_cached_reply_contexts_may_be_stale() {
    use mxr_protocol::{DaemonEvent, IpcPayload};
    let state = AppState::in_memory().await.unwrap();
    let account_id = state.default_account_id();
    let mut events = state.event_tx.subscribe();

    crate::handler::diagnostics_impl::add_account_address(
        &state,
        &account_id,
        "alias@example.com",
        false,
    )
    .await
    .unwrap();

    let announced = std::iter::from_fn(|| events.try_recv().ok()).any(|message| {
        matches!(
            message.payload,
            IpcPayload::Event(DaemonEvent::AccountAddressesChanged { account_id: ref changed })
                if *changed == account_id
        )
    });
    assert!(
        announced,
        "address changes must reach every connected client"
    );
}
