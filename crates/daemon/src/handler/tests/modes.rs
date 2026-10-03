//! The modes on one email: membership and its reasons, done per mode with
//! its handoff copy, archive when the last mode lets go, the preview
//! matching the run, undo, and the rail's counts.

use super::desk::{request, Fixture, ME};
use super::*;
use chrono::Duration;
use mxr_core::id::{MessageId, ThreadId};
use mxr_core::types::{Envelope, MessageDirection};
use mxr_protocol::{
    ModeDoneOutcomeData, ModeKindData, RailStatusData, SenderKindData, ThreadModesData,
};

/// One message with its own sender and subject.
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
    envelope
}

/// You asked Sam about the lease; he sent it back to sign and asked if
/// you're around Thursday, so it's your turn.
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
    assert!(messages.early, "Messages is an early version on the desk");
    assert!(
        messages
            .reason
            .starts_with("Here because: your turn with sam@lettings.example"),
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
    // You've written to Sam, so he's never asked about.
    assert!(placed.new_sender.is_none());

    // The same thread named by its message.
    let ResponseData::ModeMembership { threads } = request(
        &fx,
        Request::GetModeMembership {
            message_id: Some(ask.id.clone()),
            thread_id: None,
            thread_ids: vec![thread.clone()],
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
    // Nothing changed yet.
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

    // Off the desk too: Messages and the desk share one owed rule.
    let ResponseData::Desk { owed, .. } = fx.desk(25).await else {
        panic!("expected the desk")
    };
    assert_eq!(owed.total, 0);

    // A new message brings it back to Messages only.
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
    assert_eq!(preview.copy, "Ticked off. Archived on the mail server.");
    assert_eq!(todo_state(&fx, &todo).await, mxr_store::TodoState::Open);
    assert!(in_inbox(&fx, &ask.id).await);

    let (outcome, mutation_id) = done(&fx, &thread, ModeKindData::Todo, false).await;
    assert_eq!(outcome, preview);
    assert_eq!(todo_state(&fx, &todo).await, mxr_store::TodoState::Done);
    assert!(!in_inbox(&fx, &ask.id).await, "the last mode let go");
    assert!(membership(&fx, &thread).await.modes.is_empty());

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
    let row = fx.state.store.get_todo(&todo).await.unwrap().unwrap();
    assert!(row.done_at.is_none());
}

#[tokio::test]
async fn with_archive_on_last_done_off_done_only_leaves_the_mode() {
    let fx = Fixture::new().await;
    let mut config = fx.state.config_snapshot();
    config.modes.archive_on_last_done = false;
    fx.state.set_config_for_test(config).await;
    let (thread, ask) = landlord(&fx).await;

    let (outcome, _) = done(&fx, &thread, ModeKindData::Messages, false).await;
    assert_eq!(outcome.archived, 0);
    assert_eq!(
        outcome.copy,
        "Done in Messages. Left in the inbox on the mail server."
    );
    assert!(in_inbox(&fx, &ask.id).await);
    assert!(membership(&fx, &thread).await.modes.is_empty());
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
        "Here because: looks like a record, \"receipt\" in the subject (rule)."
    );

    // Archive holds records, not the inbox: done in Updates archives.
    let (outcome, _) = done(&fx, &receipt_thread, ModeKindData::Updates, false).await;
    assert_eq!(outcome.copy, "Done. Archived on the mail server.");
    assert!(!in_inbox(&fx, &receipt.id).await);
    assert_eq!(
        modes(&membership(&fx, &receipt_thread).await),
        [ModeKindData::Archive]
    );

    // With the setting off, a later notification brings Updates back.
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

    // Answering is the sender's kind: the question goes.
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

#[tokio::test]
async fn archive_has_no_done_and_unknown_threads_say_so() {
    let fx = Fixture::new().await;
    let (thread, _) = landlord(&fx).await;
    let msg = IpcMessage {
        id: 3,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::SetModeDone {
            thread_ids: vec![thread.clone()],
            mode: ModeKindData::Archive,
            dry_run: true,
        }),
    };
    assert!(matches!(
        handle_request(&fx.state, &msg).await.payload,
        IpcPayload::Response(Response::Error { .. })
    ));

    let missing = ThreadId::new();
    let ResponseData::ModeDone { items, .. } = request(
        &fx,
        Request::SetModeDone {
            thread_ids: vec![missing, thread.clone(), thread],
            mode: ModeKindData::Todo,
            dry_run: true,
        },
    )
    .await
    else {
        panic!("expected ModeDone")
    };
    assert_eq!(items[0].error.as_deref(), Some("conversation not found"));
    assert_eq!(
        items[1].error.as_deref(),
        Some("no open to-do on this conversation")
    );
    assert_eq!(
        items[2].error.as_deref(),
        Some("this thread is already in the request")
    );
}

#[tokio::test]
async fn the_rail_lists_now_the_modes_and_inbox_with_keys_and_counts() {
    let fx = Fixture::new().await;
    let (_, ask) = landlord(&fx).await;
    add_todo(&fx, &ask.id, "tomorrow").await;
    let build = ThreadId::new();
    mail(
        &fx,
        &build,
        "notifications@github.com",
        "Build failed on main",
        Duration::hours(2),
    )
    .await;

    let ResponseData::Rail { rail } = request(&fx, Request::GetRail { account_id: None }).await
    else {
        panic!("expected the rail")
    };
    let ids: Vec<&str> = rail.entries.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(
        ids,
        ["now", "messages", "todo", "updates", "reading", "archive", "inbox"]
    );
    let keys: Vec<&str> = rail.entries.iter().map(|e| e.key.as_str()).collect();
    assert_eq!(keys, ["g h", "g m", "g x", "g u", "g r", "g e", "g i"]);
    let entry = |id: &str| rail.entries.iter().find(|e| e.id == id).unwrap();
    // Sam owes nothing; you owe Sam, and the to-do is due: work is 2.
    assert_eq!(entry("now").badge, Some(2));
    assert_eq!(entry("messages").count, Some(1));
    assert_eq!(entry("todo").count, Some(1));
    assert_eq!(
        entry("todo").badge,
        None,
        "To do earns a badge later (D117)"
    );
    assert_eq!(entry("updates").count, Some(1));
    assert_eq!(entry("reading").count, Some(0));
    assert_eq!(entry("archive").count, None);
    for id in ["messages", "updates", "reading", "archive"] {
        assert_eq!(entry(id).status, RailStatusData::Early, "{id}");
        assert!(entry(id).early_note.is_some(), "{id}");
    }
    for id in ["now", "todo", "inbox"] {
        assert_eq!(entry(id).status, RailStatusData::Built, "{id}");
        assert!(entry(id).header.is_some(), "{id}");
    }
    assert!(
        rail.more.iter().any(|link| link.id == "screener"),
        "the Screener lives under More"
    );
}
