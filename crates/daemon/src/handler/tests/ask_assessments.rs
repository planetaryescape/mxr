use super::desk::{request, Fixture, ME};
use super::*;
use chrono::Duration;
use mxr_core::asks::{AskAnswer, ASKS_POLICY_VERSION};
use mxr_core::{Envelope, MessageBody, MessageId, ThreadId};
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
    assert!(before.has_stored_asks());
    assert_eq!(before.origin, "rule");
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

#[tokio::test]
async fn projection_failure_after_facet_commit_replays_without_new_dirty_mail() {
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    let mail = fx
        .message(&thread, "alex@example.com", ME, Duration::minutes(5), None)
        .await;
    body(&fx, &mail, "Please sign the contract.").await;
    sqlx::raw_sql("CREATE TRIGGER fail_projection BEFORE INSERT ON todos BEGIN SELECT RAISE(ABORT, 'injected projection failure'); END;")
        .execute(fx.state.store.writer()).await.unwrap();
    assert!(crate::handler::assessments::reconcile_messages(
        &fx.state,
        &fx.account,
        std::slice::from_ref(&mail.id)
    )
    .await
    .is_err());
    assert_eq!(
        fx.state
            .store
            .pending_asks_projections(&fx.account)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(fx
        .state
        .store
        .dirty_assessment_threads(&fx.account, None)
        .await
        .unwrap()
        .is_empty());
    sqlx::query("DROP TRIGGER fail_projection")
        .execute(fx.state.store.writer())
        .await
        .unwrap();
    crate::handler::assessments::reconcile_dirty_page(&fx.state, &fx.account, None)
        .await
        .unwrap();
    assert!(fx
        .state
        .store
        .pending_asks_projections(&fx.account)
        .await
        .unwrap()
        .is_empty());
    assert_eq!(
        fx.state
            .store
            .list_todos_in_state(Some(&fx.account), mxr_store::TodoState::Open, 100)
            .await
            .unwrap()
            .len(),
        1
    );
    crate::handler::assessments::reconcile_dirty_page(&fx.state, &fx.account, None)
        .await
        .unwrap();
    assert_eq!(
        fx.state
            .store
            .list_todos_in_state(Some(&fx.account), mxr_store::TodoState::Open, 100)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn stored_reply_debt_respects_questions_cancellation_and_nonreply_senders() {
    let fx = Fixture::new().await;
    for (sender, text, reply) in [
        ("alex@example.com", "Does this work for you?", true),
        (
            "alex@example.com",
            "I've corrected the account name. No action is required.",
            false,
        ),
        ("no-reply@example.com", "Does this work for you?", false),
    ] {
        let thread = ThreadId::new();
        let mail = fx
            .message(&thread, sender, ME, Duration::minutes(5), None)
            .await;
        body(&fx, &mail, text).await;
        let modes = membership(&fx, &thread).await;
        assert_eq!(
            modes
                .asks
                .iter()
                .flat_map(|a| &a.units)
                .any(|u| u.answer == AskAnswer::AsksYouToReply),
            reply
        );
    }
}

#[tokio::test]
async fn already_done_correction_and_undo_mirror_the_stored_owner_promise() {
    let fx = Fixture::new().await;
    let mail = fx
        .message(
            &ThreadId::new(),
            ME,
            "alex@example.com",
            Duration::minutes(5),
            None,
        )
        .await;
    body(&fx, &mail, "I'll send the report by tomorrow.").await;
    crate::handler::todos::scan_messages(&fx.state, std::slice::from_ref(&mail.id)).await;
    let rows = fx
        .state
        .store
        .list_todos_in_state(Some(&fx.account), mxr_store::TodoState::Open, 100)
        .await
        .unwrap();
    let row = rows.first().expect("owner promise task");
    let commitment = row
        .commitment_id
        .as_deref()
        .expect("canonical commitment link");
    let ResponseData::TodoChange { change } = request(
        &fx,
        Request::CorrectTodo {
            todo_id: row.id.clone(),
            reason: mxr_protocol::TodoCorrectionData::AlreadyDone,
            time_zone: None,
            edits: vec![],
            dry_run: false,
        },
    )
    .await
    else {
        panic!("correction");
    };
    assert_eq!(
        fx.state
            .store
            .get_contact_commitment(commitment)
            .await
            .unwrap()
            .unwrap()
            .status,
        mxr_store::CommitmentStatus::Resolved
    );
    request(
        &fx,
        Request::UndoTodoCorrection {
            todo_id: row.id.clone(),
            correction_id: change.correction_undo.unwrap(),
            dry_run: false,
        },
    )
    .await;
    assert_eq!(
        fx.state
            .store
            .get_contact_commitment(commitment)
            .await
            .unwrap()
            .unwrap()
            .status,
        mxr_store::CommitmentStatus::Open
    );
    assert_eq!(
        fx.state.store.get_todo(&row.id).await.unwrap().as_ref(),
        Some(row)
    );
}

#[tokio::test]
async fn already_done_bill_files_archive_and_undo_removes_only_its_filing() {
    let fx = Fixture::new().await;
    let mail = fx
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
        &mail,
        "Invoice #B123 amount due £25 by 1 November 2026.",
    )
    .await;
    crate::handler::todos::scan_messages(&fx.state, std::slice::from_ref(&mail.id)).await;
    let rows = fx
        .state
        .store
        .list_todos_in_state(Some(&fx.account), mxr_store::TodoState::Open, 100)
        .await
        .unwrap();
    let row = rows.first().expect("bill task");
    let ResponseData::TodoChange { change } = request(
        &fx,
        Request::CorrectTodo {
            todo_id: row.id.clone(),
            reason: mxr_protocol::TodoCorrectionData::AlreadyDone,
            time_zone: None,
            edits: vec![],
            dry_run: false,
        },
    )
    .await
    else {
        panic!("correction");
    };
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM records WHERE account_id = ? AND dedup_key = ?")
            .bind(fx.account.as_str())
            .bind(mxr_store::todo_record_dedup_key(&row.id))
            .fetch_one(fx.state.store.reader())
            .await
            .unwrap();
    assert_eq!(count, 1);
    request(
        &fx,
        Request::UndoTodoCorrection {
            todo_id: row.id.clone(),
            correction_id: change.correction_undo.unwrap(),
            dry_run: false,
        },
    )
    .await;
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM records WHERE account_id = ? AND dedup_key = ?")
            .bind(fx.account.as_str())
            .bind(mxr_store::todo_record_dedup_key(&row.id))
            .fetch_one(fx.state.store.reader())
            .await
            .unwrap();
    assert_eq!(count, 0);
    assert_eq!(
        fx.state.store.get_todo(&row.id).await.unwrap().as_ref(),
        Some(row)
    );
}

#[tokio::test]
async fn legacy_ics_upgrade_keeps_one_row_and_user_history_then_partstat_completes() {
    use mxr_store::TodoState;
    for (state, edited) in [
        (TodoState::Open, false),
        (TodoState::Open, true),
        (TodoState::Done, false),
        (TodoState::Dismissed, false),
    ] {
        let fx = Fixture::new().await;
        let thread = ThreadId::new();
        let mail = fx
            .message(
                &thread,
                "organizer@example.com",
                ME,
                Duration::minutes(5),
                None,
            )
            .await;
        let mut metadata = mxr_core::MessageMetadata::default();
        metadata.calendar = Some(mxr_core::CalendarMetadata {
            method: Some("REQUEST".into()),
            uid: Some("invite-uid".into()),
            summary: Some("Planning".into()),
            starts_at: Some("20261101T090000Z".into()),
            rsvp_requested: true,
            attendees: vec![mxr_core::CalendarAttendee {
                email: ME.into(),
                name: None,
                uri: None,
                partstat: Some("NEEDS-ACTION".into()),
                role: None,
                rsvp: Some(true),
            }],
            ..Default::default()
        });
        fx.state
            .store
            .insert_body(&MessageBody {
                message_id: mail.id.clone(),
                text_plain: Some("Calendar invitation".into()),
                text_html: None,
                attachments: vec![],
                fetched_at: mail.date,
                metadata,
            })
            .await
            .unwrap();
        membership(&fx, &thread).await;
        let mut old = fx
            .state
            .store
            .list_todos_in_state(Some(&fx.account), TodoState::Open, 100)
            .await
            .unwrap()
            .remove(0);
        sqlx::query("DELETE FROM todos WHERE id = ?")
            .bind(&old.id)
            .execute(fx.state.store.writer())
            .await
            .unwrap();
        old.origin = "ics".into();
        old.rules_version = 1;
        old.dedup_key = format!("rsvp|{thread}|planning");
        old.field_sources = "{}".into();
        old.state = state;
        old.user_edited = edited;
        if edited {
            old.title = "My planning title".into();
        }
        fx.state.store.insert_todo(&old).await.unwrap();
        crate::handler::asks_upgrade::page(&fx.state, &fx.account, chrono::Utc::now())
            .await
            .unwrap();
        crate::handler::assessments::reconcile_dirty_page(&fx.state, &fx.account, None)
            .await
            .unwrap();
        let current = fx.state.store.get_todo(&old.id).await.unwrap().unwrap();
        assert_eq!(current.state, state);
        assert_eq!(current.title, old.title);
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM todos WHERE source_message_id = ?")
                .bind(mail.id.as_str())
                .fetch_one(fx.state.store.reader())
                .await
                .unwrap();
        assert_eq!(count, 1);
        crate::handler::asks_upgrade::page(&fx.state, &fx.account, chrono::Utc::now())
            .await
            .unwrap();
        crate::handler::assessments::reconcile_dirty_page(&fx.state, &fx.account, None)
            .await
            .unwrap();
        assert_eq!(
            fx.state.store.get_todo(&old.id).await.unwrap().unwrap(),
            current
        );
        if state == TodoState::Open {
            fx.state
                .store
                .update_calendar_invite_partstat(&mail.id, ME, "ACCEPTED")
                .await
                .unwrap();
            fx.state
                .store
                .complete_answered_rsvp_todos(chrono::Utc::now())
                .await
                .unwrap();
            assert_eq!(
                fx.state
                    .store
                    .get_todo(&old.id)
                    .await
                    .unwrap()
                    .unwrap()
                    .state,
                TodoState::Done
            );
        }
    }
}
