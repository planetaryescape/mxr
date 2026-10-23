use super::desk::{request, Fixture, ME};
use super::*;
use chrono::Duration;
use mxr_core::asks::{AskAnswer, ASKS_POLICY_VERSION};
use mxr_core::{Envelope, MessageBody, ThreadId};
use mxr_protocol::{ResponseData, ThreadModesData};

async fn body(fx: &Fixture, mail: &Envelope, text: &str) {
    fx.state
        .store
        .insert_body(&MessageBody {
            message_id: mail.id.clone(),
            text_plain: Some(text.into()),
            text_html: None,
            attachments: vec![],
            fetched_at: mail.date,
            metadata: Default::default(),
        })
        .await
        .unwrap();
}

async fn membership(fx: &Fixture, thread: &ThreadId) -> ThreadModesData {
    let ResponseData::ModeMembership { mut threads } = request(
        fx,
        Request::GetModeMembership {
            message_id: None,
            thread_id: Some(thread.clone()),
            thread_ids: vec![],
        },
    )
    .await
    else {
        panic!("mode membership response");
    };
    threads.remove(0)
}

#[tokio::test]
async fn asks_membership_reads_persisted_received_and_sent_facets_without_outbound_homes() {
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    let inbound = fx
        .message(&thread, "alex@example.com", ME, Duration::minutes(10), None)
        .await;
    body(&fx, &inbound, "Please sign the contract.").await;
    let sent = fx
        .message(
            &thread,
            ME,
            "alex@example.com",
            Duration::minutes(5),
            inbound.message_id_header.as_deref(),
        )
        .await;
    body(&fx, &sent, "I'll send the report by tomorrow.").await;
    let modes = membership(&fx, &thread).await;
    assert!(modes.asks_pending.is_empty());
    assert_eq!(modes.asks.len(), 2);
    assert_eq!(modes.assessments.len(), 1);
    assert_eq!(modes.assessments[0].message_id, inbound.id);
    let stored = fx
        .state
        .store
        .asks_for_messages(&fx.account, &[inbound.id.clone(), sent.id.clone()])
        .await
        .unwrap();
    for facet in &modes.asks {
        assert_eq!(stored.get(&facet.message_id), Some(facet));
    }
    let promised = stored.get(&sent.id).unwrap();
    assert_eq!(promised.policy_version, ASKS_POLICY_VERSION);
    assert!(promised
        .units
        .iter()
        .any(|u| u.answer == AskAnswer::YouPromised));
    assert_eq!(promised.facts.predecessor.as_ref(), Some(&inbound.id));
    assert!(stored[&inbound.id].facts.owner_replied_afterwards);
}

#[tokio::test]
async fn body_replacement_invalidates_asks_and_canonical_membership_persists_the_replacement() {
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    let mail = fx
        .message(&thread, "alex@example.com", ME, Duration::minutes(5), None)
        .await;
    body(&fx, &mail, "Please sign the contract.").await;
    let before = membership(&fx, &thread).await;
    let source = before
        .asks
        .iter()
        .find(|a| a.message_id == mail.id)
        .unwrap();
    assert!(source.units.iter().any(|u| u.answer == AskAnswer::AsksYou));
    body(&fx, &mail, "Please do not sign the contract.").await;
    assert!(fx
        .state
        .store
        .asks_for_messages(&fx.account, std::slice::from_ref(&mail.id))
        .await
        .unwrap()
        .is_empty());
    let after = membership(&fx, &thread).await;
    let replacement = after.asks.iter().find(|a| a.message_id == mail.id).unwrap();
    assert_ne!(source.input_fingerprint, replacement.input_fingerprint);
    assert!(!replacement
        .units
        .iter()
        .any(|u| u.answer == AskAnswer::AsksYou));
    assert_eq!(before.assessments[0].home, after.assessments[0].home);
    assert_eq!(before.assessments[0].who, after.assessments[0].who);
}

#[tokio::test]
async fn alias_changes_invalidate_and_reinterpret_sent_speaker_through_owned_addresses() {
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    let mail = fx
        .message(
            &thread,
            "alias@example.com",
            "alex@example.com",
            Duration::minutes(5),
            None,
        )
        .await;
    body(&fx, &mail, "I'll send the report by tomorrow.").await;
    let before = membership(&fx, &thread).await;
    let source = before
        .asks
        .iter()
        .find(|a| a.message_id == mail.id)
        .unwrap();
    assert!(!source.facts.owner_authored);
    sqlx::query("INSERT INTO account_addresses(account_id, email, is_primary) VALUES (?, 'alias@example.com', 0)")
        .bind(fx.account.as_str()).execute(fx.state.store.writer()).await.unwrap();
    assert!(fx
        .state
        .store
        .asks_for_messages(&fx.account, std::slice::from_ref(&mail.id))
        .await
        .unwrap()
        .is_empty());
    let after = membership(&fx, &thread).await;
    let answer = after.asks.iter().find(|a| a.message_id == mail.id).unwrap();
    assert!(answer.facts.owner_authored);
    assert!(answer
        .units
        .iter()
        .any(|u| u.answer == AskAnswer::YouPromised));
    assert!(
        after.assessments.is_empty(),
        "owned alias cannot acquire an inbound home"
    );
}

#[tokio::test]
async fn asks_read_full_cached_body_and_html_fallback_instead_of_desk_snippets() {
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    let mail = fx
        .message(&thread, "alex@example.com", ME, Duration::minutes(5), None)
        .await;
    let text = format!(
        "{}\nPlease sign the contract.",
        "For your records.\n".repeat(600)
    );
    body(&fx, &mail, &text).await;
    let modes = membership(&fx, &thread).await;
    let answer = modes.asks.iter().find(|a| a.message_id == mail.id).unwrap();
    assert!(answer
        .units
        .iter()
        .any(|u| u.answer == AskAnswer::AsksYou
            && answer.quote(u) == Some("Please sign the contract.")));
    fx.state
        .store
        .insert_body(&MessageBody {
            message_id: mail.id.clone(),
            text_plain: None,
            text_html: Some("<p>Please sign the agreement.</p>".into()),
            attachments: vec![],
            fetched_at: mail.date,
            metadata: Default::default(),
        })
        .await
        .unwrap();
    let html = membership(&fx, &thread).await;
    let answer = html.asks.iter().find(|a| a.message_id == mail.id).unwrap();
    assert!(!answer.pending_body);
    assert!(answer
        .units
        .iter()
        .any(|u| u.answer == AskAnswer::AsksYou
            && answer.quote(u) == Some("Please sign the agreement.")));
}
