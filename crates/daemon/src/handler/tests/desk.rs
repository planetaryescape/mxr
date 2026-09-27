use super::*;
use chrono::Duration;
use mxr_core::id::{LabelId, MessageId, ThreadId};
use mxr_core::types::{
    Address, Envelope, EventSource, Label, LabelKind, MessageDirection, MessageFlags, Snoozed,
};
use mxr_store::{CommitmentDirection, CommitmentStatus, ContactCommitmentRecord};

const ME: &str = "user@example.com";

struct Fixture {
    state: Arc<AppState>,
    account: mxr_core::AccountId,
    inbox: LabelId,
}

impl Fixture {
    async fn new() -> Self {
        let state = Arc::new(AppState::in_memory().await.unwrap());
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
            account,
            inbox,
        }
    }

    /// Store one message; inbound mail lands in the inbox.
    async fn message(
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

    async fn store_envelope(&self, envelope: &Envelope, direction: MessageDirection) {
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

    async fn promise(&self, thread: &ThreadId, evidence: &MessageId, due_in: Duration) {
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

    async fn desk(&self, lane_limit: u32) -> ResponseData {
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

fn thread_ids(lane: &DeskLaneData) -> Vec<ThreadId> {
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
