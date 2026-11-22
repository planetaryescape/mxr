use super::*;
use chrono::Duration;
use mxr_core::id::{LabelId, MessageId, ThreadId};
use mxr_core::types::{
    Address, Envelope, EventSource, Label, LabelKind, MessageDirection, MessageFlags,
};
use mxr_protocol::KindRuleData;

const ME: &str = "user@example.com";
const NEWSLETTER: &str = "editor@weekly.example";
const ROBOT: &str = "receipts@shop.example";

struct Fixture {
    state: Arc<AppState>,
    account: mxr_core::AccountId,
}

impl Fixture {
    async fn new() -> Self {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let account = state.default_account_id();
        let fx = Self { state, account };
        fx.inbox_label(&fx.account.clone()).await;
        fx
    }

    async fn inbox_label(&self, account: &mxr_core::AccountId) -> LabelId {
        let inbox = LabelId::from_scoped_provider_id(account, "fake", "INBOX");
        self.state
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
        inbox
    }

    async fn inbound(
        &self,
        account: &mxr_core::AccountId,
        from: &str,
        subject: &str,
        age: Duration,
        unsubscribe: bool,
    ) -> Envelope {
        let id = MessageId::new();
        let envelope = Envelope {
            id: id.clone(),
            account_id: account.clone(),
            provider_id: format!("places-{id}"),
            thread_id: ThreadId::new(),
            message_id_header: Some(format!("<{id}@example.com>")),
            in_reply_to: None,
            references: vec![],
            from: Address {
                name: None,
                email: from.into(),
            },
            to: vec![Address {
                name: None,
                email: ME.into(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: subject.into(),
            date: chrono::Utc::now() - age,
            flags: MessageFlags::empty(),
            snippet: String::new(),
            has_attachments: false,
            size_bytes: 10,
            unsubscribe: if unsubscribe {
                UnsubscribeMethod::OneClick {
                    url: "https://weekly.example/unsubscribe".into(),
                }
            } else {
                UnsubscribeMethod::None
            },
            link_count: 0,
            body_word_count: 0,
            label_provider_ids: vec![],
            keywords: std::collections::BTreeSet::new(),
        };
        let store = &self.state.store;
        store
            .upsert_envelope_with_direction(&envelope, MessageDirection::Inbound)
            .await
            .unwrap();
        let inbox = LabelId::from_scoped_provider_id(account, "fake", "INBOX");
        store
            .set_message_labels(&envelope.id, &[inbox], EventSource::User)
            .await
            .unwrap();
        envelope
    }

    async fn send(&self, request: Request) -> ResponseData {
        let msg = IpcMessage {
            expected_workspace: None,
            id: 1,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(request),
        };
        match handle_request(&self.state, &msg).await.payload {
            IpcPayload::Response(Response::Ok { data }) => data,
            other => panic!("expected Ok, got {other:?}"),
        }
    }

    async fn place(
        &self,
        place: MailPlaceData,
        account_id: Option<mxr_core::AccountId>,
    ) -> (Vec<PlaceBundleData>, u32) {
        match self
            .send(Request::ListPlace {
                place,
                account_id,
                sender_email: None,
                limit: 50,
                offset: 0,
                messages_per_bundle: 20,
                message_offset: 0,
            })
            .await
        {
            ResponseData::Place {
                bundles,
                total_messages,
                ..
            } => (bundles, total_messages),
            other => panic!("expected Place, got {other:?}"),
        }
    }

    async fn sweep(
        &self,
        sender_email: Option<&str>,
        dry_run: bool,
        preview_token: Option<String>,
    ) -> (SweepPreviewData, Option<JobData>) {
        match self
            .send(Request::SweepPlace {
                place: MailPlaceData::PaperTrail,
                account_id: Some(self.account.clone()),
                sender_email: sender_email.map(str::to_string),
                dry_run,
                preview_token,
            })
            .await
        {
            ResponseData::PlaceSwept { preview, job, .. } => (preview, job),
            other => panic!("expected PlaceSwept, got {other:?}"),
        }
    }

    async fn refused(&self, request: Request) -> String {
        let msg = IpcMessage {
            expected_workspace: None,
            id: 1,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(request),
        };
        match handle_request(&self.state, &msg).await.payload {
            IpcPayload::Response(Response::Error { message, .. }) => message,
            other => panic!("expected an error, got {other:?}"),
        }
    }

    async fn wait_for_job(&self, job_id: &str) -> JobData {
        for _ in 0..3000 {
            if let ResponseData::Job { job } = self
                .send(Request::GetJob {
                    job_id: job_id.to_string(),
                })
                .await
            {
                if !matches!(job.status, JobStatusData::Queued | JobStatusData::Running) {
                    return job;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        panic!("job {job_id} did not finish");
    }

    async fn in_inbox(&self, id: &MessageId) -> bool {
        self.state
            .store
            .get_envelope(id)
            .await
            .unwrap()
            .unwrap()
            .label_provider_ids
            .iter()
            .any(|label| label == "INBOX")
    }
}

fn bundle<'a>(bundles: &'a [PlaceBundleData], sender: &str) -> Option<&'a PlaceBundleData> {
    bundles.iter().find(|b| b.sender_email == sender)
}

#[tokio::test]
async fn places_bundle_mail_by_sender_and_say_why() {
    let fx = Fixture::new().await;
    fx.inbound(&fx.account, NEWSLETTER, "Issue 41", Duration::days(2), true)
        .await;
    fx.inbound(
        &fx.account,
        NEWSLETTER,
        "Issue 42",
        Duration::hours(3),
        true,
    )
    .await;
    for (index, hours) in [5, 4, 1].into_iter().enumerate() {
        fx.inbound(
            &fx.account,
            ROBOT,
            &format!("Receipt {index}"),
            Duration::hours(hours),
            true,
        )
        .await;
    }
    fx.inbound(
        &fx.account,
        "maya@orbit.example",
        "Lunch?",
        Duration::hours(2),
        false,
    )
    .await;

    let (reading, reading_total) = fx.place(MailPlaceData::Reading, None).await;
    assert_eq!(reading_total, 2);
    let weekly = bundle(&reading, NEWSLETTER).expect("newsletter bundle");
    assert_eq!(weekly.message_count, 2);
    assert_eq!(weekly.unread_count, 2);
    assert_eq!(weekly.newest_subject, "Issue 42");
    assert_eq!(weekly.kind.kind, SenderKindData::Reading);
    assert_eq!(weekly.kind.reason, "has List-Unsubscribe");
    assert!(!weekly.kind.corrected);
    assert_eq!(
        weekly
            .messages
            .iter()
            .map(|m| m.subject.as_str())
            .collect::<Vec<_>>(),
        vec!["Issue 42", "Issue 41"],
        "newest first"
    );

    let (paper, _) = fx.place(MailPlaceData::PaperTrail, None).await;
    assert_eq!(paper.len(), 1, "people stay out: {paper:?}");
    let shop = &paper[0];
    assert_eq!(shop.sender_email, ROBOT);
    assert_eq!(shop.message_count, 3);
    assert_eq!(shop.kind.rule, KindRuleData::Fallback);
    assert_eq!(shop.kind.reason, "No rule decided; Updates for now");
}

#[tokio::test]
async fn correcting_a_sender_moves_its_mail_and_future_mail_follows() {
    let fx = Fixture::new().await;
    let issue = fx
        .inbound(
            &fx.account,
            NEWSLETTER,
            "Issue 42",
            Duration::hours(3),
            true,
        )
        .await;

    match fx
        .send(Request::SetSenderKind {
            account_id: fx.account.clone(),
            sender_email: "Editor@Weekly.Example".into(),
            kind: Some(SenderKindData::People),
        })
        .await
    {
        ResponseData::SenderKindSet {
            sender_email,
            sender_kind,
            previous,
            ..
        } => {
            assert_eq!(sender_email, NEWSLETTER, "stored lowercased");
            assert_eq!(sender_kind, Some(SenderKindData::People));
            assert_eq!(previous, None, "was automatic");
        }
        other => panic!("expected SenderKindSet, got {other:?}"),
    }
    assert!(fx.place(MailPlaceData::Reading, None).await.0.is_empty());
    match fx
        .send(Request::GetDesk {
            account_id: None,
            lane_limit: 25,
        })
        .await
    {
        ResponseData::Desk {
            people_new, owed, ..
        } => {
            let on_desk = people_new
                .rows
                .iter()
                .chain(&owed.rows)
                .any(|row| row.message_id == issue.id);
            assert!(
                !on_desk,
                "a Messages choice alone must not manufacture debt"
            );
        }
        other => panic!("expected Desk, got {other:?}"),
    }
    let ResponseData::Messages { messages } = fx
        .send(Request::ListMessages {
            account_id: None,
            turn: None,
            filter: mxr_protocol::MessagesFilterData {
                turn: Some(mxr_protocol::MessagesTurnData::All),
                ..Default::default()
            },
            limit: 50,
        })
        .await
    else {
        panic!("messages");
    };
    assert!(
        messages
            .quiet
            .iter()
            .chain(&messages.recent)
            .chain(&messages.pinned)
            .chain(&messages.your_turn)
            .flat_map(|row| &row.topics)
            .any(|topic| topic.message_ids.contains(&issue.id)),
        "the explicit Messages placement remains visible"
    );

    match fx
        .send(Request::SetSenderKind {
            account_id: fx.account.clone(),
            sender_email: NEWSLETTER.into(),
            kind: Some(SenderKindData::PaperTrail),
        })
        .await
    {
        ResponseData::SenderKindSet { previous, .. } => {
            assert_eq!(previous, Some(SenderKindData::People), "for undo");
        }
        other => panic!("expected SenderKindSet, got {other:?}"),
    }
    let next = fx
        .inbound(
            &fx.account,
            NEWSLETTER,
            "Issue 43",
            Duration::hours(1),
            true,
        )
        .await;
    let (paper, _) = fx.place(MailPlaceData::PaperTrail, None).await;
    let moved = bundle(&paper, NEWSLETTER).expect("moved to Paper trail");
    assert_eq!(moved.message_count, 2);
    assert_eq!(moved.kind.reason, "you moved this sender to Paper trail");
    assert!(moved.kind.corrected);

    match fx
        .send(Request::GetMessageKind {
            message_id: next.id.clone(),
        })
        .await
    {
        ResponseData::MessageKind { mail_kind, .. } => {
            assert_eq!(mail_kind.kind, SenderKindData::PaperTrail);
            assert_eq!(mail_kind.rule, KindRuleData::Decision);
        }
        other => panic!("expected MessageKind, got {other:?}"),
    }

    fx.send(Request::SetSenderKind {
        account_id: fx.account.clone(),
        sender_email: NEWSLETTER.into(),
        kind: None,
    })
    .await;
    let (reading, _) = fx.place(MailPlaceData::Reading, None).await;
    assert_eq!(bundle(&reading, NEWSLETTER).unwrap().message_count, 2);

    fx.send(Request::SetSenderKind {
        account_id: fx.account.clone(),
        sender_email: NEWSLETTER.into(),
        kind: Some(SenderKindData::ScreenedOut),
    })
    .await;
    assert!(fx.place(MailPlaceData::Reading, None).await.0.is_empty());
    assert!(fx.place(MailPlaceData::PaperTrail, None).await.0.is_empty());
}

#[tokio::test]
async fn sweep_preview_is_the_sweep_pins_stay_and_undo_restores() {
    let fx = Fixture::new().await;
    let mut receipts = Vec::new();
    for index in 0..4 {
        receipts.push(
            fx.inbound(
                &fx.account,
                ROBOT,
                &format!("Receipt {index}"),
                Duration::hours(10 - index),
                false,
            )
            .await,
        );
    }
    let alerts = fx
        .inbound(
            &fx.account,
            "status@alerts.example.com",
            "Up again",
            Duration::hours(1),
            false,
        )
        .await;
    let issue = fx
        .inbound(
            &fx.account,
            NEWSLETTER,
            "Issue 42",
            Duration::hours(1),
            true,
        )
        .await;

    match fx
        .send(Request::PinMessages {
            message_ids: vec![receipts[1].id.clone()],
            pinned: true,
        })
        .await
    {
        ResponseData::MessagesPinned { changed, pinned } => {
            assert_eq!(changed, 1);
            assert!(pinned);
        }
        other => panic!("expected MessagesPinned, got {other:?}"),
    }
    let (paper, _) = fx.place(MailPlaceData::PaperTrail, None).await;
    let shop = bundle(&paper, ROBOT).unwrap();
    assert_eq!(shop.pinned_count, 1);
    assert!(shop.messages[0].pinned, "pinned first");

    let (preview, job) = fx.sweep(Some(ROBOT), true, None).await;
    assert!(job.is_none());
    assert_eq!(preview.count, 3);
    assert_eq!(preview.pinned_excluded, 1);
    assert_eq!(preview.senders.len(), 1);
    assert_eq!(preview.senders[0].count, 3);
    assert_eq!(
        preview.sample_subjects,
        vec!["Receipt 3", "Receipt 2", "Receipt 0"],
        "newest first, pinned left out"
    );

    let late = fx
        .inbound(
            &fx.account,
            ROBOT,
            "Receipt late",
            Duration::hours(20),
            false,
        )
        .await;

    let (swept, job) = fx
        .sweep(Some(ROBOT), false, preview.preview_token.clone())
        .await;
    assert_eq!(swept.count, preview.count, "same selection");
    assert_eq!(swept.sample_subjects, preview.sample_subjects);
    let job = fx.wait_for_job(&job.expect("an archive job").job_id).await;
    assert_eq!(job.status, JobStatusData::Succeeded, "{job:?}");
    assert_eq!(job.progress.succeeded, 3);

    for (index, receipt) in receipts.iter().enumerate() {
        assert_eq!(
            fx.in_inbox(&receipt.id).await,
            index == 1,
            "receipt {index}"
        );
    }
    assert!(fx.in_inbox(&late.id).await, "arrived after the preview");
    assert!(fx.in_inbox(&alerts.id).await, "another bundle");
    assert!(fx.in_inbox(&issue.id).await, "another place");

    for undo_id in job.undo_ids.iter().rev() {
        fx.send(Request::UndoMutation {
            mutation_id: undo_id.clone(),
        })
        .await;
    }
    for receipt in &receipts {
        assert!(fx.in_inbox(&receipt.id).await);
    }

    let (all, _) = fx.sweep(None, true, None).await;
    assert_eq!(all.count, 5, "3 receipts + late + alerts: {all:?}");
    assert_eq!(all.pinned_excluded, 1);
    assert_eq!(all.senders[0].sender_email, ROBOT, "largest first");
}

#[tokio::test]
async fn places_and_sweeps_keep_to_their_account() {
    let fx = Fixture::new().await;
    let other = mxr_core::Account {
        id: mxr_core::AccountId::new(),
        name: "Other".into(),
        email: "other@example.com".into(),
        sync_backend: None,
        send_backend: None,
        enabled: true,
    };
    fx.state.store.insert_account(&other).await.unwrap();
    fx.inbox_label(&other.id).await;
    let theirs = fx
        .inbound(&other.id, ROBOT, "Their receipt", Duration::hours(1), false)
        .await;
    let mine = fx
        .inbound(&fx.account, ROBOT, "My receipt", Duration::hours(2), false)
        .await;

    let (scoped, _) = fx
        .place(MailPlaceData::PaperTrail, Some(fx.account.clone()))
        .await;
    assert_eq!(scoped.len(), 1);
    assert_eq!(scoped[0].account_id, fx.account);
    let (everywhere, _) = fx.place(MailPlaceData::PaperTrail, None).await;
    assert_eq!(everywhere.len(), 2, "one bundle per account");

    let (preview, _) = fx.sweep(None, true, None).await;
    let (preview, job) = fx.sweep(None, false, preview.preview_token).await;
    assert_eq!(preview.count, 1);
    let job = fx.wait_for_job(&job.unwrap().job_id).await;
    assert_eq!(job.progress.succeeded, 1);
    assert!(!fx.in_inbox(&mine.id).await);
    assert!(
        fx.in_inbox(&theirs.id).await,
        "the other account is untouched"
    );
}

fn paper_trail_scope(fx: &Fixture) -> crate::handler::places::SweepScope {
    crate::handler::places::SweepScope {
        place: MailPlaceData::PaperTrail,
        account_id: Some(fx.account.clone()),
        sender_email: None,
    }
}

#[tokio::test]
async fn a_pin_made_while_a_sweep_runs_keeps_its_message() {
    let fx = Fixture::new().await;
    let mut ids = Vec::new();
    for index in 0..3 {
        ids.push(
            fx.inbound(
                &fx.account,
                ROBOT,
                &format!("Receipt {index}"),
                Duration::minutes(index),
                false,
            )
            .await
            .id,
        );
    }
    let pinned = ids[1].clone();
    let pinning = fx.state.sweep_gate.change([&fx.account]).await;
    let started = super::mutations::start_mutation_job(
        fx.state.clone(),
        MutationCommand::Archive {
            message_ids: ids.clone(),
        },
        None,
        super::mutations::ChunkGuard::Sweep(paper_trail_scope(&fx)),
    )
    .await
    .unwrap();
    let ResponseData::JobStarted { job } = started else {
        panic!("expected a job");
    };
    for id in &ids {
        assert!(fx.in_inbox(id).await);
    }
    fx.state
        .store
        .set_message_pins(std::slice::from_ref(&pinned), true)
        .await
        .unwrap();
    drop(pinning);

    let job = fx.wait_for_job(&job.job_id).await;
    assert_eq!(job.status, JobStatusData::Succeeded, "{job:?}");
    assert_eq!(job.progress.succeeded, 2);
    assert_eq!(job.progress.skipped, 1);
    assert!(fx.in_inbox(&pinned).await);
    assert!(!fx.in_inbox(&ids[0]).await);
    assert!(!fx.in_inbox(&ids[2]).await);
}

#[tokio::test]
async fn a_partly_failed_undo_keeps_what_failed_for_a_retry() {
    let fx = Fixture::new().await;
    let restorable = fx
        .inbound(&fx.account, ROBOT, "Receipt", Duration::hours(1), false)
        .await;
    let offline = mxr_core::Account {
        id: mxr_core::AccountId::new(),
        name: "Offline".into(),
        email: "offline@example.com".into(),
        sync_backend: None,
        send_backend: None,
        enabled: true,
    };
    fx.state.store.insert_account(&offline).await.unwrap();
    fx.inbox_label(&offline.id).await;
    let stuck = fx
        .inbound(&offline.id, ROBOT, "Receipt", Duration::hours(1), false)
        .await;
    let snapshot = |envelope: &Envelope| mxr_store::UndoEntrySnapshot {
        message_id: envelope.id.clone(),
        account_id: envelope.account_id.clone(),
        provider_id: envelope.provider_id.clone(),
        prior_flags_bits: 0,
        prior_label_provider_ids: vec!["INBOX".into()],
        uncertain: false,
    };
    let now = chrono::Utc::now().timestamp();
    fx.state
        .store
        .write_undo_entry(&mxr_store::UndoEntry {
            mutation_id: "undo-partial".into(),
            kind: mxr_store::UndoableMutationKind::Archive,
            snapshots: vec![snapshot(&restorable), snapshot(&stuck)],
            desk: None,
            applied_at: now,
            expires_at: now + 60,
        })
        .await
        .unwrap();

    let message = fx
        .refused(Request::UndoMutation {
            mutation_id: "undo-partial".into(),
        })
        .await;
    assert!(message.contains("restored 1, 1 failed"), "{message}");
    let kept = fx
        .state
        .store
        .read_undo_entry("undo-partial")
        .await
        .unwrap()
        .expect("kept for a retry");
    assert_eq!(kept.snapshots.len(), 1);
    assert_eq!(kept.snapshots[0].message_id, stuck.id);

    let again = fx
        .refused(Request::UndoMutation {
            mutation_id: "undo-partial".into(),
        })
        .await;
    assert!(again.contains("restored 0, 1 failed"), "{again}");
}
