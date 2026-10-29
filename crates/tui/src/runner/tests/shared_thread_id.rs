//! A Gmail legacy thread id can be held by two accounts, so a list row is
//! (account, thread). Actions, fetches and per-row state must tell the two
//! conversations apart.

use super::*;
use crate::app::{thread_accounts, ThreadKey};
use crate::runtime::get_thread_request;
use mxr_protocol::{DeskLaneData, DeskLaneKind, DeskRowData, ModeKindData, ThreadAccountRef};

struct Shared {
    thread: ThreadId,
    account_a: AccountId,
    account_b: AccountId,
    a: Vec<Envelope>,
    b: Vec<Envelope>,
}

/// Account A holds two messages of the thread, account B one; the thread id
/// is the same. A's are the newest, so A's row is first.
fn shared() -> Shared {
    let thread = ThreadId::new();
    let account_a = AccountId::new();
    let account_b = AccountId::new();
    let now = chrono::Utc::now();
    let build = |account: &AccountId, subject: &str, age_minutes: i64| {
        TestEnvelopeBuilder::new()
            .account_id(account.clone())
            .thread_id(thread.clone())
            .subject(subject)
            .provider_id(subject)
            .date(now - chrono::Duration::minutes(age_minutes))
            .to(vec![])
            .message_id_header(None)
            .snippet("")
            .build()
    };
    Shared {
        a: vec![build(&account_a, "A1", 1), build(&account_a, "A2", 2)],
        b: vec![build(&account_b, "B1", 10)],
        thread,
        account_a,
        account_b,
    }
}

fn app_with(shared: &Shared) -> App {
    let mut app = App::new();
    let all: Vec<Envelope> = shared.a.iter().chain(&shared.b).cloned().collect();
    app.mailbox.envelopes = all.clone();
    app.mailbox.all_envelopes = all;
    app
}

fn ids(envelopes: &[Envelope]) -> std::collections::HashSet<MessageId> {
    envelopes.iter().map(|env| env.id.clone()).collect()
}

fn select_row_of(app: &mut App, account: &AccountId) {
    let index = app
        .mail_list_rows()
        .iter()
        .position(|row| &row.representative.account_id == account)
        .expect("a row for the account");
    app.mailbox.selected_index = index;
}

/// The messages the archive just asked for, whether it queued at once or
/// is waiting on a bulk confirm.
fn archived(app: &App) -> std::collections::HashSet<MessageId> {
    let request = app
        .pending_mutation_queue
        .last()
        .map(|queued| &queued.request)
        .or_else(|| {
            app.modals
                .pending_bulk_confirm
                .as_ref()
                .map(|pending| &pending.request)
        })
        .expect("an archive was requested");
    match request {
        Request::Mutation {
            mutation: MutationCommand::Archive { message_ids },
            ..
        } => message_ids.iter().cloned().collect(),
        other => panic!("expected an archive, got {other:?}"),
    }
}

#[test]
fn one_thread_id_in_two_accounts_is_two_rows() {
    let shared = shared();
    let app = app_with(&shared);
    let rows = app.mail_list_rows();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].representative.account_id, shared.account_a);
    assert_eq!(rows[0].message_count, 2);
    assert_eq!(rows[1].representative.account_id, shared.account_b);
    assert_eq!(rows[1].message_count, 1);
    assert_ne!(rows[0].thread_key(), rows[1].thread_key());
}

#[test]
fn archive_on_a_row_acts_on_only_that_accounts_messages() {
    let shared = shared();
    let mut app = app_with(&shared);

    select_row_of(&mut app, &shared.account_a);
    app.apply(Action::Archive);
    assert_eq!(archived(&app), ids(&shared.a), "A's row never takes B's");

    let mut app = app_with(&shared);
    select_row_of(&mut app, &shared.account_b);
    app.apply(Action::Archive);
    assert_eq!(archived(&app), ids(&shared.b), "B's row never takes A's");
}

#[test]
fn visual_selection_of_a_row_holds_only_that_accounts_messages() {
    let shared = shared();
    let mut app = app_with(&shared);
    select_row_of(&mut app, &shared.account_a);
    app.apply(Action::VisualLineMode);
    // Out over both rows and back: the range is rebuilt from the rows.
    app.apply(Action::MoveDown);
    assert_eq!(
        app.mailbox.selected_set.len(),
        3,
        "both rows are in the range"
    );
    app.apply(Action::MoveUp);
    assert_eq!(app.mailbox.selected_set, ids(&shared.a));
}

#[test]
fn opening_a_row_fetches_that_rows_account() {
    let shared = shared();
    let mut app = app_with(&shared);

    select_row_of(&mut app, &shared.account_a);
    app.apply(Action::OpenSelected);
    let key_a = ThreadKey::new(shared.account_a.clone(), shared.thread.clone());
    assert_eq!(app.mailbox.pending_thread_fetch, Some(key_a.clone()));
    assert!(matches!(
        get_thread_request(&key_a),
        Request::GetThread { thread_id, account_id: Some(account) }
            if thread_id == shared.thread && account == shared.account_a
    ));

    // The runtime takes the fetch; B's row, with the same thread id, is
    // still fetched rather than treated as already in flight.
    app.mailbox.in_flight_thread_fetch = app.mailbox.pending_thread_fetch.take();
    select_row_of(&mut app, &shared.account_b);
    app.apply(Action::OpenSelected);
    let key_b = ThreadKey::new(shared.account_b.clone(), shared.thread.clone());
    assert_eq!(app.mailbox.pending_thread_fetch, Some(key_b.clone()));
    assert!(matches!(
        get_thread_request(&key_b),
        Request::GetThread { account_id: Some(account), .. } if account == shared.account_b
    ));
}

