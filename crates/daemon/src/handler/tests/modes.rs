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
    // A signed lease is a contract: ticking it off files it in Archive,
    // and the toast keeps that apart from the provider's archive.
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

    // Filed as a record (the detector needs a reference or a total, which
    // this bare test mail lacks, so it is filed by hand).
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
            todo_ids: Vec::new(),
            sender: None,
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
            todo_ids: Vec::new(),
            sender: None,
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
    assert_eq!(entry("reading").count, None, "nothing in Reading is owed");
    assert_eq!(entry("archive").count, None);
    for id in [
        "now", "messages", "todo", "updates", "reading", "archive", "inbox",
    ] {
        assert_eq!(entry(id).status, RailStatusData::Built, "{id}");
        assert!(entry(id).header.is_some(), "{id}");
    }
    assert!(
        rail.more.iter().any(|link| link.id == "screener"),
        "the Screener lives under More"
    );
}

#[tokio::test]
async fn only_the_mode_holding_a_thread_can_let_it_go() {
    let fx = Fixture::new().await;
    let (thread, ask) = landlord(&fx).await;

    let (outcome, _) = done(&fx, &thread, ModeKindData::Updates, true).await;
    assert_eq!(outcome.error.as_deref(), Some("not in Updates"));
    assert_eq!(outcome.archived, 0);

    done(&fx, &thread, ModeKindData::Messages, false).await;
    // Archived by the last mode letting go; done again is refused.
    let (again, _) = done(&fx, &thread, ModeKindData::Messages, true).await;
    assert_eq!(again.error.as_deref(), Some("already done in Messages"));
    assert!(!in_inbox(&fx, &ask.id).await);
}

#[tokio::test]
async fn restoring_on_the_desk_brings_back_a_thread_done_in_messages() {
    let fx = Fixture::new().await;
    let mut config = fx.state.config_snapshot();
    config.modes.archive_on_last_done = false;
    fx.state.set_config_for_test(config).await;
    let (thread, _) = landlord(&fx).await;
    done(&fx, &thread, ModeKindData::Messages, false).await;
    assert!(membership(&fx, &thread).await.modes.is_empty());

    let ResponseData::DeskThreadsRestored { restored } = request(
        &fx,
        Request::RestoreDeskThreads {
            thread_ids: vec![thread.clone()],
        },
    )
    .await
    else {
        panic!("expected DeskThreadsRestored")
    };
    assert_eq!(restored, 1);
    assert_eq!(
        modes(&membership(&fx, &thread).await),
        [ModeKindData::Messages]
    );
}

#[tokio::test]
async fn an_automated_new_sender_is_not_asked_and_lands_in_its_mode_saying_why() {
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    mail(
        &fx,
        &thread,
        "alerts@newservice.example",
        "Your weekly usage report",
        Duration::hours(1),
    )
    .await;
    let placed = membership(&fx, &thread).await;
    assert!(
        placed.new_sender.is_none(),
        "only a person is asked about; machines go to their mode silently"
    );
    assert_eq!(modes(&placed), [ModeKindData::Updates]);
    assert_eq!(
        placed.modes[0].reason,
        "Here because: automated sender (rule)."
    );
}

/// Iris wrote weeks ago and you never answered or archived it: no lane
/// holds it, but it is still person mail in the inbox.
async fn quiet_person(fx: &Fixture, age: Duration) -> (ThreadId, Envelope) {
    let thread = ThreadId::new();
    let note = mail(
        fx,
        &thread,
        "iris@people.example",
        "Photos from the trip",
        age,
    )
    .await;
    // She is someone you've written to before, so she isn't new.
    let other = ThreadId::new();
    mail(fx, &other, ME, "Dinner?", Duration::days(90)).await;
    (thread, note)
}

#[tokio::test]
async fn person_mail_left_in_the_inbox_stays_in_messages_as_quiet() {
    let fx = Fixture::new().await;
    for age in [Duration::days(10), Duration::days(45)] {
        let (thread, _) = quiet_person(&fx, age).await;
        let placed = membership(&fx, &thread).await;
        assert_eq!(
            modes(&placed),
            [ModeKindData::Messages],
            "person mail in the inbox never vanishes from every mode ({age})"
        );
        assert_eq!(placed.held_by, [ModeKindData::Messages]);
        let entry = &placed.modes[0];
        assert!(
            entry.reason.starts_with("Here because: quiet, "),
            "{}",
            entry.reason
        );
        assert!(
            entry.also_in.starts_with("Also in Messages: quiet, "),
            "{}",
            entry.also_in
        );
    }
}

