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
        assert_eq!(
            stored.get(&facet.message_id).map(|a| a.summary()),
            Some(facet.clone())
        );
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
    assert!(
        fx.state
            .store
            .list_todos_in_state(Some(&fx.account), mxr_store::TodoState::Open, 100)
            .await
            .unwrap()
            .is_empty(),
        "a stale automatic row is hidden before reconciliation"
    );
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
    assert!(
        fx.state
            .store
            .list_todos_in_state(Some(&fx.account), mxr_store::TodoState::Open, 100)
            .await
            .unwrap()
            .is_empty(),
        "withdrawn asks do not retain an active task"
    );
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
    assert!(!source
        .units
        .iter()
        .any(|u| u.answer == AskAnswer::YouPromised));
    assert!(!stored_owner(&fx, &mail.id).await);
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
    assert!(stored_owner(&fx, &mail.id).await);
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
    assert!(answer.units.iter().any(|u| u.answer == AskAnswer::AsksYou
        && u.quote.as_deref() == Some("Please sign the contract.")));
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
    assert!(answer.units.iter().any(|u| u.answer == AskAnswer::AsksYou
        && u.quote.as_deref() == Some("Please sign the agreement.")));
}

async fn stored_owner(fx: &Fixture, id: &MessageId) -> bool {
    fx.state
        .store
        .asks_for_messages(&fx.account, std::slice::from_ref(id))
        .await
        .unwrap()[id]
        .facts
        .owner_authored
}

#[tokio::test]
async fn stored_asks_drive_owner_actions_quotes_and_four_corrections_with_undo() {
    use mxr_protocol::{TodoCorrectionData as Reason, TodoEditData, TodoStateData};
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    let mail = fx
        .message(&thread, "alex@example.com", ME, Duration::minutes(5), None)
        .await;
    body(&fx, &mail, "Please sign the contract.").await;
    crate::handler::todos::scan_messages(&fx.state, std::slice::from_ref(&mail.id)).await;
    let rows = fx
        .state
        .store
        .list_todos_in_state(Some(&fx.account), mxr_store::TodoState::Open, 100)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    let before = rows[0].clone();
    assert_eq!(before.origin, "asks");
    assert_eq!(before.reason, "Please sign the contract.");
    let fields = mxr_todo::provenance::FieldSources::from_json(&before.field_sources);
    assert!(fields
        .get("requester")
        .and_then(|p| p.evidence.as_deref())
        .unwrap()
        .contains("asked"));
    for reason in [
        Reason::NotATodo,
        Reason::NotMine,
        Reason::AlreadyDone,
        Reason::WrongDetails,
    ] {
        let edits = if reason == Reason::WrongDetails {
            vec![TodoEditData {
                field: "title".into(),
                value: "Sign the right contract".into(),
            }]
        } else {
            vec![]
        };
        let ResponseData::TodoChange { change: preview } = request(
            &fx,
            Request::CorrectTodo {
                todo_id: before.id.clone(),
                reason,
                time_zone: None,
                edits: edits.clone(),
                dry_run: true,
            },
        )
        .await
        else {
            panic!("preview");
        };
        assert!(preview.correction_undo.is_none());
        assert_eq!(
            fx.state.store.get_todo(&before.id).await.unwrap(),
            Some(before.clone())
        );
        let ResponseData::TodoChange { change } = request(
            &fx,
            Request::CorrectTodo {
                todo_id: before.id.clone(),
                reason,
                time_zone: None,
                edits,
                dry_run: false,
            },
        )
        .await
        else {
            panic!("change");
        };
        assert_eq!(change.changed[0].why, "Please sign the contract.");
        let expected_state = match reason {
            Reason::NotATodo | Reason::NotMine => TodoStateData::Dismissed,
            Reason::AlreadyDone => TodoStateData::Done,
            Reason::WrongDetails => TodoStateData::Open,
        };
        assert_eq!(change.changed[0].state, expected_state);
        request(
            &fx,
            Request::UndoTodoCorrection {
                todo_id: before.id.clone(),
                correction_id: change.correction_undo.unwrap(),
                dry_run: false,
            },
        )
        .await;
        assert_eq!(
            fx.state.store.get_todo(&before.id).await.unwrap(),
            Some(before.clone())
        );
    }
    let error = crate::handler::todos::correct(
        &fx.state,
        &before.id,
        Reason::WrongDetails,
        &[TodoEditData {
            field: "__proto__".into(),
            value: "x".into(),
        }],
        None,
        false,
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("Can't edit"));
}

#[tokio::test]
async fn two_references_reminders_and_distinct_actions_use_stored_identity() {
    let fx = Fixture::new().await;
    let first = fx
        .message(
            &ThreadId::new(),
            "bills@example.com",
            ME,
            Duration::minutes(15),
            None,
        )
        .await;
    body(
        &fx,
        &first,
        "Invoice #A123 amount due £25 by 1 November 2026.",
    )
    .await;
    let second = fx
        .message(
            &ThreadId::new(),
            "bills@example.com",
            ME,
            Duration::minutes(10),
            None,
        )
        .await;
    body(
        &fx,
        &second,
        "Invoice #B456 amount due £40 by 1 November 2026.",
    )
    .await;
    let reminder = fx
        .message(
            &ThreadId::new(),
            "bills@example.com",
            ME,
            Duration::minutes(5),
            None,
        )
        .await;
    body(
        &fx,
        &reminder,
        "Reminder: Invoice #A123 amount due £25 by 1 November 2026.",
    )
    .await;
    for mail in [&first, &second, &reminder] {
        crate::handler::todos::scan_messages(&fx.state, std::slice::from_ref(&mail.id)).await;
    }
    let rows = fx
        .state
        .store
        .list_todos_in_state(Some(&fx.account), mxr_store::TodoState::Open, 100)
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows
        .iter()
        .any(|row| row.source_message_id.as_ref() == Some(&reminder.id)));
    assert!(rows
        .iter()
        .any(|row| row.source_message_id.as_ref() == Some(&second.id)));
    let multi = fx
        .message(
            &ThreadId::new(),
            "alex@example.com",
            ME,
            Duration::minutes(2),
            None,
        )
        .await;
    body(
        &fx,
        &multi,
        "Please sign the contract and pay invoice #C789 amount due £30 by 1 November 2026.",
    )
    .await;
    crate::handler::todos::scan_messages(&fx.state, std::slice::from_ref(&multi.id)).await;
    let rows = fx
        .state
        .store
        .list_todos_in_state(Some(&fx.account), mxr_store::TodoState::Open, 100)
        .await
        .unwrap();
    assert_eq!(
        rows.iter()
            .filter(|row| row.source_message_id.as_ref() == Some(&multi.id))
            .count(),
        2
    );
}
