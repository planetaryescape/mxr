use super::desk::{request, Fixture, ME};
use super::*;
use chrono::Duration;
use mxr_core::id::{MessageId, ThreadId};
use mxr_core::types::{Envelope, MessageDirection};
use mxr_protocol::{ModeDoneOutcomeData, ModeKindData, SenderKindData, ThreadModesData};

async fn mail(
    fx: &Fixture,
    thread: &ThreadId,
    from: &str,
    subject: &str,
    age: Duration,
) -> Envelope {
    let to = if from == ME {
        "sam@lettings.example"
    } else {
        ME
    };
    let mut envelope = fx.message(thread, from, to, age, None).await;
    envelope.subject = subject.to_string();
    let direction = if from == ME {
        MessageDirection::Outbound
    } else {
        MessageDirection::Inbound
    };
    fx.store_envelope(&envelope, direction).await;
    if direction == MessageDirection::Inbound {
        fx.state
            .store
            .insert_body(&mxr_core::MessageBody {
                message_id: envelope.id.clone(),
                text_plain: Some("Could you reply with your availability?".into()),
                text_html: None,
                attachments: vec![],
                fetched_at: envelope.date,
                metadata: Default::default(),
            })
            .await
            .unwrap();
    }
    envelope
}

async fn landlord(fx: &Fixture) -> (ThreadId, Envelope) {
    let thread = ThreadId::new();
    mail(fx, &thread, ME, "Lease renewal", Duration::days(2)).await;
    let ask = mail(
        fx,
        &thread,
        "sam@lettings.example",
        "Re: Lease renewal",
        Duration::hours(3),
    )
    .await;
    (thread, ask)
}

async fn add_todo(fx: &Fixture, message: &MessageId, due: &str) -> String {
    match request(
        fx,
        Request::CreateTodo {
            message_id: message.clone(),
            title: "Sign the lease renewal".into(),
            kind: Some("sign".into()),
            due: Some(due.into()),
            time_zone: None,
            dry_run: false,
        },
    )
    .await
    {
        ResponseData::TodoChange { change } => change.changed[0].id.clone(),
        other => panic!("expected a to-do, got {other:?}"),
    }
}

async fn membership(fx: &Fixture, thread: &ThreadId) -> ThreadModesData {
    match request(
        fx,
        Request::GetModeMembership {
            message_id: None,
            thread_id: Some(thread.clone()),
            thread_ids: Vec::new(),
            limit: None,
        },
    )
    .await
    {
        ResponseData::ModeMembership { mut threads } => {
            assert_eq!(threads.len(), 1);
            threads.remove(0)
        }
        other => panic!("expected membership, got {other:?}"),
    }
}

fn modes(data: &ThreadModesData) -> Vec<ModeKindData> {
    data.modes.iter().map(|entry| entry.mode).collect()
}

async fn done(
    fx: &Fixture,
    thread: &ThreadId,
    mode: ModeKindData,
    dry_run: bool,
) -> (ModeDoneOutcomeData, Option<String>) {
    match request(
        fx,
        Request::SetModeDone {
            thread_ids: vec![thread.clone()],
            mode,
            dry_run,
            todo_ids: Vec::new(),
            sender: None,
        },
    )
    .await
    {
        ResponseData::ModeDone {
            mut items,
            dry_run: echoed,
            mutation_id,
            undo_unavailable,
        } => {
            assert_eq!(echoed, dry_run);
            assert!(!undo_unavailable);
            assert_eq!(items.len(), 1);
            (items.remove(0), mutation_id)
        }
        other => panic!("expected ModeDone, got {other:?}"),
    }
}

async fn in_inbox(fx: &Fixture, id: &MessageId) -> bool {
    fx.state
        .store
        .get_envelope(id)
        .await
        .unwrap()
        .unwrap()
        .label_provider_ids
        .iter()
        .any(|label| label == "INBOX")
}

async fn todo_state(fx: &Fixture, id: &str) -> mxr_store::TodoState {
    fx.state.store.get_todo(id).await.unwrap().unwrap().state
}

