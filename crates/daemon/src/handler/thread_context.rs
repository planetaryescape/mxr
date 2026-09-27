//! `GetThreadContext`: what the store already knows about a conversation,
//! with no model involved. The reader shows it before the messages, so its
//! reads run concurrently.

use super::relationship_profile::commitment_data;
use super::{HandlerError, HandlerResult};
use crate::state::AppState;
use mxr_core::id::{AccountId, ThreadId};
use mxr_core::types::{Envelope, MessageDirection, ResponseTimeDirection, UnsubscribeMethod};
use mxr_protocol::{
    CommitmentData, CommitmentDirectionData, OwedReplyHereData, ResponseData, ThreadContextData,
    ThreadCounterpartyData, ThreadPromiseData,
};
use std::collections::{BTreeSet, HashMap};

pub(super) async fn get_thread_context(state: &AppState, thread_id: &ThreadId) -> HandlerResult {
    Ok(ResponseData::ThreadContext {
        context: load_thread_context(state, thread_id).await?,
    })
}

pub(super) async fn load_thread_context(
    state: &AppState,
    thread_id: &ThreadId,
) -> Result<ThreadContextData, HandlerError> {
    let envelopes = state.store.get_thread_envelopes(thread_id).await?;
    let Some(first) = envelopes.first() else {
        return Err(format!("thread {thread_id} not found").into());
    };
    let owned = owned_addresses(state, &first.account_id).await?;
    build_thread_context(state, thread_id, &envelopes, &owned).await
}

/// The account's own addresses, lowercased.
pub(super) async fn owned_addresses(
    state: &AppState,
    account_id: &AccountId,
) -> Result<BTreeSet<String>, HandlerError> {
    Ok(state
        .store
        .list_account_addresses(account_id)
        .await?
        .into_iter()
        .map(|address| address.email.to_ascii_lowercase())
        .collect())
}

/// Facts for a thread whose envelopes (non-empty) and owned addresses the
/// caller already loaded.
pub(super) async fn build_thread_context(
    state: &AppState,
    thread_id: &ThreadId,
    envelopes: &[Envelope],
    owned: &BTreeSet<String>,
) -> Result<ThreadContextData, HandlerError> {
    let Some(first) = envelopes.first() else {
        return Err(format!("thread {thread_id} not found").into());
    };
    let account_id = first.account_id.clone();
    let (directions, commitments) = tokio::try_join!(
        state.store.thread_message_directions(thread_id),
        state
            .store
            .list_open_thread_commitments(&account_id, thread_id),
    )?;
    let directions: HashMap<_, _> = directions.into_iter().collect();
    let is_yours = |envelope: &Envelope| match directions.get(&envelope.id) {
        Some(MessageDirection::Outbound) => true,
        Some(MessageDirection::Inbound) => false,
        // Rows synced before the address table existed: fall back to the
        // owned addresses the direction column would have used.
        _ => owned.contains(&envelope.from.email.to_ascii_lowercase()),
    };

    let mut newest_first: Vec<&Envelope> = envelopes.iter().collect();
    newest_first.sort_by_key(|envelope| std::cmp::Reverse(envelope.date));

    let counterparty_address = newest_first.iter().find_map(|envelope| {
        if is_yours(envelope) {
            envelope
                .to
                .iter()
                .chain(&envelope.cc)
                .find(|address| !owned.contains(&address.email.to_ascii_lowercase()))
                .cloned()
        } else {
            Some(envelope.from.clone())
        }
    });

    let counterparty = match counterparty_address {
        Some(address) if !address.email.trim().is_empty() => {
            let email = address.email.trim().to_ascii_lowercase();
            // Every spelling of the address in this thread (From, To, Cc):
            // the store matches them exactly so its address indexes apply,
            // while reply pairs and From headers keep the original case.
            let mut variants: Vec<String> = envelopes
                .iter()
                .flat_map(|envelope| {
                    std::iter::once(&envelope.from)
                        .chain(&envelope.to)
                        .chain(&envelope.cc)
                })
                .map(|address| address.email.trim())
                .filter(|spelling| spelling.eq_ignore_ascii_case(&email))
                .map(str::to_string)
                .collect();
            variants.sort_unstable();
            variants.dedup();
            let median = |direction| {
                state
                    .store
                    .reply_latency_median(&account_id, direction, &email, &variants)
            };
            let (exchange, (your_samples, your_p50), (their_samples, their_p50)) = tokio::try_join!(
                state
                    .store
                    .counterparty_exchange(&account_id, &email, &variants, thread_id),
                median(ResponseTimeDirection::IReplied),
                median(ResponseTimeDirection::TheyReplied),
            )?;
            let bulk_sender = exchange.list_sender
                || envelopes.iter().any(|envelope| {
                    envelope.from.email.eq_ignore_ascii_case(&email)
                        && !matches!(envelope.unsubscribe, UnsubscribeMethod::None)
                });
            Some(ThreadCounterpartyData {
                email,
                display_name: address
                    .name
                    .map(|name| name.trim().to_string())
                    .filter(|name| !name.is_empty()),
                messages_from_them: exchange.from_them,
                messages_from_you: exchange.from_you,
                your_reply_p50_seconds: your_p50,
                your_reply_samples: your_samples,
                their_reply_p50_seconds: their_p50,
                their_reply_samples: their_samples,
                last_contact_elsewhere_at: exchange.last_elsewhere_at,
                bulk_sender,
            })
        }
        _ => None,
    };

    let owed_reply = newest_first.first().and_then(|newest| {
        let bulk = !matches!(newest.unsubscribe, UnsubscribeMethod::None)
            || counterparty.as_ref().is_some_and(|person| {
                person.bulk_sender && newest.from.email.eq_ignore_ascii_case(&person.email)
            });
        (!is_yours(newest) && !bulk).then(|| OwedReplyHereData {
            message_id: newest.id.clone(),
            since: newest.date,
        })
    });

    Ok(ThreadContextData {
        thread_id: thread_id.clone(),
        account_id,
        counterparty,
        owed_reply,
        promises: commitments
            .into_iter()
            .map(|record| {
                let commitment = commitment_data(record);
                ThreadPromiseData {
                    owner: promise_owner(&commitment, envelopes, owned),
                    commitment,
                }
            })
            .collect(),
    })
}