#[tokio::test]
async fn ticking_off_a_to_do_on_a_quiet_thread_never_archives_it() {
    let fx = Fixture::new().await;
    let (thread, note) = quiet_person(&fx, Duration::days(45)).await;
    let todo = add_todo(&fx, &note.id, "tomorrow").await;

    let (outcome, _) = done(&fx, &thread, ModeKindData::Todo, false).await;
    assert_eq!(outcome.todos_ticked, vec![todo]);
    assert_eq!(outcome.still_in, [ModeKindData::Messages]);
    assert_eq!(outcome.archived, 0, "nobody said done in Messages");
    assert!(in_inbox(&fx, &note.id).await);

    // Done in Messages is what lets it go.
    let (outcome, _) = done(&fx, &thread, ModeKindData::Messages, false).await;
    assert_eq!(outcome.archived, 1);
    assert!(!in_inbox(&fx, &note.id).await);
}

/// Delete a message the way a provider expunge reaches the store, without
/// the done-mark cleanup, so the next insert can take its rowid.
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

    // Updates: two notifications, done, the newest deleted, a new one in.
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

    // Messages: Sam's ask, done, deleted, and a new message from Sam.
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
async fn done_in_updates_takes_the_thread_out_of_its_early_view_while_to_do_keeps_it() {
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    let failed = mail(
        &fx,
        &thread,
        "billing@shop.example",
        "Payment failed for your plan",
        Duration::hours(2),
    )
    .await;
    add_todo(&fx, &failed.id, "tomorrow").await;
    let in_place = |fx: &Fixture| {
        let fx_state = fx.state.clone();
        let thread = thread.clone();
        async move {
            let ResponseData::Place { bundles, .. } = crate::handler::places::list_place(
                &fx_state,
                mxr_protocol::MailPlaceData::PaperTrail,
                None,
                None,
                crate::handler::places::PlacePage {
                    limit: 50,
                    offset: 0,
                    messages_per_bundle: 50,
                    message_offset: 0,
                },
            )
            .await
            .unwrap() else {
                panic!("expected a place")
            };
            bundles
                .iter()
                .flat_map(|bundle| &bundle.messages)
                .any(|message| message.thread_id == thread)
        }
    };
    assert!(in_place(&fx).await, "Updates' early view shows it");

    let (outcome, _) = done(&fx, &thread, ModeKindData::Updates, false).await;
    assert_eq!(outcome.still_in, [ModeKindData::Todo]);
    assert!(in_inbox(&fx, &failed.id).await, "To do still holds it");
    assert!(!in_place(&fx).await, "done in Updates leaves Updates' view");
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
async fn a_late_message_with_an_older_date_brings_a_done_thread_back() {
    let fx = Fixture::new().await;
    let mut config = fx.state.config_snapshot();
    config.modes.archive_on_last_done = false;
    fx.state.set_config_for_test(config).await;

    // Updates: done, then a notification arrives dated before the others.
    let build = ThreadId::new();
    let from = "notifications@github.com";
    mail(&fx, &build, from, "Build failed", Duration::hours(2)).await;
    done(&fx, &build, ModeKindData::Updates, false).await;
    assert!(membership(&fx, &build).await.modes.is_empty());
    mail(
        &fx,
        &build,
        from,
        "Build failed (delayed)",
        Duration::hours(5),
    )
    .await;
    assert_eq!(
        modes(&membership(&fx, &build).await),
        [ModeKindData::Updates],
        "a message the mark never saw is new, whatever its Date says"
    );

    // Messages: done, a message deleted, and a new one dated earlier.
    let (thread, ask) = landlord(&fx).await;
    done(&fx, &thread, ModeKindData::Messages, false).await;
    drop_message(&fx, &ask.id).await;
    mail(
        &fx,
        &thread,
        "sam@lettings.example",
        "Re: Lease renewal",
        Duration::days(3),
    )
    .await;
    assert_eq!(
        modes(&membership(&fx, &thread).await),
        [ModeKindData::Messages],
        "same count, older date, but a message the mark never saw"
    );
}

async fn todo_done(fx: &Fixture, thread: &ThreadId, todo_ids: Vec<String>) -> ModeDoneOutcomeData {
    match request(
        fx,
        Request::SetModeDone {
            thread_ids: vec![thread.clone()],
            mode: ModeKindData::Todo,
            dry_run: false,
            todo_ids,
            sender: None,
        },
    )
    .await
    {
        ResponseData::ModeDone { mut items, .. } => items.remove(0),
        other => panic!("expected ModeDone, got {other:?}"),
    }
}

#[tokio::test]
async fn ticking_off_one_to_do_leaves_the_others_and_the_inbox() {
    let fx = Fixture::new().await;
    let (thread, ask) = landlord(&fx).await;
    let first = add_todo(&fx, &ask.id, "tomorrow").await;
    let second = match request(
        &fx,
        Request::CreateTodo {
            message_id: ask.id.clone(),
            title: "Book the boiler engineer".into(),
            kind: None,
            due: Some("friday".into()),
            time_zone: None,
            dry_run: false,
        },
    )
    .await
    {
        ResponseData::TodoChange { change } => change.changed[0].id.clone(),
        other => panic!("expected a to-do, got {other:?}"),
    };
    done(&fx, &thread, ModeKindData::Messages, false).await;

    let outcome = todo_done(&fx, &thread, vec![first.clone()]).await;
    assert_eq!(outcome.todos_ticked, vec![first.clone()]);
    assert_eq!(outcome.still_in, [ModeKindData::Todo], "one to-do is left");
    assert_eq!(outcome.archived, 0);
    assert!(in_inbox(&fx, &ask.id).await);
    assert_eq!(todo_state(&fx, &second).await, mxr_store::TodoState::Open);

    let outcome = todo_done(&fx, &thread, vec![second.clone()]).await;
    assert!(outcome.still_in.is_empty());
    assert_eq!(outcome.archived, 1, "the last to-do was the last mode");
}

#[tokio::test]
async fn done_for_a_sender_covers_every_thread_of_theirs_in_the_mode() {
    let fx = Fixture::new().await;
    let from = "notifications@github.com";
    let mut threads = Vec::new();
    for index in 0..25 {
        let thread = ThreadId::new();
        mail(
            &fx,
            &thread,
            from,
            &format!("Build {index} passed"),
            Duration::hours(1),
        )
        .await;
        threads.push(thread);
    }
    let other = ThreadId::new();
    mail(
        &fx,
        &other,
        "alerts@vercel.example",
        "Deploy ok",
        Duration::hours(1),
    )
    .await;

    let sender = Some(mxr_protocol::ModeDoneSenderData {
        account_id: fx.account.clone(),
        sender_email: from.into(),
    });
    let ResponseData::ModeDone { items, .. } = request(
        &fx,
        Request::SetModeDone {
            thread_ids: Vec::new(),
            mode: ModeKindData::Updates,
            dry_run: true,
            todo_ids: Vec::new(),
            sender: sender.clone(),
        },
    )
    .await
    else {
        panic!("expected ModeDone")
    };
    assert_eq!(
        items.len(),
        25,
        "the preview counts all of the sender's threads"
    );
    let ResponseData::ModeDone { items, .. } = request(
        &fx,
        Request::SetModeDone {
            thread_ids: Vec::new(),
            mode: ModeKindData::Updates,
            dry_run: false,
            todo_ids: Vec::new(),
            sender,
        },
    )
    .await
    else {
        panic!("expected ModeDone")
    };
    assert_eq!(items.len(), 25);
    assert!(membership(&fx, &threads[24]).await.modes.is_empty());
    assert_eq!(
        modes(&membership(&fx, &other).await),
        [ModeKindData::Updates]
    );
}

#[tokio::test]
async fn quiet_mail_is_counted_on_the_rail_beside_messages() {
    let fx = Fixture::new().await;
    quiet_person(&fx, Duration::days(45)).await;
    let ResponseData::Rail { rail } = request(&fx, Request::GetRail { account_id: None }).await
    else {
        panic!("expected Rail")
    };
    let messages = rail
        .entries
        .iter()
        .find(|entry| entry.id == "messages")
        .unwrap();
    assert_eq!(messages.quiet, Some(1));
}

/// A bank desk that has written the same two subjects for months and
/// you never wrote back, and a toy shop's `hello@`: machines, not people.
#[tokio::test]
async fn templated_and_role_senders_you_never_wrote_to_land_in_updates_not_messages() {
    let fx = Fixture::new().await;
    let mut latest = ThreadId::new();
    for n in 0..8 {
        latest = ThreadId::new();
        let subject = if n % 2 == 0 {
            "Payment Confirmation"
        } else {
            "Foreign Payment"
        };
        mail(
            &fx,
            &latest,
            "desk.officer@bank.example",
            subject,
            Duration::days(i64::from(8 - n) * 20) - Duration::hours(1),
        )
        .await;
    }
    let shop = ThreadId::new();
    mail(
        &fx,
        &shop,
        "hello@toys.example",
        "What's new in the card store",
        Duration::hours(2),
    )
    .await;
    fx.state.store.refresh_contacts().await.unwrap();

    let bank = membership(&fx, &latest).await;
    assert_eq!(modes(&bank), [ModeKindData::Updates]);
    assert_eq!(
        bank.modes[0].reason,
        "Here because: the same few subjects every time, and you have never written to them (rule)."
    );
    assert!(bank.in_inbox, "the inbox keeps it");
    assert!(bank.new_sender.is_none());
    let toys = membership(&fx, &shop).await;
    assert_eq!(modes(&toys), [ModeKindData::Updates]);
    assert_eq!(
        toys.modes[0].reason,
        "Here because: a role address you have never written to (rule)."
    );

    // Moving the sender to people wins over every rule.
    request(
        &fx,
        Request::SetSenderKind {
            account_id: fx.account.clone(),
            sender_email: "desk.officer@bank.example".into(),
            kind: Some(SenderKindData::People),
        },
    )
    .await;
    assert_eq!(
        modes(&membership(&fx, &latest).await),
        [ModeKindData::Messages]
    );
}

/// The same bank desk, once you have written to it, is someone you talk to.
#[tokio::test]
async fn a_templated_sender_you_have_written_to_stays_in_messages() {
    let fx = Fixture::new().await;
    let mut latest = ThreadId::new();
    for n in 0..8 {
        latest = ThreadId::new();
        mail(
            &fx,
            &latest,
            "desk.officer@bank.example",
            "Payment Confirmation",
            Duration::days(i64::from(8 - n) * 20) - Duration::hours(1),
        )
        .await;
    }
    let mut asked = fx
        .message(
            &ThreadId::new(),
            ME,
            "desk.officer@bank.example",
            Duration::days(200),
            None,
        )
        .await;
    asked.subject = "Question about a payment".into();
    fx.store_envelope(&asked, MessageDirection::Outbound).await;
    fx.state.store.refresh_contacts().await.unwrap();
    assert_eq!(
        modes(&membership(&fx, &latest).await),
        [ModeKindData::Messages]
    );
}

/// A welcome from a club's founders, sent through a bulk-mail service: the
/// first message from a sender you never wrote to, and not a person.
#[tokio::test]
async fn a_welcome_sent_through_a_bulk_service_is_not_a_new_person() {
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    let welcome = mail(
        &fx,
        &thread,
        "maya.chen@club.example",
        "Welcome to the Club",
        Duration::hours(2),
    )
    .await;
    fx.state
        .store
        .insert_body(&mxr_core::types::MessageBody {
            message_id: welcome.id.clone(),
            text_plain: Some("Start the tutorial: https://club.example/manual".into()),
            text_html: None,
            attachments: vec![],
            fetched_at: chrono::Utc::now(),
            metadata: mxr_core::types::MessageMetadata {
                raw_headers: Some(
                    "Received: from a.example\r\nX-SES-Outgoing: 2026.10.07\r\nFeedback-ID: 1:club\r\n"
                        .into(),
                ),
                ..mxr_core::types::MessageMetadata::default()
            },
        })
        .await
        .unwrap();
    fx.state.store.refresh_contacts().await.unwrap();

    let placed = membership(&fx, &thread).await;
    assert_eq!(modes(&placed), [ModeKindData::Updates]);
    assert!(
        placed.new_sender.is_none(),
        "a machine is never asked about"
    );
    assert_eq!(
        placed.modes[0].reason,
        "Here because: sent through a bulk-mail service, and you have never written to them (rule)."
    );

    // The same welcome from a founders@ address needs no headers to tell.
    let other = ThreadId::new();
    mail(
        &fx,
        &other,
        "founders@club.example",
        "Welcome to the Club",
        Duration::hours(1),
    )
    .await;
    fx.state.store.refresh_contacts().await.unwrap();
    assert_eq!(
        modes(&membership(&fx, &other).await),
        [ModeKindData::Updates]
    );
}
