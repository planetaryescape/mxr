use super::*;
use chrono::Duration;
use mxr_core::id::{AccountId, LabelId, MessageId, ThreadId};
use mxr_core::types::{
    Address, BackendRef, Envelope, EventSource, Label, LabelKind, MessageDirection, MessageFlags,
    ProviderKind, SearchMode, SortOrder, UnsubscribeMethod,
};
use mxr_protocol::{JobData, JobStatusData, MutationCommand, SelectionData, SelectionPreviewData};
use mxr_provider_fake::RecordedMutation;
use std::collections::BTreeSet;

pub(super) struct Fixture {
    pub(super) state: Arc<AppState>,
    pub(super) fake: Arc<mxr_provider_fake::FakeProvider>,
    pub(super) account: AccountId,
    _index_dir: tempfile::TempDir,
}

impl Fixture {
    pub(super) async fn new() -> Self {
        Self::with_config(mxr_config::MxrConfig::default()).await
    }

    pub(super) async fn with_config(config: mxr_config::MxrConfig) -> Self {
        let index_dir = tempfile::tempdir().unwrap();
        let index = mxr_search::SearchIndex::open(index_dir.path()).unwrap();
        let (state, fake) = AppState::in_memory_with_fake_index(config, index)
            .await
            .unwrap();
        let state = Arc::new(state);
        let account = state.default_account_id();
        let fx = Self {
            state,
            fake,
            account,
            _index_dir: index_dir,
        };
        fx.label(&fx.account.clone(), "INBOX", "Inbox").await;
        fx
    }

    pub(super) async fn label(
        &self,
        account: &AccountId,
        provider_id: &str,
        name: &str,
    ) -> LabelId {
        let kind = if provider_id.starts_with("Label_") {
            LabelKind::User
        } else {
            LabelKind::System
        };
        self.label_of(account, provider_id, name, kind).await
    }

    pub(super) async fn label_of(
        &self,
        account: &AccountId,
        provider_id: &str,
        name: &str,
        kind: LabelKind,
    ) -> LabelId {
        let id = LabelId::from_scoped_provider_id(account, "fake", provider_id);
        self.state
            .store
            .upsert_label(&Label {
                id: id.clone(),
                account_id: account.clone(),
                name: name.into(),
                kind,
                color: None,
                provider_id: provider_id.into(),
                unread_count: 0,
                total_count: 0,
                role: None,
            })
            .await
            .unwrap();
        id
    }

    pub(super) async fn other_account(&self, enabled: bool) -> AccountId {
        let id = AccountId::new();
        self.state
            .store
            .insert_account(&mxr_core::Account {
                id: id.clone(),
                name: if enabled { "Other" } else { "Hidden" }.to_string(),
                email: "other@example.com".to_string(),
                sync_backend: Some(BackendRef {
                    provider_kind: ProviderKind::Fake,
                    config_key: "fake".to_string(),
                }),
                send_backend: None,
                enabled,
            })
            .await
            .unwrap();
        self.label(&id, "INBOX", "Inbox").await;
        id
    }

    pub(super) async fn many(&self, n: usize, subject: &str) -> Vec<Envelope> {
        let mut made = Vec::with_capacity(n);
        for index in 0..n {
            made.push(
                self.store_message(
                    &self.account,
                    &format!("{subject} {index}"),
                    index as i64 + 1,
                    MessageFlags::empty(),
                    &[],
                )
                .await,
            );
        }
        let refs: Vec<&Envelope> = made.iter().collect();
        self.index(&refs).await;
        made
    }

    pub(super) async fn message(
        &self,
        account: &AccountId,
        subject: &str,
        minutes_ago: i64,
    ) -> Envelope {
        self.message_with(account, subject, minutes_ago, MessageFlags::empty(), &[])
            .await
    }

    pub(super) async fn message_with(
        &self,
        account: &AccountId,
        subject: &str,
        minutes_ago: i64,
        flags: MessageFlags,
        extra_labels: &[LabelId],
    ) -> Envelope {
        let envelope = self
            .store_message(account, subject, minutes_ago, flags, extra_labels)
            .await;
        self.index(&[&envelope]).await;
        envelope
    }