/// Who made a promise, named the way the thread names them. Theirs are owned
/// by `who_owes` (an address, or a name the extractor wrote), not by the
/// thread's main counterparty: in a group thread each keeps its own owner.
fn promise_owner(
    commitment: &CommitmentData,
    envelopes: &[Envelope],
    owned: &BTreeSet<String>,
) -> String {
    let who = commitment.who_owes.trim();
    let is_address = who.contains('@');
    if commitment.direction == CommitmentDirectionData::Yours
        || (is_address && owned.contains(&who.to_ascii_lowercase()))
    {
        return "you".into();
    }
    if !who.is_empty() && !is_address {
        return who.to_string();
    }
    let email = if is_address {
        who
    } else {
        commitment.email.as_str()
    };
    envelopes
        .iter()
        .flat_map(|envelope| {
            std::iter::once(&envelope.from)
                .chain(&envelope.to)
                .chain(&envelope.cc)
        })
        .find(|address| address.email.eq_ignore_ascii_case(email))
        .and_then(|address| address.name.as_deref())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(email)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::TestEnvelopeBuilder;
    use mxr_core::id::AccountId;
    use mxr_core::types::Address;
    use mxr_store::{CommitmentDirection, CommitmentStatus, ContactCommitmentRecord};

    fn at(days_ago: i64) -> chrono::DateTime<chrono::Utc> {
        chrono::Utc::now() - chrono::Duration::days(days_ago)
    }

    async fn put(
        state: &AppState,
        account_id: &AccountId,
        thread_id: &ThreadId,
        from: (&str, &str),
        to: &str,
        days_ago: i64,
        direction: MessageDirection,
    ) -> Envelope {
        let envelope = TestEnvelopeBuilder::new()
            .account_id(account_id.clone())
            .thread_id(thread_id.clone())
            .provider_id(format!("ctx-{from:?}-{days_ago}"))
            .sender_address(from.0, from.1)
            .to(vec![Address {
                name: None,
                email: to.into(),
            }])
            .date(at(days_ago))
            .build();
        state
            .store
            .upsert_envelope_with_direction(&envelope, direction)
            .await
            .unwrap();
        envelope
    }

    fn context(response: ResponseData) -> ThreadContextData {
        match response {
            ResponseData::ThreadContext { context } => context,
            other => panic!("unexpected response {other:?}"),
        }
    }

    #[tokio::test]
    async fn facts_name_the_counterparty_owed_reply_and_thread_commitments() {
        let state = AppState::in_memory().await.unwrap();
        let account_id = state.default_account_id();
        let earlier = ThreadId::new();
        let here = ThreadId::new();
        use MessageDirection::{Inbound, Outbound};
        put(
            &state,
            &account_id,
            &earlier,
            ("Maya Ortiz", "maya@example.com"),
            "me@example.com",
            40,
            Inbound,
        )
        .await;
        put(
            &state,
            &account_id,
            &earlier,
            ("Me", "me@example.com"),
            "maya@example.com",
            39,
            Outbound,
        )
        .await;
        let newest = put(
            &state,
            &account_id,
            &here,
            ("Maya Ortiz", "maya@example.com"),
            "me@example.com",
            2,
            Inbound,
        )
        .await;

        for (what, direction, thread) in [
            ("send the runbook link", CommitmentDirection::Yours, &here),
            ("share the dashboard", CommitmentDirection::Theirs, &here),
            ("unrelated", CommitmentDirection::Yours, &earlier),
        ] {
            state
                .store
                .upsert_contact_commitment(&ContactCommitmentRecord {
                    id: format!("c-{what}"),
                    account_id: account_id.clone(),
                    email: "maya@example.com".into(),
                    thread_id: thread.clone(),
                    direction,
                    status: CommitmentStatus::Open,
                    who_owes: "you".into(),
                    what: what.into(),
                    by_when: Some(at(-3)),
                    evidence_msg_id: newest.id.clone(),
                    extracted_at: at(1),
                    resolved_at: None,
                })
                .await
                .unwrap();
        }

        let facts = context(get_thread_context(&state, &here).await.unwrap());
        let person = facts.counterparty.expect("maya is the counterparty");
        assert_eq!(person.email, "maya@example.com");
        assert_eq!(person.display_name.as_deref(), Some("Maya Ortiz"));
        assert_eq!(person.messages_from_them, 2);
        assert_eq!(person.messages_from_you, 1);
        assert_eq!(
            person
                .last_contact_elsewhere_at
                .map(|when| (chrono::Utc::now() - when).num_days()),
            Some(39)
        );
        assert!(!person.bulk_sender);
        assert_eq!(
            facts.owed_reply.map(|owed| owed.message_id),
            Some(newest.id.clone())
        );
        let mut whats: Vec<_> = facts
            .promises
            .iter()
            .map(|promise| promise.commitment.what.as_str())
            .collect();
        whats.sort_unstable();
        assert_eq!(whats, vec!["send the runbook link", "share the dashboard"]);
    }

    #[tokio::test]
    async fn each_promise_in_a_group_thread_keeps_its_own_owner() {
        let state = AppState::in_memory().await.unwrap();
        let account_id = state.default_account_id();
        let here = ThreadId::new();
        use MessageDirection::Inbound;
        put(
            &state,
            &account_id,
            &here,
            ("Alice Park", "alice@example.com"),
            "me@example.com",
            3,
            Inbound,
        )
        .await;
        let newest = put(
            &state,
            &account_id,
            &here,
            ("Bob Stone", "bob@example.com"),
            "me@example.com",
            1,
            Inbound,
        )
        .await;
        for (id, who, email) in [
            ("a", "alice@example.com", "alice@example.com"),
            ("b", "bob@example.com", "bob@example.com"),
            ("c", "Carol", "carol@example.com"),
        ] {
            state
                .store
                .upsert_contact_commitment(&ContactCommitmentRecord {
                    id: id.into(),
                    account_id: account_id.clone(),
                    email: email.into(),
                    thread_id: here.clone(),
                    direction: CommitmentDirection::Theirs,
                    status: CommitmentStatus::Open,
                    who_owes: who.into(),
                    what: format!("task {id}"),
                    by_when: None,
                    evidence_msg_id: newest.id.clone(),
                    extracted_at: at(0),
                    resolved_at: None,
                })
                .await
                .unwrap();
        }

        let facts = context(get_thread_context(&state, &here).await.unwrap());
        assert_eq!(facts.counterparty.unwrap().email, "bob@example.com");
        let mut owners: Vec<_> = facts
            .promises
            .iter()
            .map(|promise| (promise.commitment.what.clone(), promise.owner.clone()))
            .collect();
        owners.sort();
        assert_eq!(
            owners,
            vec![
                ("task a".to_string(), "Alice Park".to_string()),
                ("task b".to_string(), "Bob Stone".to_string()),
                ("task c".to_string(), "Carol".to_string()),
            ]
        );
    }

    /// Reply pairs keep the address as the mail spelled it; a mixed-case
    /// sender still gets a usual reply time.
    #[tokio::test]
    async fn a_mixed_case_address_still_has_a_usual_reply_time() {
        let state = AppState::in_memory().await.unwrap();
        let account_id = state.default_account_id();
        let here = ThreadId::new();
        let mut asked = TestEnvelopeBuilder::new()
            .account_id(account_id.clone())
            .thread_id(here.clone())
            .provider_id("case-ask")
            .sender_address("Maya", "Maya@Example.com")
            .date(at(3))
            .build();
        asked.message_id_header = Some("<case-ask@x>".into());
        state
            .store
            .upsert_envelope_with_direction(&asked, MessageDirection::Inbound)
            .await
            .unwrap();
        let mut answered = TestEnvelopeBuilder::new()
            .account_id(account_id.clone())
            .thread_id(here.clone())
            .provider_id("case-answer")
            .sender_address("Me", "me@example.com")
            .recipient_address(Some("Maya"), "Maya@Example.com")
            .date(at(3) + chrono::Duration::hours(2))
            .build();
        answered.in_reply_to = Some("<case-ask@x>".into());
        state
            .store
            .upsert_envelope_with_direction(&answered, MessageDirection::Outbound)
            .await
            .unwrap();
        assert!(state
            .store
            .try_create_reply_pair(&answered, MessageDirection::Outbound)
            .await
            .unwrap());

        let facts = context(get_thread_context(&state, &here).await.unwrap());
        let person = facts.counterparty.unwrap();
        assert_eq!(person.email, "maya@example.com");
        assert_eq!(person.your_reply_samples, 1);
        assert_eq!(person.your_reply_p50_seconds, Some(2 * 3600));
    }

    #[tokio::test]
    async fn your_latest_message_means_no_owed_reply_and_the_recipient_is_the_counterparty() {
        let state = AppState::in_memory().await.unwrap();
        let account_id = state.default_account_id();
        let here = ThreadId::new();
        use MessageDirection::{Inbound, Outbound};
        put(
            &state,
            &account_id,
            &here,
            ("Theo", "theo@example.com"),
            "me@example.com",
            3,
            Inbound,
        )
        .await;
        put(
            &state,
            &account_id,
            &here,
            ("Me", "me@example.com"),
            "theo@example.com",
            1,
            Outbound,
        )
        .await;

        let facts = context(get_thread_context(&state, &here).await.unwrap());
        assert_eq!(facts.owed_reply, None);
        let person = facts.counterparty.unwrap();
        assert_eq!(person.email, "theo@example.com");
        assert_eq!(person.last_contact_elsewhere_at, None, "first conversation");
    }

    #[tokio::test]
    async fn newsletters_never_count_as_owed() {
        let state = AppState::in_memory().await.unwrap();
        let account_id = state.default_account_id();
        let here = ThreadId::new();
        let envelope = TestEnvelopeBuilder::new()
            .account_id(account_id.clone())
            .thread_id(here.clone())
            .provider_id("news-1")
            .sender_address("Weekly", "news@list.example.com")
            .unsubscribe(UnsubscribeMethod::HttpLink {
                url: "https://list.example.com/u".into(),
            })
            .build();
        state
            .store
            .upsert_envelope_with_direction(&envelope, MessageDirection::Inbound)
            .await
            .unwrap();

        let facts = context(get_thread_context(&state, &here).await.unwrap());
        assert_eq!(facts.owed_reply, None);
        assert!(facts.counterparty.unwrap().bulk_sender);
    }
}
