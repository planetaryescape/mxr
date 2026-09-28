use super::*;
use chrono::Duration;
use mxr_core::id::{LabelId, MessageId, ThreadId};
use mxr_core::types::{
    Address, Envelope, EventSource, Label, LabelKind, MessageDirection, MessageFlags, Snoozed,
};
use mxr_store::{CommitmentDirection, CommitmentStatus, ContactCommitmentRecord};

pub(super) const ME: &str = "user@example.com";

pub(super) struct Fixture {
    pub(super) state: Arc<AppState>,
    /// The account's provider, for making its mutations fail.
    pub(super) fake: Arc<mxr_provider_fake::FakeProvider>,
    pub(super) account: mxr_core::AccountId,
    pub(super) inbox: LabelId,
}

impl Fixture {
    pub(super) async fn new() -> Self {
        let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
        let state = Arc::new(state);
        let account = state.default_account_id();
        let inbox = LabelId::from_scoped_provider_id(&account, "fake", "INBOX");
        state
            .store
            .upsert_label(&Label {
                id: inbox.clone(),
                account_id: account.clone(),
                name: "Inbox".into(),
                kind: LabelKind::System,
                color: None,
                provider_id: "INBOX".into(),
                unread_count: 0,
                total_count: 0,
                role: None,
            })
            .await
            .unwrap();
        Self {
            state,
            fake,
            account,
            inbox,
        }
    }