#[test]
fn a_thread_answer_for_the_other_account_is_not_shown() {
    let shared = shared();
    let mut app = app_with(&shared);
    select_row_of(&mut app, &shared.account_a);
    app.apply(Action::OpenSelected);

    let thread_of = |account: &AccountId, messages: &[Envelope]| Thread {
        id: shared.thread.clone(),
        account_id: account.clone(),
        subject: "s".into(),
        participants: vec![],
        message_count: messages.len() as u32,
        unread_count: 0,
        latest_date: messages[0].date,
        snippet: String::new(),
        message_ids: messages.iter().map(|env| env.id.clone()).collect(),
    };
    app.resolve_thread_success(
        thread_of(&shared.account_b, &shared.b),
        shared.b.clone(),
        None,
    );
    assert!(app.mailbox.viewed_thread.is_none());

    app.resolve_thread_success(
        thread_of(&shared.account_a, &shared.a),
        shared.a.clone(),
        None,
    );
    let viewed = app.mailbox.viewed_thread.as_ref().expect("A's thread");
    assert_eq!(viewed.account_id, shared.account_a);
    assert_eq!(
        app.pending_thread_context,
        Some(ThreadKey::new(
            shared.account_a.clone(),
            shared.thread.clone()
        ))
    );
}

fn desk_row(account: &AccountId, thread: &ThreadId, subject: &str) -> DeskRowData {
    DeskRowData {
        lane: DeskLaneKind::Owed,
        account_id: account.clone(),
        thread_id: thread.clone(),
        message_id: MessageId::new(),
        message_ids: vec![],
        counterparty_email: "maya@example.com".into(),
        counterparty_name: None,
        subject: subject.into(),
        reason: "replied to your message".into(),
        since: chrono::Utc::now(),
        age_seconds: 60,
        usual_seconds: None,
        usual_samples: 0,
        overdue: false,
        unread: true,
        starred: false,
        commitment_id: None,
        back_at: None,
    }
}

#[test]
fn done_on_one_accounts_desk_row_leaves_the_other_and_names_its_account() {
    let shared = shared();
    let mut app = App::new();
    let rows = vec![
        desk_row(&shared.account_a, &shared.thread, "from A"),
        desk_row(&shared.account_b, &shared.thread, "from B"),
    ];
    app.mailbox.desk_page = crate::app::DeskPageState {
        lanes: vec![(
            DeskLaneKind::Owed,
            DeskLaneData {
                total: 2,
                rows: rows.clone(),
            },
        )],
        elsewhere: Default::default(),
        loaded: true,
        low_tide: false,
    };

    let key_a = ThreadKey::new(shared.account_a.clone(), shared.thread.clone());
    app.queue_mode_done(ModeKindData::Messages, key_a.clone());

    let left: Vec<&str> = app
        .mailbox
        .desk_page
        .rows()
        .map(|row| row.subject.as_str())
        .collect();
    assert_eq!(left, ["from B"], "B's row stays on the desk");
    assert!(matches!(
        &app.pending_mutation_queue.last().expect("queued").request,
        Request::SetModeDone { thread_ids, thread_accounts, .. }
            if *thread_ids == [shared.thread.clone()]
                && *thread_accounts == [ThreadAccountRef {
                    thread_id: shared.thread.clone(),
                    account_id: shared.account_a.clone(),
                }]
    ));
}

#[test]
fn keys_name_their_account_only_when_they_know_it() {
    let thread = ThreadId::new();
    let account = AccountId::new();
    let keys = [
        ThreadKey::new(account.clone(), thread.clone()),
        ThreadKey::bare(thread.clone()),
    ];
    assert_eq!(
        thread_accounts(&keys),
        vec![ThreadAccountRef {
            thread_id: thread,
            account_id: account
        }],
        "a bare id sends no pair and keeps the daemon's own choice"
    );
}

#[test]
fn focus_keeps_both_accounts_conversations() {
    let shared = shared();
    let mut app = App::new();
    app.apply(Action::OpenNow);
    app.apply(Action::OpenFocus);
    app.set_focus_sources(
        Ok((
            DeskLaneData {
                rows: vec![
                    desk_row(&shared.account_a, &shared.thread, "from A"),
                    desk_row(&shared.account_b, &shared.thread, "from B"),
                ],
                total: 2,
            },
            mxr_protocol::PeopleSplitData {
                write_with: 2,
                others: 0,
            },
        )),
        Ok(vec![]),
    );
    assert_eq!(app.mailbox.focus_page.queue.len(), 2);
    app.apply(Action::FocusDone);
    assert!(matches!(
        &app.pending_mutation_queue.last().expect("queued").request,
        Request::ResolveDeskItems { items, .. }
            if items.len() == 1
                && items[0].account_id.as_ref() == Some(&shared.account_a)
    ));
    assert_eq!(
        app.mailbox
            .focus_page
            .current()
            .map(|item| item.subject.as_str()),
        Some("from B")
    );
}