#[tokio::test]
async fn the_landlords_email_is_in_messages_and_to_do_each_saying_why() {
    let fx = Fixture::new().await;
    let (thread, ask) = landlord(&fx).await;
    let todo = add_todo(&fx, &ask.id, "tomorrow").await;

    let placed = membership(&fx, &thread).await;
    assert_eq!(modes(&placed), [ModeKindData::Messages, ModeKindData::Todo]);
    assert_eq!(placed.held_by, modes(&placed));
    assert!(placed.in_inbox);
    let messages = &placed.modes[0];
    assert_eq!(messages.key, "g m");
    assert!(!messages.early, "Messages is built (phase 3)");
    assert!(
        messages.reason.contains(
            &placed
                .assessments
                .iter()
                .find(|a| a.message_id == ask.id)
                .unwrap()
                .deciding
                .reason
        ),
        "{}",
        messages.reason
    );
    assert_eq!(
        messages.also_in,
        "Also in Messages: your turn with sam@lettings.example"
    );
    let todo_entry = &placed.modes[1];
    assert!(!todo_entry.early);
    assert_eq!(todo_entry.todo_ids, vec![todo]);
    assert!(
        todo_entry
            .also_in
            .starts_with("Also in To do: Sign the lease renewal, "),
        "{}",
        todo_entry.also_in
    );
    assert!(placed.new_sender.is_none());

    let ResponseData::ModeMembership { threads } = request(
        &fx,
        Request::GetModeMembership {
            message_id: Some(ask.id.clone()),
            thread_id: None,
            thread_ids: vec![thread.clone()],
            limit: None,
        },
    )
    .await
    else {
        panic!("expected membership")
    };
    assert_eq!(threads.len(), 1, "the thread once, however it was named");
}

#[tokio::test]
async fn done_in_messages_leaves_the_to_do_and_gmail_and_the_preview_matches() {
    let fx = Fixture::new().await;
    let (thread, ask) = landlord(&fx).await;
    let todo = add_todo(&fx, &ask.id, "tomorrow").await;

    let (preview, no_undo) = done(&fx, &thread, ModeKindData::Messages, true).await;
    assert!(no_undo.is_none());
    assert!(preview.error.is_none(), "{:?}", preview.error);
    assert_eq!(preview.still_in, vec![ModeKindData::Todo]);
    assert_eq!(preview.archived, 0);
    assert!(
        preview
            .copy
            .starts_with("Done in Messages. Still in To do (due "),
        "{}",
        preview.copy
    );
    assert_eq!(
        modes(&membership(&fx, &thread).await),
        [ModeKindData::Messages, ModeKindData::Todo]
    );

    let (outcome, mutation_id) = done(&fx, &thread, ModeKindData::Messages, false).await;
    assert_eq!(outcome, preview, "the run matches its preview");
    assert!(mutation_id.is_some(), "the mark can be undone");
    assert!(in_inbox(&fx, &ask.id).await, "To do still holds it");
    assert_eq!(todo_state(&fx, &todo).await, mxr_store::TodoState::Open);
    let after = membership(&fx, &thread).await;
    assert_eq!(modes(&after), [ModeKindData::Todo]);
    assert_eq!(after.done_in, [ModeKindData::Messages]);

    let ResponseData::Desk { owed, .. } = fx.desk(25).await else {
        panic!("expected the desk")
    };
    assert_eq!(owed.total, 0);

    mail(
        &fx,
        &thread,
        "sam@lettings.example",
        "Re: Lease renewal",
        Duration::minutes(5),
    )
    .await;
    assert_eq!(
        modes(&membership(&fx, &thread).await),
        [ModeKindData::Messages, ModeKindData::Todo]
    );
}

#[tokio::test]
async fn ticking_off_the_last_mode_archives_and_undo_puts_everything_back() {
    let fx = Fixture::new().await;
    let (thread, ask) = landlord(&fx).await;
    let todo = add_todo(&fx, &ask.id, "tomorrow").await;
    done(&fx, &thread, ModeKindData::Messages, false).await;

    let (preview, _) = done(&fx, &thread, ModeKindData::Todo, true).await;
    assert_eq!(preview.todos_ticked, vec![todo.clone()]);
    assert!(preview.still_in.is_empty());
    assert_eq!(preview.archived, 1, "Sam's message is the one in the inbox");
    assert_eq!(
        preview.copy,
        "Ticked off. Filed in Archive. Archived on the mail server."
    );
    assert_eq!(todo_state(&fx, &todo).await, mxr_store::TodoState::Open);
    assert!(in_inbox(&fx, &ask.id).await);

    let (outcome, mutation_id) = done(&fx, &thread, ModeKindData::Todo, false).await;
    assert_eq!(outcome, preview);
    assert_eq!(todo_state(&fx, &todo).await, mxr_store::TodoState::Done);
    assert!(!in_inbox(&fx, &ask.id).await, "the last mode let go");
    assert_eq!(
        modes(&membership(&fx, &thread).await),
        [ModeKindData::Archive],
        "the filed contract stays in Archive"
    );

    assert!(matches!(
        request(
            &fx,
            Request::UndoMutation {
                mutation_id: mutation_id.expect("an undo id"),
            },
        )
        .await,
        ResponseData::Ack
    ));
    assert_eq!(todo_state(&fx, &todo).await, mxr_store::TodoState::Open);
    assert!(in_inbox(&fx, &ask.id).await);
    assert!(
        !modes(&membership(&fx, &thread).await).contains(&ModeKindData::Archive),
        "undo unfiles what the tick-off filed"
    );
    let row = fx.state.store.get_todo(&todo).await.unwrap().unwrap();
    assert!(row.done_at.is_none());
}