    pub(super) async fn store_message(
        &self,
        account: &AccountId,
        subject: &str,
        minutes_ago: i64,
        flags: MessageFlags,
        extra_labels: &[LabelId],
    ) -> Envelope {
        let id = MessageId::new();
        let envelope = Envelope {
            id: id.clone(),
            account_id: account.clone(),
            provider_id: format!("sel-{id}"),
            thread_id: ThreadId::new(),
            message_id_header: Some(format!("<{id}@example.com>")),
            in_reply_to: None,
            references: vec![],
            from: Address {
                name: Some("Alice".into()),
                email: "alice@example.com".into(),
            },
            to: vec![Address {
                name: None,
                email: "user@example.com".into(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: subject.into(),
            date: chrono::Utc::now() - Duration::minutes(minutes_ago),
            flags,
            snippet: subject.into(),
            has_attachments: false,
            size_bytes: 10,
            unsubscribe: UnsubscribeMethod::None,
            link_count: 0,
            body_word_count: 0,
            label_provider_ids: vec![],
            keywords: BTreeSet::new(),
        };
        self.state
            .store
            .upsert_envelope_with_direction(&envelope, MessageDirection::Inbound)
            .await
            .unwrap();
        let inbox = LabelId::from_scoped_provider_id(account, "fake", "INBOX");
        let mut labels = vec![inbox];
        labels.extend(extra_labels.iter().cloned());
        self.state
            .store
            .set_message_labels(&id, &labels, EventSource::User)
            .await
            .unwrap();
        envelope
    }

    pub(super) async fn index(&self, envelopes: &[&Envelope]) {
        let ids: Vec<MessageId> = envelopes.iter().map(|e| e.id.clone()).collect();
        super::super::mutations::reindex_messages_in_search(&self.state, &ids)
            .await
            .unwrap();
    }

    pub(super) async fn send(&self, request: Request) -> ResponseData {
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

    pub(super) async fn send_raw(&self, request: Request) -> Response {
        let msg = IpcMessage {
            expected_workspace: None,
            id: 1,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(request),
        };
        match handle_request(&self.state, &msg).await.payload {
            IpcPayload::Response(response) => response,
            other => panic!("expected a response, got {other:?}"),
        }
    }

    pub(super) async fn refused(&self, request: Request) -> String {
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

    pub(super) async fn paged_search(
        &self,
        query: &str,
        account_id: Option<&AccountId>,
        page: u32,
    ) -> Vec<MessageId> {
        let mut found = Vec::new();
        let mut offset = 0;
        loop {
            let ResponseData::SearchResults {
                results, has_more, ..
            } = self
                .send(Request::Search {
                    query: query.into(),
                    limit: page,
                    offset,
                    account_id: account_id.cloned(),
                    mode: Some(SearchMode::Lexical),
                    sort: Some(SortOrder::DateDesc),
                    explain: false,
                    limit_from_config: false,
                })
                .await
            else {
                panic!("expected search results");
            };
            found.extend(results.into_iter().map(|r| r.message_id));
            if !has_more {
                return found;
            }
            offset += page;
        }
    }

    pub(super) async fn resolved(
        &self,
        query: &str,
        account_id: Option<&AccountId>,
    ) -> Vec<MessageId> {
        super::super::diagnostics_impl::search_resolve::resolve_search_selection(
            &self.state,
            query,
            account_id,
        )
        .await
        .unwrap()
        .ids
    }

    pub(super) fn selection(query: &str, account: Option<&AccountId>) -> SelectionData {
        SelectionData::Search {
            query: query.into(),
            account_id: account.cloned(),
        }
    }

    pub(super) async fn preview(
        &self,
        query: &str,
        mutation: &MutationCommand,
    ) -> SelectionPreviewData {
        let ResponseData::SelectionPreview { preview } = self
            .send(Request::PreviewSelection {
                selection: Self::selection(query, Some(&self.account)),
                mutation: mutation.clone(),
                sample_size: None,
            })
            .await
        else {
            panic!("expected a selection preview");
        };
        preview
    }

    pub(super) fn start_request(
        &self,
        job_id: &str,
        query: &str,
        token: &str,
        mutation: &MutationCommand,
    ) -> Request {
        Request::StartSelectionJob {
            job_id: job_id.into(),
            preview_token: token.into(),
            selection: Self::selection(query, Some(&self.account)),
            mutation: mutation.clone(),
            client_correlation_id: None,
        }
    }

    pub(super) async fn start(
        &self,
        query: &str,
        token: &str,
        mutation: &MutationCommand,
    ) -> JobData {
        let job_id = uuid::Uuid::now_v7().to_string();
        let ResponseData::JobStarted { job } = self
            .send(self.start_request(&job_id, query, token, mutation))
            .await
        else {
            panic!("expected a started job");
        };
        job
    }

    pub(super) async fn job(&self, job_id: &str) -> JobData {
        match self
            .send(Request::GetJob {
                job_id: job_id.into(),
            })
            .await
        {
            ResponseData::Job { job } => job,
            other => panic!("expected a job, got {other:?}"),
        }
    }

    pub(super) async fn wait_for_job(&self, job_id: &str) -> JobData {
        for _ in 0..6000 {
            let job = self.job(job_id).await;
            if !matches!(job.status, JobStatusData::Queued | JobStatusData::Running) {
                return job;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        panic!("job {job_id} did not finish");
    }

    pub(super) fn archived_provider_ids(&self) -> Vec<String> {
        self.fake
            .mutations()
            .into_iter()
            .filter_map(|mutation| match mutation {
                RecordedMutation::LabelsModified {
                    provider_id,
                    removed,
                    ..
                } if removed.iter().any(|label| label == "INBOX") => Some(provider_id),
                _ => None,
            })
            .collect()
    }

    pub(super) async fn delete_from_store(&self, envelope: &Envelope) {
        self.state
            .store
            .delete_messages_and_derived(
                &envelope.account_id,
                std::slice::from_ref(&envelope.provider_id),
            )
            .await
            .unwrap();
    }

    pub(super) async fn in_inbox(&self, id: &MessageId) -> bool {
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

pub(super) fn archive() -> MutationCommand {
    MutationCommand::Archive {
        message_ids: Vec::new(),
    }
}

#[tokio::test]
async fn the_resolver_lists_what_paged_search_lists() {
    let fx = Fixture::new().await;
    let work = fx.label(&fx.account, "Label_work", "Work").await;
    let other = fx.other_account(true).await;
    let hidden = fx.other_account(false).await;

    for n in 0..9 {
        fx.message(&fx.account, &format!("report {n}"), 100 + n)
            .await;
    }
    fx.message_with(
        &fx.account,
        "report labelled",
        50,
        MessageFlags::empty(),
        std::slice::from_ref(&work),
    )
    .await;
    fx.message_with(&fx.account, "report trashed", 40, MessageFlags::TRASH, &[])
        .await;
    fx.message_with(&fx.account, "report spam", 30, MessageFlags::SPAM, &[])
        .await;
    for n in 0..3 {
        fx.message(&other, &format!("report other {n}"), 20 + n)
            .await;
    }
    for n in 0..3 {
        fx.message(&hidden, &format!("report hidden {n}"), 10 + n)
            .await;
    }
    let ghost = fx
        .store_message(&fx.account, "report ghost", 5, MessageFlags::empty(), &[])
        .await;
    fx.index(&[&ghost]).await;
    fx.delete_from_store(&ghost).await;

    let cases: Vec<(&str, Option<AccountId>)> = vec![
        ("report", None),
        ("report", Some(fx.account.clone())),
        ("report", Some(other.clone())),
        ("report", Some(hidden.clone())),
        ("label:Work", None),
        ("label:Work report", Some(fx.account.clone())),
        ("in:trash report", None),
        ("in:spam report", None),
        ("in:anywhere report", None),
        ("subject:ghost", None),
        ("nomatchforthis", None),
    ];
    for (query, account) in cases {
        let resolved = fx.resolved(query, account.as_ref()).await;
        let paged = fx.paged_search(query, account.as_ref(), 4).await;
        assert_eq!(
            resolved, paged,
            "`{query}` for {account:?}: resolver and paged search disagree"
        );
    }

    let message = fx
        .refused(Request::Search {
            query: "report OR".into(),
            limit: 10,
            offset: 0,
            account_id: None,
            mode: Some(SearchMode::Lexical),
            sort: Some(SortOrder::DateDesc),
            explain: false,
            limit_from_config: false,
        })
        .await;
    assert!(message.contains("Syntax Error"), "{message}");
    assert!(
        super::super::diagnostics_impl::search_resolve::resolve_search_selection(
            &fx.state,
            "report OR",
            None
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("Syntax Error")
    );

    let everything = fx.resolved("report", None).await;
    let account_live = fx.resolved("report", Some(&fx.account)).await;
    assert_eq!(
        account_live.len(),
        10,
        "nine inbox mail and the labelled one"
    );
    assert_eq!(
        everything.len(),
        13,
        "plus the other visible account's three"
    );
    assert!(!everything.contains(&ghost.id));
    assert!(fx.resolved("report", Some(&hidden)).await.is_empty());
    assert_eq!(fx.resolved("in:anywhere report", None).await.len(), 15);
}

#[tokio::test]
async fn a_selection_by_owed_reply_covers_the_whole_set_not_one_page() {
    let fx = Fixture::new().await;
    let mut owed = Vec::new();
    for n in 0..30 {
        let envelope = fx
            .store_message(
                &fx.account,
                &format!("are we on {n}"),
                60 * 24 * 10 + n,
                MessageFlags::empty(),
                &[],
            )
            .await;
        owed.push(envelope);
    }
    let answered = fx
        .store_message(
            &fx.account,
            "answered",
            60 * 24 * 12,
            MessageFlags::empty(),
            &[],
        )
        .await;
    let mut reply = fx
        .store_message(
            &fx.account,
            "re: answered",
            60 * 24 * 11,
            MessageFlags::empty(),
            &[],
        )
        .await;
    reply.thread_id = answered.thread_id.clone();
    reply.from = Address {
        name: None,
        email: "user@example.com".into(),
    };
    fx.state
        .store
        .upsert_envelope_with_direction(&reply, MessageDirection::Outbound)
        .await
        .unwrap();
    let mut all: Vec<&Envelope> = owed.iter().collect();
    all.push(&answered);
    all.push(&reply);
    fx.index(&all).await;
    fx.state.store.refresh_contacts().await.unwrap();

    let listed: BTreeSet<String> = fx
        .state
        .store
        .list_owed_replies(&fx.account, None, None, 1000)
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.thread_id.as_str())
        .collect();
    assert!(listed.len() >= 30, "setup: {} owed threads", listed.len());

    let resolved = fx.resolved("is:owed-reply", None).await;
    let threads: BTreeSet<String> = {
        let mut threads = BTreeSet::new();
        for id in &resolved {
            let envelope = fx.state.store.get_envelope(id).await.unwrap().unwrap();
            threads.insert(envelope.thread_id.as_str());
        }
        threads
    };
    assert_eq!(threads, listed, "every owed thread, and nothing else");
    assert!(resolved.len() >= 30);
}

#[tokio::test]
async fn a_preview_counts_and_samples_without_listing_ids() {
    let fx = Fixture::new().await;
    let msgs = fx.many(30, "invoice").await;
    let other = fx.other_account(true).await;
    fx.message(&other, "invoice elsewhere", 500).await;

    let preview = fx.preview("invoice", &archive()).await;
    assert_eq!(preview.count, 30, "scoped to the account the request names");
    assert_eq!(preview.threads, 30);
    assert_eq!(preview.accounts.len(), 1);
    assert_eq!(preview.accounts[0].account_id, fx.account);
    assert_eq!(preview.accounts[0].account_name, "Fake Account");
    assert_eq!(preview.accounts[0].count, 30);
    assert_eq!(preview.sample_size, 20);
    assert_eq!(preview.sample.len(), 20);
    assert_eq!(preview.sample[0].message_id, msgs[0].id, "newest first");
    assert_eq!(preview.sample[19].message_id, msgs[19].id);
    assert_eq!(preview.sample[0].from, "Alice");
    assert_eq!(preview.sample[0].subject, "invoice 0");
    assert!(!preview.capped);
    assert!(!preview.preview_token.is_empty());
    assert!(preview.expires_at > chrono::Utc::now().timestamp_millis());

    let ResponseData::SelectionPreview { preview: all } = fx
        .send(Request::PreviewSelection {
            selection: Fixture::selection("invoice", None),
            mutation: archive(),
            sample_size: Some(3),
        })
        .await
    else {
        panic!("expected a preview");
    };
    assert_eq!(all.count, 31);
    assert_eq!(all.accounts.len(), 2);
    assert_eq!(all.sample.len(), 3);
    assert_eq!(all.sample_size, 3);

    let wire = serde_json::to_vec(&ResponseData::SelectionPreview { preview }).unwrap();
    assert!(wire.len() < 20_000, "{} bytes", wire.len());
}

#[tokio::test]
async fn the_job_acts_on_what_the_preview_counted_and_nothing_else() {
    let fx = Fixture::new().await;
    let msgs = fx.many(5, "invoice").await;
    let preview = fx.preview("invoice", &archive()).await;
    assert_eq!(preview.count, 5);

    let late = fx.message(&fx.account, "invoice late", 0).await;
    fx.delete_from_store(&msgs[2]).await;

    let started = fx
        .start("invoice", &preview.preview_token, &archive())
        .await;
    let accounts = &started.result.as_ref().unwrap().accounts;
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].account_id, fx.account);
    assert_eq!(accounts[0].account_name, "Fake Account");
    assert!(started.selection.is_some());
    assert_eq!(started.progress.total, 5);

    let job = fx.wait_for_job(&started.job_id).await;
    assert_eq!(job.status, JobStatusData::Succeeded, "{job:?}");
    assert_eq!(job.error, None);
    assert_eq!(
        (
            job.progress.total,
            job.progress.succeeded,
            job.progress.skipped,
            job.progress.failed
        ),
        (5, 4, 1, 0),
        "the deleted message is skipped, not a reason to stop"
    );
    assert_eq!(
        fx.archived_provider_ids(),
        vec![
            msgs[0].provider_id.clone(),
            msgs[1].provider_id.clone(),
            msgs[3].provider_id.clone(),
            msgs[4].provider_id.clone(),
        ],
        "exactly the previewed messages that still exist, newest first"
    );
    assert!(
        fx.in_inbox(&late.id).await,
        "mail after the preview is left alone"
    );
    assert_eq!(job.result.as_ref().unwrap().accounts[0].succeeded, 4);
}

fn edit_labels(add: &[&str], remove: &[&str]) -> MutationCommand {
    MutationCommand::ModifyLabels {
        message_ids: Vec::new(),
        add: add.iter().map(ToString::to_string).collect(),
        remove: remove.iter().map(ToString::to_string).collect(),
    }
}

#[tokio::test]
async fn a_label_edit_may_only_name_the_users_own_labels() {
    let fx = Fixture::new().await;
    fx.label_of(&fx.account, "Deleted Items", "TRASH", LabelKind::System)
        .await;
    fx.label_of(&fx.account, "Clients", "Clients", LabelKind::Folder)
        .await;
    fx.label(&fx.account, "Label_work", "Work").await;
    let msgs = fx.many(3, "invoice").await;

    let refused = |text: String| {
        assert!(
            text.contains("only work on labels you created")
                || text.contains("name the label to add or remove"),
            "{text}"
        );
        text
    };
    for (add, remove) in [
        (&["TRASH"][..], &[][..]),
        (&["SPAM"], &[]),
        (&["INBOX"], &[]),
        (&["Inbox"], &[]),
        (&["Deleted Items"], &[]),
        (&["Work", "TRASH"], &[]),
        (&["trash"], &[]),
        (&["DELETED"], &[]),
        (&["\\Deleted"], &[]),
        (&["Nope"], &[]),
        (&["Clients"], &[]),
        (&[], &["TRASH"]),
        (&[], &["INBOX"]),
        (&["Work"], &["Deleted Items"]),
        (&[], &[]),
    ] {
        let mutation = edit_labels(add, remove);
        let preview = fx
            .refused(Request::PreviewSelection {
                selection: Fixture::selection("invoice", Some(&fx.account)),
                mutation: mutation.clone(),
                sample_size: None,
            })
            .await;
        refused(preview);
        let start = fx
            .refused(fx.start_request(
                &uuid::Uuid::now_v7().to_string(),
                "invoice",
                "any-token",
                &mutation,
            ))
            .await;
        assert!(
            start.contains("expired or was already used")
                || start.contains("name the label to add or remove"),
            "{start}"
        );
    }
    assert!(fx.fake.mutations().is_empty());
    for message in &msgs {
        assert!(fx.in_inbox(&message.id).await);
    }

    assert_eq!(
        fx.preview("invoice", &edit_labels(&["Work"], &[]))
            .await
            .count,
        3
    );
    assert_eq!(
        fx.preview("invoice", &edit_labels(&[], &["Work"]))
            .await
            .count,
        3
    );
    assert_eq!(
        fx.preview("invoice", &edit_labels(&["Label_work"], &["Work"]))
            .await
            .count,
        3
    );
}

#[tokio::test]
async fn a_token_is_single_use_and_for_one_selection_and_action() {
    let fx = Fixture::new().await;
    fx.label(&fx.account, "Label_work", "Work").await;
    fx.many(3, "note").await;
    let preview = fx.preview("note", &archive()).await;
    let token = preview.preview_token;
    let new_id = || uuid::Uuid::now_v7().to_string();

    let label = MutationCommand::ModifyLabels {
        message_ids: Vec::new(),
        add: vec!["Work".into()],
        remove: Vec::new(),
    };
    for (query, mutation) in [("note", &label), ("different", &archive())] {
        let refusal = fx
            .refused(fx.start_request(&new_id(), query, &token, mutation))
            .await;
        assert!(
            refusal.contains("different selection or action"),
            "{refusal}"
        );
    }
    let refusal = fx
        .refused(fx.start_request(&new_id(), "note", "not-a-token", &archive()))
        .await;
    assert!(refusal.contains("expired or was already used"), "{refusal}");

    let job = fx.start("note", &token, &archive()).await;
    fx.wait_for_job(&job.job_id).await;
    let again = fx
        .refused(fx.start_request(&new_id(), "note", &token, &archive()))
        .await;
    assert!(again.contains("expired or was already used"), "{again}");
    assert_eq!(fx.archived_provider_ids().len(), 3);
}

#[tokio::test]
async fn a_failing_message_ends_the_job_with_exact_counts_and_its_details() {
    let fx = Fixture::new().await;
    let msgs = fx.many(250, "invoice").await;
    let preview = fx.preview("invoice", &archive()).await;
    fx.fake.fail_mutations_of(Some(&msgs[150].provider_id));
    let started = fx
        .start("invoice", &preview.preview_token, &archive())
        .await;
    let job = fx.wait_for_job(&started.job_id).await;

    assert_eq!(job.status, JobStatusData::Failed, "{job:?}");
    assert!(job
        .error
        .as_deref()
        .unwrap()
        .contains("stopped after partial progress: 150 succeeded, 49 skipped, 1 failed"));
    assert_eq!(
        (
            job.progress.total,
            job.progress.completed,
            job.progress.succeeded,
            job.progress.skipped,
            job.progress.failed
        ),
        (250, 200, 150, 49, 1)
    );
    assert_eq!(job.failures.len(), 1);
    assert_eq!(job.failures[0].message_id, msgs[150].id);
    assert!(job.failures[0].error.contains("fake: mutation failed"));
    assert!(job.undo_ids.is_empty(), "an --all job keeps no undo");
    assert_eq!(job.result.as_ref().unwrap().accounts[0].failed, 1);

    assert_eq!(fx.archived_provider_ids().len(), 150);
    for later in &msgs[200..] {
        assert!(fx.in_inbox(&later.id).await);
    }
    for skipped in &msgs[151..200] {
        assert!(fx.in_inbox(&skipped.id).await);
    }
}

#[tokio::test]
async fn a_rate_limit_pauses_the_job_and_it_resumes_without_repeating_a_call() {
    let fx = Fixture::new().await;
    let msgs = fx.many(150, "invoice").await;
    let preview = fx.preview("invoice", &archive()).await;
    fx.fake.rate_limit_next_mutations(3, 30);
    let started = fx
        .start("invoice", &preview.preview_token, &archive())
        .await;

    let mut paused_until = None;
    for _ in 0..1000 {
        let job = fx.job(&started.job_id).await;
        if job.paused_until.is_some() {
            paused_until = job.paused_until;
            assert_eq!(job.status, JobStatusData::Running);
            break;
        }
        if job.status == JobStatusData::Succeeded {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let paused_until = paused_until.expect("the job showed when it would resume");
    assert!(paused_until > chrono::Utc::now().timestamp_millis() - 1_000);

    let job = fx.wait_for_job(&started.job_id).await;
    assert_eq!(job.status, JobStatusData::Succeeded, "{job:?}");
    assert_eq!(job.progress.succeeded, 150);
    assert_eq!(job.progress.failed, 0);
    assert_eq!(job.paused_until, None);
    let recorded = fx.fake.mutations();
    assert_eq!(recorded.len(), 150, "a refused call is not a call");
    let mut provider_ids = fx.archived_provider_ids();
    provider_ids.sort();
    provider_ids.dedup();
    assert_eq!(provider_ids.len(), 150, "no message twice");
    for message in &msgs {
        assert!(!fx.in_inbox(&message.id).await);
    }
}

#[tokio::test]
async fn a_restart_marks_a_running_job_interrupted_with_the_counts_it_had() {
    let fx = Fixture::new().await;
    let reached = fx.many(100, "alpha").await;
    let never_reached = fx.many(105, "beta").await;

    let preview = fx.preview("alpha", &archive()).await;
    let started = fx.start("alpha", &preview.preview_token, &archive()).await;
    let done = fx.wait_for_job(&started.job_id).await;
    assert_eq!(done.status, JobStatusData::Succeeded);

    let mut running = done.clone();
    running.status = JobStatusData::Running;
    running.finished_at = None;
    running.error = None;
    running.progress.total = 205;
    super::super::mutations::persist_job(&fx.state, &running).await;

    let marked = super::super::job_recovery::mark_interrupted_mutation_jobs(&fx.state).await;
    assert_eq!(marked, 1);
    let job = fx.job(&started.job_id).await;
    assert_eq!(job.status, JobStatusData::Failed);
    assert!(
        job.error.as_deref().unwrap().contains("interrupted"),
        "{job:?}"
    );
    assert_eq!(
        (
            job.progress.completed,
            job.progress.succeeded,
            job.progress.total
        ),
        (100, 100, 205),
        "the counts the job had persisted, not guessed"
    );
    assert!(job.finished_at.is_some());

    assert_eq!(
        super::super::job_recovery::mark_interrupted_mutation_jobs(&fx.state).await,
        0
    );

    let preview = fx.preview("in:inbox", &archive()).await;
    assert_eq!(preview.count, 105);
    let second = fx
        .start("in:inbox", &preview.preview_token, &archive())
        .await;
    let second = fx.wait_for_job(&second.job_id).await;
    assert_eq!(second.progress.succeeded, 105);
    for message in reached.iter().chain(&never_reached) {
        assert!(!fx.in_inbox(&message.id).await);
    }
}

impl Fixture {
    pub(super) async fn archive_job(&self, query: &str) -> JobData {
        let preview = self.preview(query, &archive()).await;
        let started = self.start(query, &preview.preview_token, &archive()).await;
        let job = self.wait_for_job(&started.job_id).await;
        assert_eq!(job.status, JobStatusData::Succeeded, "{job:?}");
        job
    }

    pub(super) async fn undo_entries(&self) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM mutation_undo_log")
            .fetch_one(self.state.store.reader())
            .await
            .unwrap()
    }
}

#[tokio::test]
async fn an_all_job_writes_no_undo_entries() {
    let fx = Fixture::new().await;
    fx.label(&fx.account, "Label_work", "Work").await;
    fx.many(250, "invoice").await;
    fx.many(3, "receipt").await;
    let jobs = [
        ("invoice", edit_labels(&["Work"], &[])),
        ("invoice", edit_labels(&[], &["Work"])),
        ("invoice", archive()),
    ];
    for (query, mutation) in jobs {
        let preview = fx.preview(query, &mutation).await;
        let started = fx.start(query, &preview.preview_token, &mutation).await;
        let job = fx.wait_for_job(&started.job_id).await;
        assert_eq!(job.status, JobStatusData::Succeeded, "{job:?}");
        assert_eq!(job.progress.succeeded, 250);
        assert!(job.undo_ids.is_empty(), "{job:?}");
        assert_eq!(job.undo_id_count, 0);
        assert!(job.result.as_ref().unwrap().mutation_id.is_none());
    }
    let read_archive = MutationCommand::ReadAndArchive {
        message_ids: Vec::new(),
    };
    let preview = fx.preview("receipt", &read_archive).await;
    fx.fake.rate_limit_mutations_after(1, 1, 5);
    let started = fx
        .start("receipt", &preview.preview_token, &read_archive)
        .await;
    let job = fx.wait_for_job(&started.job_id).await;
    assert_eq!(job.status, JobStatusData::Succeeded, "{job:?}");
    assert!(job.undo_ids.is_empty());
    assert_eq!(fx.undo_entries().await, 0, "an --all job writes no undo");

    let mail = fx.many(1, "letter").await;
    let ResponseData::MutationResult { result } = fx
        .send(Request::mutation(MutationCommand::Archive {
            message_ids: vec![mail[0].id.clone()],
        }))
        .await
    else {
        panic!("expected a mutation result");
    };
    assert!(result.mutation_id.is_some());
    assert_eq!(fx.undo_entries().await, 1);
}

#[tokio::test]
async fn a_scoped_profile_reaches_only_selections_of_its_own_account() {
    use mxr_config::SafetyPolicy;
    let fx = Fixture::new().await;
    let other = fx.other_account(true).await;
    fx.many(3, "invoice").await;
    fx.message(&other, "invoice other", 5).await;

    let mut config = fx.state.config_snapshot();
    config.agent_surfaces.profiles.insert(
        "mcp".into(),
        mxr_config::AgentProfileConfig {
            safety_policy: SafetyPolicy::Full,
            allowed_accounts: vec![fx.account.as_str()],
            allow_send: false,
            allow_destructive: true,
            allowed_destructive_actions: vec![],
        },
    );
    fx.state.set_config_for_test(config.clone()).await;
    let check = |request: Request| {
        let state = fx.state.clone();
        let config = config.clone();
        async move {
            enforce_client_profile(&state, &config, ::mxr_protocol::ClientKind::Mcp, &request).await
        }
    };
    let preview = |account: Option<&AccountId>| Request::PreviewSelection {
        selection: Fixture::selection("invoice", account),
        mutation: archive(),
        sample_size: None,
    };
    let start = |account: Option<&AccountId>| Request::StartSelectionJob {
        job_id: uuid::Uuid::now_v7().to_string(),
        preview_token: "x".into(),
        selection: Fixture::selection("invoice", account),
        mutation: archive(),
        client_correlation_id: None,
    };

    assert!(check(preview(None)).await.is_err());
    assert!(check(start(None)).await.is_err());
    assert!(check(preview(Some(&other))).await.is_err());
    assert!(check(start(Some(&other))).await.is_err());
    check(preview(Some(&fx.account))).await.unwrap();
    check(start(Some(&fx.account))).await.unwrap();

    let finished = fx.archive_job("invoice").await;
    check(Request::GetJob {
        job_id: finished.job_id.clone(),
    })
    .await
    .unwrap();

    assert!(enforce_safety_policy(SafetyPolicy::ReadOnly, &preview(Some(&fx.account))).is_ok());
    assert!(enforce_safety_policy(SafetyPolicy::ReadOnly, &start(Some(&fx.account))).is_err());
}

async fn job_activity(fx: &Fixture) -> Vec<mxr_store::ActivityRow> {
    crate::activity::settle(&fx.state.activity).await;
    fx.state
        .store
        .list_activity(
            &mxr_store::ActivityFilter {
                target_kind: Some("job".into()),
                ..Default::default()
            },
            100,
            None,
        )
        .await
        .unwrap()
        .rows
}

#[tokio::test]
async fn a_job_records_one_accepted_and_one_finished_entry_without_ids() {
    let fx = Fixture::new().await;
    let msgs = fx.many(5, "invoice").await;
    let job = fx.archive_job("invoice").await;
    let rows = job_activity(&fx).await;
    assert_eq!(rows.len(), 2, "{rows:?}");
    let contexts: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| serde_json::from_str(row.context_json.as_deref().unwrap()).unwrap())
        .collect();
    let phase = |name: &str| contexts.iter().find(|c| c["phase"] == name).unwrap();
    assert_eq!(phase("accepted")["count"], 5);
    assert_eq!(phase("accepted")["query"], "invoice");
    assert_eq!(phase("finished")["succeeded"], 5);
    assert_eq!(phase("finished")["failed"], 0);
    for (row, context) in rows.iter().zip(&contexts) {
        assert_eq!(row.action, "mail.archive");
        assert_eq!(row.target_id.as_deref(), Some(job.job_id.as_str()));
        assert_eq!(context["job_id"], job.job_id.as_str());
        let text = row.context_json.as_deref().unwrap();
        assert!(!text.contains("target_ids"));
        for message in &msgs {
            assert!(!text.contains(&message.id.as_str()), "{text}");
        }
    }
}

#[tokio::test]
async fn a_job_leaves_out_the_query_when_search_queries_are_not_tracked() {
    let mut config = mxr_config::MxrConfig::default();
    config.activity.track_search_queries = false;
    let fx = Fixture::with_config(config).await;
    fx.many(2, "invoice").await;
    fx.archive_job("invoice").await;
    let rows = job_activity(&fx).await;
    assert_eq!(rows.len(), 2);
    for row in rows {
        assert!(!row.context_json.unwrap().contains("invoice"));
    }
}

#[tokio::test]
async fn a_job_records_nothing_under_the_activity_kill_switch() {
    const CHILD: &str = "MXR_TEST_SELECTION_ACTIVITY_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "handler::tests::selection_jobs::a_job_records_nothing_under_the_activity_kill_switch",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env("MXR_ACTIVITY", "off")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let fx = Fixture::new().await;
    assert!(!fx.state.activity.is_enabled());
    fx.many(2, "invoice").await;
    fx.archive_job("invoice").await;
    assert!(job_activity(&fx).await.is_empty());
}

#[tokio::test]
async fn one_token_starts_one_job_whatever_the_job_ids() {
    let fx = Fixture::new().await;
    fx.many(150, "invoice").await;
    let token = fx.preview("invoice", &archive()).await.preview_token;
    let (j, k) = (
        uuid::Uuid::now_v7().to_string(),
        uuid::Uuid::now_v7().to_string(),
    );
    let (a, b) = tokio::join!(
        fx.send_raw(fx.start_request(&j, "invoice", &token, &archive())),
        fx.send_raw(fx.start_request(&k, "invoice", &token, &archive())),
    );
    let mut started = Vec::new();
    for (id, reply) in [(&j, a), (&k, b)] {
        match reply {
            Response::Ok {
                data: ResponseData::JobStarted { job },
            } => {
                assert_eq!(&job.job_id, id);
                started.push(job.job_id);
            }
            Response::Error { message, .. } => {
                assert!(message.contains("expired or was already used"), "{message}");
                assert!(
                    fx.state.store.get_mutation_job(id).await.unwrap().is_none(),
                    "a refused start leaves no job behind"
                );
            }
            other => panic!("unexpected reply {other:?}"),
        }
    }
    assert_eq!(started.len(), 1, "exactly one start won");
    let job = fx.wait_for_job(&started[0]).await;
    assert_eq!(job.progress.succeeded, 150);
    let mut archived = fx.archived_provider_ids();
    assert_eq!(archived.len(), 150);
    archived.sort();
    archived.dedup();
    assert_eq!(archived.len(), 150);
}