    /// Store one message; inbound mail lands in the inbox.
    pub(super) async fn message(
        &self,
        thread: &ThreadId,
        from: &str,
        to: &str,
        age: Duration,
        in_reply_to: Option<&str>,
    ) -> Envelope {
        let outbound = from == ME;
        let id = MessageId::new();
        let envelope = Envelope {
            id: id.clone(),
            account_id: self.account.clone(),
            provider_id: format!("desk-{id}"),
            thread_id: thread.clone(),
            message_id_header: Some(format!("<{id}@example.com>")),
            in_reply_to: in_reply_to.map(str::to_string),
            references: vec![],
            from: Address {
                name: None,
                email: from.into(),
            },
            to: vec![Address {
                name: None,
                email: to.into(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "Re: Launch plan".into(),
            date: chrono::Utc::now() - age,
            flags: if outbound {
                MessageFlags::READ | MessageFlags::SENT
            } else {
                MessageFlags::empty()
            },
            snippet: String::new(),
            has_attachments: false,
            size_bytes: 10,
            unsubscribe: UnsubscribeMethod::None,
            link_count: 0,
            body_word_count: 0,
            label_provider_ids: vec![],
            keywords: std::collections::BTreeSet::new(),
        };
        let direction = if outbound {
            MessageDirection::Outbound
        } else {
            MessageDirection::Inbound
        };
        self.store_envelope(&envelope, direction).await;
        envelope
    }

    pub(super) async fn store_envelope(&self, envelope: &Envelope, direction: MessageDirection) {
        let store = &self.state.store;
        store
            .upsert_envelope_with_direction(envelope, direction)
            .await
            .unwrap();
        if direction == MessageDirection::Inbound {
            store
                .set_message_labels(
                    &envelope.id,
                    std::slice::from_ref(&self.inbox),
                    EventSource::User,
                )
                .await
                .unwrap();
        }
        store
            .try_create_reply_pair(envelope, direction)
            .await
            .unwrap();
    }

    pub(super) async fn promise(&self, thread: &ThreadId, evidence: &MessageId, due_in: Duration) {
        self.state
            .store
            .upsert_contact_commitment(&ContactCommitmentRecord {
                id: format!("c-{thread}"),
                account_id: self.account.clone(),
                email: "nora@example.com".into(),
                thread_id: thread.clone(),
                direction: CommitmentDirection::Yours,
                status: CommitmentStatus::Open,
                who_owes: "you".into(),
                what: "send the security review notes".into(),
                by_when: Some(chrono::Utc::now() + due_in),
                evidence_msg_id: evidence.clone(),
                extracted_at: chrono::Utc::now(),
                resolved_at: None,
            })
            .await
            .unwrap();
    }

    pub(super) async fn desk(&self, lane_limit: u32) -> ResponseData {
        let msg = IpcMessage {
            id: 1,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::GetDesk {
                account_id: None,
                lane_limit,
            }),
        };
        match handle_request(&self.state, &msg).await.payload {
            IpcPayload::Response(Response::Ok { data }) => data,
            other => panic!("expected desk, got {other:?}"),
        }
    }
}

pub(super) fn thread_ids(lane: &DeskLaneData) -> Vec<ThreadId> {
    lane.rows.iter().map(|row| row.thread_id.clone()).collect()
}

#[tokio::test]
async fn get_desk_assigns_lanes_dedupes_and_explains_pace() {
    let fx = Fixture::new().await;

    // Two old exchanges (outside the desk window) where you answered Maya
    // within an hour: that is your usual pace with her.
    for days in [40, 45] {
        let old = ThreadId::new();
        let asked = fx
            .message(&old, "maya@example.com", ME, Duration::days(days), None)
            .await;
        fx.message(
            &old,
            ME,
            "maya@example.com",
            Duration::days(days) - Duration::hours(1),
            asked.message_id_header.as_deref(),
        )
        .await;
    }

    // Owed: Maya replied two days ago. A promise in the same thread must
    // not show it twice.
    let owed = ThreadId::new();
    let mine = fx
        .message(&owed, ME, "maya@example.com", Duration::days(3), None)
        .await;
    let reply = fx
        .message(
            &owed,
            "maya@example.com",
            ME,
            Duration::days(2),
            mine.message_id_header.as_deref(),
        )
        .await;
    fx.promise(&owed, &reply.id, Duration::days(1)).await;

    // Waiting: you wrote Jon three days ago and he has not answered.
    let waiting = ThreadId::new();
    fx.message(&waiting, ME, "jon@example.com", Duration::days(3), None)
        .await;

    // Due beats waiting: you wrote Nora and promised notes in two days.
    let due = ThreadId::new();
    let to_nora = fx
        .message(&due, ME, "nora@example.com", Duration::days(5), None)
        .await;
    fx.promise(&due, &to_nora.id, Duration::days(2)).await;

    // New from people, newest first.
    let older_new = ThreadId::new();
    fx.message(
        &older_new,
        "theo@example.com",
        ME,
        Duration::hours(20),
        None,
    )
    .await;
    let newer_new = ThreadId::new();
    fx.message(&newer_new, "iris@example.com", ME, Duration::hours(2), None)
        .await;

    // Snoozed mail is out of sight.
    let snoozed = ThreadId::new();
    let napping = fx
        .message(&snoozed, "ari@example.com", ME, Duration::hours(1), None)
        .await;
    fx.state
        .store
        .insert_snooze(&Snoozed {
            message_id: napping.id.clone(),
            account_id: fx.account.clone(),
            snoozed_at: chrono::Utc::now(),
            wake_at: chrono::Utc::now() + Duration::days(1),
            original_labels: vec![fx.inbox.clone()],
        })
        .await
        .unwrap();

    // A newsletter is reading, not work.
    let newsletter_thread = ThreadId::new();
    let mut newsletter = fx
        .message(
            &newsletter_thread,
            "weekly@lists.example.com",
            ME,
            Duration::hours(3),
            None,
        )
        .await;
    newsletter.unsubscribe = UnsubscribeMethod::OneClick {
        url: "https://lists.example.com/unsubscribe".into(),
    };
    fx.store_envelope(&newsletter, MessageDirection::Inbound)
        .await;

    let ResponseData::Desk {
        owed: owed_lane,
        due: due_lane,
        waiting: waiting_lane,
        people_new,
        elsewhere,
        last_from_people_at,
        ..
    } = fx.desk(25).await
    else {
        panic!("expected a desk");
    };

    assert_eq!(thread_ids(&owed_lane), vec![owed.clone()]);
    let row = &owed_lane.rows[0];
    assert_eq!(row.reason, "replied to your message");
    assert_eq!(row.subject, "Launch plan");
    assert_eq!(row.message_id, reply.id);
    assert_eq!(row.message_ids.len(), 2);
    assert_eq!(row.usual_seconds, Some(3600));
    assert_eq!(row.usual_samples, 2);
    assert!(row.overdue, "two days against a one hour habit");

    assert_eq!(thread_ids(&due_lane), vec![due.clone()]);
    assert_eq!(
        due_lane.rows[0].commitment_id.as_deref(),
        Some(&*format!("c-{due}"))
    );
    assert!(due_lane.rows[0].reason.contains("security review notes"));
    assert!(due_lane.rows[0].age_seconds < 0, "not due yet");

    assert_eq!(thread_ids(&waiting_lane), vec![waiting.clone()]);
    assert_eq!(waiting_lane.rows[0].counterparty_email, "jon@example.com");
    assert_eq!(waiting_lane.rows[0].reason, "no reply yet");

    assert_eq!(thread_ids(&people_new), vec![newer_new, older_new]);
    assert_eq!(people_new.rows[0].reason, "first message from them");
    assert!(people_new.rows[0].unread);

    assert_eq!(elsewhere.reading, 1);
    // New senders await a decision; the link knows which account has them.
    assert!(elsewhere.screener > 0);
    assert_eq!(elsewhere.screener_account.as_ref(), Some(&fx.account));
    assert!(last_from_people_at.is_some());

    // The lane limit trims rows, never totals.
    let ResponseData::Desk {
        people_new: trimmed,
        ..
    } = fx.desk(1).await
    else {
        panic!("expected a desk");
    };
    assert_eq!(trimmed.rows.len(), 1);
    assert_eq!(trimmed.total, 2);
}

#[tokio::test]
async fn archiving_a_conversation_takes_it_off_the_desk() {
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    let mine = fx
        .message(&thread, ME, "maya@example.com", Duration::days(1), None)
        .await;
    let reply = fx
        .message(
            &thread,
            "maya@example.com",
            ME,
            Duration::hours(5),
            mine.message_id_header.as_deref(),
        )
        .await;
    let ResponseData::Desk { owed, .. } = fx.desk(25).await else {
        panic!("expected a desk");
    };
    assert_eq!(owed.total, 1);

    fx.state
        .store
        .set_message_labels(&reply.id, &[], EventSource::User)
        .await
        .unwrap();
    let ResponseData::Desk {
        owed,
        waiting,
        people_new,
        ..
    } = fx.desk(25).await
    else {
        panic!("expected a desk");
    };
    assert_eq!(owed.total + waiting.total + people_new.total, 0);
}

pub(super) async fn request(fx: &Fixture, request: Request) -> ResponseData {
    let msg = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(request),
    };
    match handle_request(&fx.state, &msg).await.payload {
        IpcPayload::Response(Response::Ok { data }) => data,
        other => panic!("expected data, got {other:?}"),
    }
}

#[tokio::test]
async fn done_waiting_lasts_until_a_new_message_arrives() {
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    let asked = fx
        .message(&thread, ME, "jon@example.com", Duration::days(3), None)
        .await;
    let waiting = |desk: ResponseData| match desk {
        ResponseData::Desk { waiting, .. } => thread_ids(&waiting),
        other => panic!("expected a desk, got {other:?}"),
    };
    assert_eq!(waiting(fx.desk(25).await), vec![thread.clone()]);

    // A dry run previews the same selection and changes nothing.
    let preview = request(
        &fx,
        Request::DismissDeskThreads {
            thread_ids: vec![thread.clone()],
            dry_run: true,
        },
    )
    .await;
    assert!(matches!(
        &preview,
        ResponseData::DeskThreadsDismissed { threads, dry_run: true }
            if threads.len() == 1 && threads[0].thread_id == thread
    ));
    assert_eq!(waiting(fx.desk(25).await), vec![thread.clone()]);

    request(
        &fx,
        Request::DismissDeskThreads {
            thread_ids: vec![thread.clone()],
            dry_run: false,
        },
    )
    .await;
    assert!(waiting(fx.desk(25).await).is_empty());

    // Undo puts it back; dismissing again takes it off.
    let restored = request(
        &fx,
        Request::RestoreDeskThreads {
            thread_ids: vec![thread.clone()],
        },
    )
    .await;
    assert!(matches!(
        restored,
        ResponseData::DeskThreadsRestored { restored: 1 }
    ));
    assert_eq!(waiting(fx.desk(25).await), vec![thread.clone()]);
    request(
        &fx,
        Request::DismissDeskThreads {
            thread_ids: vec![thread.clone()],
            dry_run: false,
        },
    )
    .await;

    // A follow-up you send later is new: the thread waits again.
    fx.message(
        &thread,
        ME,
        "jon@example.com",
        Duration::days(1),
        asked.message_id_header.as_deref(),
    )
    .await;
    assert_eq!(waiting(fx.desk(25).await), vec![thread.clone()]);
}

#[tokio::test]
async fn a_reply_to_a_dismissed_thread_lands_as_owed() {
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    let asked = fx
        .message(&thread, ME, "jon@example.com", Duration::days(3), None)
        .await;
    request(
        &fx,
        Request::DismissDeskThreads {
            thread_ids: vec![thread.clone()],
            dry_run: false,
        },
    )
    .await;
    fx.message(
        &thread,
        "jon@example.com",
        ME,
        Duration::hours(2),
        asked.message_id_header.as_deref(),
    )
    .await;
    let ResponseData::Desk { owed, waiting, .. } = fx.desk(25).await else {
        panic!("expected a desk");
    };
    assert_eq!(thread_ids(&owed), vec![thread]);
    assert_eq!(waiting.total, 0);
}

#[tokio::test]
async fn a_watched_contacts_thread_obeys_done_waiting_and_covers_the_thread() {
    let fx = Fixture::new().await;
    // You last wrote Nora six weeks ago, outside the desk window, and you
    // watch her at a weekly cadence: she drifts onto Waiting on.
    let thread = ThreadId::new();
    let first = fx
        .message(&thread, ME, "nora@example.com", Duration::days(45), None)
        .await;
    // Her answer long ago, archived: a person's row ignores archive.
    let answer = fx
        .message(&thread, "nora@example.com", ME, Duration::days(44), None)
        .await;
    fx.state
        .store
        .set_message_labels(&answer.id, &[], EventSource::User)
        .await
        .unwrap();
    fx.message(
        &thread,
        ME,
        "nora@example.com",
        Duration::days(42),
        first.message_id_header.as_deref(),
    )
    .await;
    fx.state
        .store
        .upsert_contact(&mxr_core::types::ContactRow {
            account_id: fx.account.clone(),
            email: "nora@example.com".into(),
            display_name: Some("Nora Kim".into()),
            first_seen_at: chrono::Utc::now() - Duration::days(200),
            last_seen_at: chrono::Utc::now() - Duration::days(42),
            last_inbound_at: None,
            last_outbound_at: Some(chrono::Utc::now() - Duration::days(42)),
            total_inbound: 0,
            total_outbound: 2,
            replied_count: 0,
            cadence_days_p50: None,
        })
        .await
        .unwrap();
    fx.state
        .store
        .watch_cadence(
            &mxr_store::RelationshipWatchEntry {
                account_id: fx.account.clone(),
                email: "nora@example.com".into(),
                expected_days: Some(7.0),
                note: None,
                added_at: chrono::Utc::now(),
            },
            false,
        )
        .await
        .unwrap();

    let ResponseData::Desk { waiting, .. } = fx.desk(25).await else {
        panic!("expected a desk");
    };
    assert_eq!(thread_ids(&waiting), vec![thread.clone()]);
    let row = &waiting.rows[0];
    assert_eq!(row.reason, "usually in touch every week");
    assert_eq!(
        row.message_ids.len(),
        3,
        "the whole thread, not one message"
    );

    // Done waiting holds for drift rows too, with nothing new since.
    request(
        &fx,
        Request::DismissDeskThreads {
            thread_ids: vec![thread.clone()],
            dry_run: false,
        },
    )
    .await;
    let ResponseData::Desk { waiting, .. } = fx.desk(25).await else {
        panic!("expected a desk");
    };
    assert_eq!(waiting.total, 0);
}

#[tokio::test]
async fn a_promise_covers_its_whole_thread_and_shows_its_star() {
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    let mut starred = fx
        .message(&thread, "nora@example.com", ME, Duration::days(40), None)
        .await;
    starred.flags |= MessageFlags::STARRED;
    fx.store_envelope(&starred, MessageDirection::Inbound).await;
    let promise = fx
        .message(
            &thread,
            ME,
            "nora@example.com",
            Duration::days(39),
            starred.message_id_header.as_deref(),
        )
        .await;
    fx.promise(&thread, &promise.id, Duration::days(1)).await;

    let ResponseData::Desk { due, .. } = fx.desk(25).await else {
        panic!("expected a desk");
    };
    let row = &due.rows[0];
    assert_eq!(row.thread_id, thread);
    assert_eq!(row.message_ids.len(), 2);
    assert!(row.starred);
    assert_eq!(row.subject, "Launch plan");
}

#[tokio::test]
async fn a_delayed_message_with_an_old_date_ends_done_waiting() {
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    let asked = fx
        .message(&thread, ME, "jon@example.com", Duration::days(3), None)
        .await;
    request(
        &fx,
        Request::DismissDeskThreads {
            thread_ids: vec![thread.clone()],
            dry_run: false,
        },
    )
    .await;
    // Delivered now, dated a week ago: still news to the thread.
    fx.message(
        &thread,
        ME,
        "jon@example.com",
        Duration::days(7),
        asked.message_id_header.as_deref(),
    )
    .await;
    let ResponseData::Desk { waiting, .. } = fx.desk(25).await else {
        panic!("expected a desk");
    };
    assert_eq!(thread_ids(&waiting), vec![thread]);
}