#[tokio::test]
async fn automated_mail_is_in_updates_a_receipt_also_in_archive_and_archive_never_holds() {
    let fx = Fixture::new().await;
    let build = ThreadId::new();
    let failed = mail(
        &fx,
        &build,
        "notifications@github.com",
        "Build failed on main",
        Duration::hours(2),
    )
    .await;
    let receipt_thread = ThreadId::new();
    let receipt = mail(
        &fx,
        &receipt_thread,
        "receipts@shop.example",
        "Your receipt from Shop",
        Duration::hours(1),
    )
    .await;

    request(
        &fx,
        Request::FileRecord {
            message_id: receipt.id.clone(),
            kind: Some(mxr_protocol::RecordKindData::Receipt),
            dry_run: false,
        },
    )
    .await;
    let notification = membership(&fx, &build).await;
    assert_eq!(modes(&notification), [ModeKindData::Updates]);
    assert_eq!(
        notification.modes[0].reason,
        "Here because: automated sender (rule)."
    );
    let record = membership(&fx, &receipt_thread).await;
    assert_eq!(
        modes(&record),
        [ModeKindData::Updates, ModeKindData::Archive]
    );
    assert_eq!(record.held_by, [ModeKindData::Updates]);
    assert_eq!(
        record.modes[1].reason,
        "Here because: you filed it (unchecked)."
    );

    let (outcome, _) = done(&fx, &receipt_thread, ModeKindData::Updates, false).await;
    assert_eq!(outcome.copy, "Done. Archived on the mail server.");
    assert!(!in_inbox(&fx, &receipt.id).await);
    assert_eq!(
        modes(&membership(&fx, &receipt_thread).await),
        [ModeKindData::Archive]
    );

    let mut config = fx.state.config_snapshot();
    config.modes.archive_on_last_done = false;
    fx.state.set_config_for_test(config).await;
    done(&fx, &build, ModeKindData::Updates, false).await;
    let after = membership(&fx, &build).await;
    assert!(after.modes.is_empty());
    assert_eq!(after.done_in, [ModeKindData::Updates]);
    assert!(in_inbox(&fx, &failed.id).await);
    mail(
        &fx,
        &build,
        "notifications@github.com",
        "Build failed on main again",
        Duration::minutes(1),
    )
    .await;
    assert_eq!(
        modes(&membership(&fx, &build).await),
        [ModeKindData::Updates]
    );
}

#[tokio::test]
async fn a_first_time_sender_is_asked_one_question_where_their_mail_landed() {
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    mail(
        &fx,
        &thread,
        "iris@new.example",
        "Hello from Iris",
        Duration::hours(1),
    )
    .await;
    let placed = membership(&fx, &thread).await;
    let question = placed.new_sender.expect("a first-time sender is asked");
    assert_eq!(question.question, "New sender. Keep in Messages?");
    assert_eq!(question.choices[0].kind, SenderKindData::People);

    request(
        &fx,
        Request::SetSenderKind {
            account_id: fx.account.clone(),
            sender_email: "iris@new.example".into(),
            kind: Some(SenderKindData::People),
        },
    )
    .await;
    assert!(membership(&fx, &thread).await.new_sender.is_none());
}

async fn drop_message(fx: &Fixture, id: &MessageId) {
    sqlx::query("DELETE FROM messages WHERE id = ?1")
        .bind(id.as_str())
        .execute(fx.state.store.writer())
        .await
        .unwrap();
}

async fn rowid(fx: &Fixture, id: &MessageId) -> i64 {
    sqlx::query_scalar("SELECT rowid FROM messages WHERE id = ?1")
        .bind(id.as_str())
        .fetch_one(fx.state.store.reader())
        .await
        .unwrap()
}

#[tokio::test]
async fn a_new_message_brings_a_thread_back_even_when_sqlite_reuses_a_rowid() {
    let fx = Fixture::new().await;
    let mut config = fx.state.config_snapshot();
    config.modes.archive_on_last_done = false;
    fx.state.set_config_for_test(config).await;

    let build = ThreadId::new();
    let from = "notifications@github.com";
    mail(&fx, &build, from, "Build failed", Duration::hours(3)).await;
    let newest = mail(&fx, &build, from, "Build failed again", Duration::hours(2)).await;
    done(&fx, &build, ModeKindData::Updates, false).await;
    assert!(membership(&fx, &build).await.modes.is_empty());
    let reused = rowid(&fx, &newest.id).await;
    drop_message(&fx, &newest.id).await;
    let fresh = mail(&fx, &build, from, "Build fixed", Duration::minutes(5)).await;
    assert_eq!(
        rowid(&fx, &fresh.id).await,
        reused,
        "SQLite reused the rowid"
    );
    assert_eq!(
        modes(&membership(&fx, &build).await),
        [ModeKindData::Updates],
        "the new notification is never hidden"
    );

    let (thread, ask) = landlord(&fx).await;
    done(&fx, &thread, ModeKindData::Messages, false).await;
    assert!(membership(&fx, &thread).await.modes.is_empty());
    let reused = rowid(&fx, &ask.id).await;
    drop_message(&fx, &ask.id).await;
    let again = mail(
        &fx,
        &thread,
        "sam@lettings.example",
        "Re: Lease renewal",
        Duration::minutes(5),
    )
    .await;
    assert_eq!(
        rowid(&fx, &again.id).await,
        reused,
        "SQLite reused the rowid"
    );
    assert_eq!(
        modes(&membership(&fx, &thread).await),
        [ModeKindData::Messages],
        "Sam's new message brings the thread back to Messages"
    );
}

#[tokio::test]
async fn desk_done_never_archives_a_thread_to_do_still_holds() {
    let fx = Fixture::new().await;
    let (thread, ask) = landlord(&fx).await;
    add_todo(&fx, &ask.id, "tomorrow").await;
    let ResponseData::DeskItemsResolved { items, .. } = request(
        &fx,
        Request::ResolveDeskItems {
            items: vec![mxr_protocol::DeskDoneItemData {
                thread_id: thread.clone(),
                lane: Some(mxr_protocol::DeskLaneKind::Owed),
                commitment_id: None,
            }],
            dry_run: false,
        },
    )
    .await
    else {
        panic!("expected DeskItemsResolved")
    };
    assert!(items[0].error.is_none(), "{:?}", items[0].error);
    assert_eq!(items[0].archived, 0, "To do still holds the thread");
    assert!(in_inbox(&fx, &ask.id).await);
    assert!(items[0].dismissed, "it still leaves the desk");
}

#[tokio::test]
async fn an_archive_made_elsewhere_keeps_the_thread_in_its_modes() {
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    let notification = mail(
        &fx,
        &thread,
        "notifications@github.com",
        "Build failed on main again",
        Duration::hours(1),
    )
    .await;
    assert_eq!(
        modes(&membership(&fx, &thread).await),
        [ModeKindData::Updates]
    );

    fx.state
        .store
        .remove_message_label(
            &notification.id,
            &fx.inbox,
            mxr_core::types::EventSource::Sync,
        )
        .await
        .unwrap();
    let after = membership(&fx, &thread).await;
    assert_eq!(
        modes(&after),
        [ModeKindData::Updates],
        "an outside archive must not silently move the email out of Updates"
    );
    assert!(!after.in_inbox);

    let own = ThreadId::new();
    let own_mail = mail(
        &fx,
        &own,
        "notifications@github.com",
        "Build passed on main",
        Duration::hours(1),
    )
    .await;
    fx.state
        .store
        .set_message_labels(&own_mail.id, &[], mxr_core::types::EventSource::User)
        .await
        .unwrap();
    assert!(
        modes(&membership(&fx, &own).await).is_empty(),
        "mxr's own archive still lets go in its mode"
    );
}
