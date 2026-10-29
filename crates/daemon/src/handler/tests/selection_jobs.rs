//! `--all`: a bulk action over a selection the daemon resolves, previews and
//! runs as one job. Everything here goes through the real handler, a Tantivy
//! index in a temp directory, and the fake provider's recorded mutations.

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
        let id = LabelId::from_scoped_provider_id(account, "fake", provider_id);
        self.state
            .store
            .upsert_label(&Label {
                id: id.clone(),
                account_id: account.clone(),
                name: name.into(),
                kind: if provider_id.starts_with("Label_") {
                    LabelKind::User
                } else {
                    LabelKind::System
                },
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

    /// A second account the fake provider does not serve: its mail can be
    /// found and listed, but not changed.
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

    /// `n` inbox messages whose subject starts with `subject`, newest first.
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

    /// Store and index one message in `account`'s inbox. `minutes_ago` keeps
    /// the dates distinct, so "newest first" has one answer.
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

    /// In the store with its labels, not yet in the index.
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

    /// Put these stored messages in the search index, as sync would.
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

    /// Every id a search lists, a page at a time, the way a client paging
    /// `Request::Search` sees them: lexical, newest first.
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

    /// Waits for the job to finish. Only a deadline, never an ordering.
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

    /// Provider ids the fake archived (removed INBOX from), in call order.
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

    /// Drop a message from the store and leave it in the search index, the
    /// way a sync that deletes mail does until the index catches up.
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

fn ids(envelopes: &[&Envelope]) -> Vec<MessageId> {
    envelopes.iter().map(|e| e.id.clone()).collect()
}

// ---------------------------------------------------------------------------
// The resolver
// ---------------------------------------------------------------------------

/// The resolver returns the set a client paging `Request::Search` would see
/// end to end, across everything the live pager filters.
#[tokio::test]
async fn the_resolver_lists_what_paged_search_lists() {
    let fx = Fixture::new().await;
    let work = fx.label(&fx.account, "Label_work", "Work").await;
    let other = fx.other_account(true).await;
    let hidden = fx.other_account(false).await;

    // Live mail across the account's states.
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
    // Another visible account, and one that is not visible.
    for n in 0..3 {
        fx.message(&other, &format!("report other {n}"), 20 + n)
            .await;
    }
    for n in 0..3 {
        fx.message(&hidden, &format!("report hidden {n}"), 10 + n)
            .await;
    }
    // In the index but gone from the store.
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

    // A query the structured parser rejects goes to the index's free-text
    // parser, which refuses it too: both surfaces fail the same way, and a
    // selection never falls back to something looser.
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

    // And the cases above are not all empty: the filters did something.
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
async fn the_resolver_lists_newest_first() {
    let fx = Fixture::new().await;
    let oldest = fx.message(&fx.account, "report old", 300).await;
    let newest = fx.message(&fx.account, "report new", 1).await;
    let middle = fx.message(&fx.account, "report mid", 100).await;
    assert_eq!(
        fx.resolved("report", None).await,
        ids(&[&newest, &middle, &oldest])
    );
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
    // One answered thread, which must stay out.
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

/// The paged search caps its owed-thread lookup at 5,000 per account; a
/// selection must not, or it would drop owed threads without a word.
#[tokio::test]
async fn a_selection_by_owed_reply_is_not_cut_at_five_thousand_threads() {
    let fx = Fixture::new().await;
    let total = 5_050usize;
    let mut stored = Vec::with_capacity(total);
    for n in 0..total {
        stored.push(
            fx.store_message(
                &fx.account,
                &format!("owed {n}"),
                60 * 24 * 10 + n as i64,
                MessageFlags::empty(),
                &[],
            )
            .await,
        );
    }
    let refs: Vec<&Envelope> = stored.iter().collect();
    fx.index(&refs).await;
    fx.state.store.refresh_contacts().await.unwrap();
    let resolved = fx.resolved("is:owed-reply", None).await;
    assert_eq!(resolved.len(), total);
}

// ---------------------------------------------------------------------------
// Preview
// ---------------------------------------------------------------------------

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

    // Across both accounts when none is named.
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

    // The reply is small whatever the selection: it holds no id list.
    let wire = serde_json::to_vec(&ResponseData::SelectionPreview { preview }).unwrap();
    assert!(wire.len() < 20_000, "{} bytes", wire.len());
}

#[tokio::test]
async fn a_preview_with_no_matches_holds_no_token() {
    let fx = Fixture::new().await;
    fx.many(2, "invoice").await;
    let preview = fx.preview("nomatchatall", &archive()).await;
    assert_eq!(preview.count, 0);
    assert!(preview.sample.is_empty());
    assert!(preview.preview_token.is_empty());
}

#[tokio::test]
async fn a_preview_refuses_what_a_selection_job_cannot_do() {
    let fx = Fixture::new().await;
    fx.many(2, "invoice").await;
    let preview = |mutation: MutationCommand, query: &str| Request::PreviewSelection {
        selection: Fixture::selection(query, None),
        mutation,
        sample_size: None,
    };
    let named = MutationCommand::Archive {
        message_ids: vec![MessageId::new()],
    };
    assert!(fx
        .refused(preview(named, "invoice"))
        .await
        .contains("by selection, not by id"));
    let trash = MutationCommand::Trash {
        message_ids: Vec::new(),
    };
    assert!(fx
        .refused(preview(trash, "invoice"))
        .await
        .contains("archive, read-archive, label and unlabel only"));
    assert!(fx
        .refused(preview(archive(), "   "))
        .await
        .contains("non-empty search query"));
    assert!(fx
        .refused(preview(archive(), "invoice AND (broken"))
        .await
        .contains("Invalid search query"));
}

// ---------------------------------------------------------------------------
// Preview and run agree
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_job_acts_on_what_the_preview_counted_and_nothing_else() {
    let fx = Fixture::new().await;
    let msgs = fx.many(5, "invoice").await;
    let preview = fx.preview("invoice", &archive()).await;
    assert_eq!(preview.count, 5);

    // Mail arrives after the preview, and one message is deleted.
    let late = fx.message(&fx.account, "invoice late", 0).await;
    fx.delete_from_store(&msgs[2]).await;

    let started = fx
        .start("invoice", &preview.preview_token, &archive())
        .await;
    // The first poll already says which accounts the job touches.
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

#[tokio::test]
async fn a_label_job_applies_and_removes_the_named_labels() {
    let fx = Fixture::new().await;
    fx.label(&fx.account, "Label_work", "Work").await;
    fx.many(3, "invoice").await;
    let add = MutationCommand::ModifyLabels {
        message_ids: Vec::new(),
        add: vec!["Work".into()],
        remove: Vec::new(),
    };
    let preview = fx.preview("invoice", &add).await;
    let job = fx.start("invoice", &preview.preview_token, &add).await;
    let job = fx.wait_for_job(&job.job_id).await;
    assert_eq!(job.status, JobStatusData::Succeeded, "{job:?}");
    assert_eq!(job.progress.succeeded, 3);
    assert_eq!(job.kind, "mutation.modify_labels");
    let added = fx
        .fake
        .mutations()
        .into_iter()
        .filter(|mutation| {
            matches!(mutation, RecordedMutation::LabelsModified { added, .. } if added == &vec!["Label_work".to_string()])
        })
        .count();
    assert_eq!(added, 3);
}

// ---------------------------------------------------------------------------
// Trash and spam stay out of reach
// ---------------------------------------------------------------------------

fn add_labels(names: &[&str]) -> MutationCommand {
    MutationCommand::ModifyLabels {
        message_ids: Vec::new(),
        add: names.iter().map(ToString::to_string).collect(),
        remove: Vec::new(),
    }
}

/// Adding `TRASH` or `SPAM` as a label trashes or spams the message at the
/// provider, so a raw request cannot reach them through a label edit: not by
/// the id in any case, and not by a display name that resolves to one.
#[tokio::test]
async fn a_label_edit_cannot_trash_or_spam_a_selection() {
    let fx = Fixture::new().await;
    fx.label(&fx.account, "TRASH", "Bin").await;
    fx.label(&fx.account, "Label_work", "Work").await;
    let msgs = fx.many(3, "invoice").await;
    let refusal = "trash and spam are not available for --all yet";

    for names in [
        &["TRASH"][..],
        &["trash"],
        &["Spam"],
        &[" SPAM "],
        &["Work", "TRASH"],
        // A display name the runner resolves to the provider id TRASH.
        &["Bin"],
        &["bin"],
    ] {
        let mutation = add_labels(names);
        let preview = fx
            .refused(Request::PreviewSelection {
                selection: Fixture::selection("invoice", Some(&fx.account)),
                mutation: mutation.clone(),
                sample_size: None,
            })
            .await;
        assert_eq!(preview, refusal, "preview of {names:?}");
        // A token cannot be had for it, but a raw start is refused on its own.
        let start = fx
            .refused(fx.start_request(
                &uuid::Uuid::now_v7().to_string(),
                "invoice",
                "any-token",
                &mutation,
            ))
            .await;
        assert_eq!(start, refusal, "start of {names:?}");
    }
    // Searching all accounts resolves display names in every account.
    let all_accounts = fx
        .refused(Request::PreviewSelection {
            selection: Fixture::selection("invoice", None),
            mutation: add_labels(&["Bin"]),
            sample_size: None,
        })
        .await;
    assert_eq!(all_accounts, refusal);

    assert!(fx.fake.mutations().is_empty());
    for message in &msgs {
        assert!(fx.in_inbox(&message.id).await);
    }

    // An ordinary label still works, and removing TRASH or SPAM is allowed:
    // it puts mail back.
    assert_eq!(fx.preview("invoice", &add_labels(&["Work"])).await.count, 3);
    let unlabel = MutationCommand::ModifyLabels {
        message_ids: Vec::new(),
        add: Vec::new(),
        remove: vec!["TRASH".into(), "SPAM".into()],
    };
    assert_eq!(fx.preview("invoice", &unlabel).await.count, 3);
}

// ---------------------------------------------------------------------------
// The token
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_token_is_single_use_and_for_one_selection_and_action() {
    let fx = Fixture::new().await;
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

    // The refusals did not use it up.
    let job = fx.start("note", &token, &archive()).await;
    fx.wait_for_job(&job.job_id).await;
    let again = fx
        .refused(fx.start_request(&new_id(), "note", &token, &archive()))
        .await;
    assert!(again.contains("expired or was already used"), "{again}");
    assert_eq!(fx.archived_provider_ids().len(), 3);
}

// ---------------------------------------------------------------------------
// Idempotent start
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_repeated_start_returns_the_same_job_and_a_lost_reply_is_recoverable() {
    let fx = Fixture::new().await;
    fx.many(3, "note").await;
    let preview = fx.preview("note", &archive()).await;
    let job_id = uuid::Uuid::now_v7().to_string();
    let request = fx.start_request(&job_id, "note", &preview.preview_token, &archive());

    let ResponseData::JobStarted { job: first } = fx.send(request.clone()).await else {
        panic!("expected a started job");
    };
    assert_eq!(first.job_id, job_id);

    // The reply was lost: the client asks for the job it named.
    assert_eq!(fx.job(&job_id).await.job_id, job_id);
    // And may send the start again, which returns the job, not a second one.
    let ResponseData::JobStarted { job: second } = fx.send(request).await else {
        panic!("expected the same job back");
    };
    assert_eq!(second.job_id, job_id);
    fx.wait_for_job(&job_id).await;
    let ResponseData::JobStarted { job: third } = fx
        .send(fx.start_request(&job_id, "note", &preview.preview_token, &archive()))
        .await
    else {
        panic!("a finished job is returned too");
    };
    assert_eq!(third.job_id, job_id);

    let ResponseData::Jobs { jobs } = fx.send(Request::ListJobs).await else {
        panic!("expected jobs");
    };
    assert_eq!(jobs.iter().filter(|job| job.job_id == job_id).count(), 1);
    let mut archived = fx.archived_provider_ids();
    archived.sort();
    archived.dedup();
    assert_eq!(archived.len(), 3);
    assert_eq!(fx.archived_provider_ids().len(), 3, "each message once");

    // The id belongs to that selection.
    let other = fx
        .refused(fx.start_request(&job_id, "different", "whatever", &archive()))
        .await;
    assert!(other.contains("already used by a different job"), "{other}");
    let bad = fx
        .refused(fx.start_request("not-a-uuid", "note", "whatever", &archive()))
        .await;
    assert!(bad.contains("UUID"), "{bad}");
}

// ---------------------------------------------------------------------------
// Partial failure
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_failing_message_ends_the_job_with_exact_counts_and_its_details() {
    let fx = Fixture::new().await;
    let msgs = fx.many(250, "invoice").await;
    let preview = fx.preview("invoice", &archive()).await;
    // Message 150 is in the second chunk of 100.
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
    assert_eq!(job.undo_ids.len(), 2, "both chunks that changed anything");
    assert_eq!(job.result.as_ref().unwrap().accounts[0].failed, 1);

    // No later chunk ran.
    assert_eq!(fx.archived_provider_ids().len(), 150);
    for later in &msgs[200..] {
        assert!(fx.in_inbox(&later.id).await);
    }
    for skipped in &msgs[151..200] {
        assert!(fx.in_inbox(&skipped.id).await);
    }
}

// ---------------------------------------------------------------------------
// Rate limits
// ---------------------------------------------------------------------------

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
async fn a_provider_that_never_stops_rate_limiting_ends_the_job_with_exact_counts() {
    let fx = Fixture::new().await;
    fx.many(5, "invoice").await;
    let preview = fx.preview("invoice", &archive()).await;
    fx.fake.rate_limit_next_mutations(10_000, 1);
    let started = fx
        .start("invoice", &preview.preview_token, &archive())
        .await;
    let job = fx.wait_for_job(&started.job_id).await;
    assert_eq!(job.status, JobStatusData::Failed);
    assert!(job.error.unwrap().contains("kept rate limiting"));
    assert_eq!((job.progress.succeeded, job.progress.failed), (0, 0));
    assert!(fx.fake.mutations().is_empty());
}

// ---------------------------------------------------------------------------
// Restart
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_restart_marks_a_running_job_interrupted_with_the_counts_it_had() {
    let fx = Fixture::new().await;
    let reached = fx.many(100, "alpha").await;
    let never_reached = fx.many(105, "beta").await;

    // A real job over the first 100 leaves a real undo entry and real counts.
    let preview = fx.preview("alpha", &archive()).await;
    let started = fx.start("alpha", &preview.preview_token, &archive()).await;
    let done = fx.wait_for_job(&started.job_id).await;
    assert_eq!(done.status, JobStatusData::Succeeded);
    assert_eq!(done.undo_ids.len(), 1);

    // The row is then written as the previous daemon would have left it
    // after its first chunk of a 205-message job: still running, and the
    // chunk's undo entry open-ended.
    let mut running = done.clone();
    running.status = JobStatusData::Running;
    running.finished_at = None;
    running.error = None;
    running.undo_expires_at = None;
    running.progress.total = 205;
    super::super::mutations::persist_job(&fx.state, &running).await;
    fx.state
        .store
        .set_undo_entries_expiry(&running.undo_ids, i64::MAX)
        .await
        .unwrap();

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
    assert_eq!(job.undo_ids, done.undo_ids);
    assert!(job.finished_at.is_some());
    let expires = job.undo_expires_at.expect("its undo window starts");
    let in_an_hour = chrono::Utc::now().timestamp_millis() + 3_600_000;
    assert!((expires - in_an_hour).abs() < 5_000);
    let entry = fx
        .state
        .store
        .read_undo_entry(&job.undo_ids[0])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(entry.expires_at, expires / 1000);

    // Nothing is left running to mark again.
    assert_eq!(
        super::super::job_recovery::mark_interrupted_mutation_jobs(&fx.state).await,
        0
    );

    // A second run over the same search finishes what the first never
    // reached; what it did reach is not in the inbox any more to match.
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

// ---------------------------------------------------------------------------
// Undo
// ---------------------------------------------------------------------------

impl Fixture {
    /// Archive everything matching `query` as one finished job.
    pub(super) async fn archive_job(&self, query: &str) -> JobData {
        let preview = self.preview(query, &archive()).await;
        let started = self.start(query, &preview.preview_token, &archive()).await;
        let job = self.wait_for_job(&started.job_id).await;
        assert_eq!(job.status, JobStatusData::Succeeded, "{job:?}");
        job
    }

    pub(super) async fn undo(&self, job_id: &str) -> JobData {
        let ResponseData::JobStarted { job } = self
            .send(Request::UndoJob {
                job_id: job_id.into(),
            })
            .await
        else {
            panic!("expected an undo job");
        };
        job
    }

    pub(super) fn restores(&self) -> usize {
        self.fake
            .mutations()
            .into_iter()
            .filter(|mutation| {
                matches!(mutation, RecordedMutation::LabelsModified { added, .. } if added.iter().any(|label| label == "INBOX"))
            })
            .count()
    }
}

#[tokio::test]
async fn a_job_anchors_every_chunks_undo_to_one_hour_after_it_ends() {
    let fx = Fixture::new().await;
    fx.many(250, "invoice").await;
    let job = fx.archive_job("invoice").await;
    assert_eq!(job.undo_ids.len(), 3);
    let expires = job.undo_expires_at.expect("an undo window");
    let finished = job.finished_at.unwrap();
    assert_eq!(
        expires - finished,
        3_600_000 - (finished % 1000),
        "an hour after the end"
    );
    for undo_id in &job.undo_ids {
        let entry = fx
            .state
            .store
            .read_undo_entry(undo_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            entry.expires_at,
            expires / 1000,
            "every chunk, not 60 s each"
        );
    }
}

#[tokio::test]
async fn undoing_a_job_puts_every_message_back_as_its_own_job() {
    let fx = Fixture::new().await;
    let msgs = fx.many(250, "invoice").await;
    let job = fx.archive_job("invoice").await;
    assert_eq!(fx.archived_provider_ids().len(), 250);
    for message in &msgs {
        assert!(!fx.in_inbox(&message.id).await);
    }

    let undo = fx.undo(&job.job_id).await;
    assert_ne!(undo.job_id, job.job_id);
    assert_eq!(undo.kind, "mutation.undo");
    assert_eq!(undo.undo_of.as_deref(), Some(job.job_id.as_str()));
    assert_eq!(undo.progress.total, 250);
    // Readable by a scoped caller from the first poll.
    assert_eq!(
        undo.result.as_ref().unwrap().accounts[0].account_id,
        fx.account
    );

    let undo = fx.wait_for_job(&undo.job_id).await;
    assert_eq!(undo.status, JobStatusData::Succeeded, "{undo:?}");
    assert_eq!(undo.error, None);
    assert_eq!(
        (
            undo.progress.succeeded,
            undo.progress.skipped,
            undo.progress.failed
        ),
        (250, 0, 0)
    );
    for message in &msgs {
        assert!(fx.in_inbox(&message.id).await);
    }
    assert_eq!(fx.restores(), 250);
    for undo_id in &job.undo_ids {
        assert!(fx
            .state
            .store
            .read_undo_entry(undo_id)
            .await
            .unwrap()
            .is_none());
    }

    // Undoing again finds nothing left, and says so instead of reporting success.
    let again = fx.undo(&job.job_id).await;
    let again = fx.wait_for_job(&again.job_id).await;
    assert_eq!(again.status, JobStatusData::Failed);
    assert!(again.error.unwrap().contains("nothing was restored"));
    assert_eq!(again.progress.skipped, 250);
}

#[tokio::test]
async fn the_undo_window_closes_an_hour_after_the_job_ends() {
    let fx = Fixture::new().await;
    fx.many(3, "invoice").await;
    let job = fx.archive_job("invoice").await;
    let closes = job.undo_expires_at.unwrap();
    let start = |at: i64| {
        let state = fx.state.clone();
        let id = job.job_id.clone();
        async move {
            super::super::job_undo::start_undo_job_at(
                &state,
                ::mxr_protocol::ClientKind::Cli,
                &id,
                at,
            )
            .await
        }
    };
    let refused = start(closes).await.unwrap_err().to_string();
    assert!(refused.contains("closed"), "{refused}");
    assert!(start(closes + 60_000).await.is_err());
    let ResponseData::JobStarted { job: undo } = start(closes - 1).await.unwrap() else {
        panic!("one millisecond inside the window starts the undo");
    };
    assert_eq!(fx.wait_for_job(&undo.job_id).await.progress.succeeded, 3);
}

#[tokio::test]
async fn an_undo_counts_the_messages_whose_snapshot_expired_or_is_missing() {
    let fx = Fixture::new().await;
    let msgs = fx.many(250, "invoice").await;
    let job = fx.archive_job("invoice").await;
    // Chunk 0 (messages 0..100) expired; chunk 1 (100..200) is gone.
    fx.state
        .store
        .set_undo_entries_expiry(&job.undo_ids[..1], 1)
        .await
        .unwrap();
    fx.state
        .store
        .delete_undo_entry(&job.undo_ids[1])
        .await
        .unwrap();

    let undo = fx.undo(&job.job_id).await;
    let undo = fx.wait_for_job(&undo.job_id).await;
    assert_eq!(undo.status, JobStatusData::Succeeded, "{undo:?}");
    assert_eq!(
        (
            undo.progress.succeeded,
            undo.progress.skipped,
            undo.progress.failed
        ),
        (50, 200, 0)
    );
    assert!(
        undo.error
            .as_deref()
            .unwrap()
            .contains("200 message(s) were not restored"),
        "{undo:?}"
    );
    for restored in &msgs[200..] {
        assert!(fx.in_inbox(&restored.id).await);
    }
    for left in &msgs[..200] {
        assert!(!fx.in_inbox(&left.id).await);
    }
}

#[tokio::test]
async fn a_single_chunk_undo_still_works_and_the_job_undo_counts_it_as_done() {
    let fx = Fixture::new().await;
    let msgs = fx.many(250, "invoice").await;
    let job = fx.archive_job("invoice").await;
    assert!(matches!(
        fx.send(Request::UndoMutation {
            mutation_id: job.undo_ids[0].clone(),
        })
        .await,
        ResponseData::Ack
    ));
    for restored in &msgs[..100] {
        assert!(fx.in_inbox(&restored.id).await);
    }
    let undo = fx.undo(&job.job_id).await;
    let undo = fx.wait_for_job(&undo.job_id).await;
    assert_eq!(
        (undo.progress.succeeded, undo.progress.skipped),
        (150, 100),
        "the chunk already undone is counted, not repeated or dropped"
    );
}

#[tokio::test]
async fn undoing_a_partly_failed_job_restores_what_it_changed() {
    let fx = Fixture::new().await;
    let msgs = fx.many(250, "invoice").await;
    let preview = fx.preview("invoice", &archive()).await;
    fx.fake.fail_mutations_of(Some(&msgs[150].provider_id));
    let started = fx
        .start("invoice", &preview.preview_token, &archive())
        .await;
    let job = fx.wait_for_job(&started.job_id).await;
    assert_eq!(job.status, JobStatusData::Failed);
    fx.fake.fail_mutations_of(None);

    let undo = fx.undo(&job.job_id).await;
    let undo = fx.wait_for_job(&undo.job_id).await;
    assert_eq!(undo.status, JobStatusData::Succeeded, "{undo:?}");
    // The 150 it archived and the one whose outcome was uncertain.
    assert_eq!(undo.progress.total, 151);
    assert_eq!(undo.progress.succeeded, 151);
    for message in &msgs {
        assert!(fx.in_inbox(&message.id).await);
    }
}

#[tokio::test]
async fn an_undo_that_cannot_reach_the_provider_keeps_what_it_could_not_restore() {
    let fx = Fixture::new().await;
    let msgs = fx.many(5, "invoice").await;
    let job = fx.archive_job("invoice").await;
    fx.fake.fail_mutations_of(Some(&msgs[2].provider_id));
    let undo = fx.undo(&job.job_id).await;
    let undo = fx.wait_for_job(&undo.job_id).await;
    assert_eq!(undo.status, JobStatusData::Failed);
    assert_eq!((undo.progress.succeeded, undo.progress.failed), (4, 1));
    assert_eq!(undo.failures[0].message_id, msgs[2].id);
    assert!(undo.error.unwrap().contains("again to retry"));

    // The same job id retries just that message.
    fx.fake.fail_mutations_of(None);
    let retry = fx.undo(&job.job_id).await;
    let retry = fx.wait_for_job(&retry.job_id).await;
    assert_eq!(retry.progress.succeeded, 1);
    assert!(fx.in_inbox(&msgs[2].id).await);
}

#[tokio::test]
async fn an_undo_waits_out_a_rate_limit() {
    let fx = Fixture::new().await;
    let msgs = fx.many(120, "invoice").await;
    let job = fx.archive_job("invoice").await;
    fx.fake.rate_limit_next_mutations(2, 5);
    let undo = fx.undo(&job.job_id).await;
    let undo = fx.wait_for_job(&undo.job_id).await;
    assert_eq!(undo.status, JobStatusData::Succeeded, "{undo:?}");
    assert_eq!(undo.progress.succeeded, 120);
    for message in &msgs {
        assert!(fx.in_inbox(&message.id).await);
    }
}

#[tokio::test]
async fn jobs_that_cannot_be_undone_as_a_whole_say_why() {
    let fx = Fixture::new().await;
    let msgs = fx.many(4, "invoice").await;

    assert!(fx
        .refused(Request::UndoJob {
            job_id: "nope".into()
        })
        .await
        .contains("job not found"));

    // A job started the old way has no one-hour window.
    let ResponseData::JobStarted { job: plain } = fx
        .send(Request::start_mutation_job(MutationCommand::Archive {
            message_ids: vec![msgs[0].id.clone()],
        }))
        .await
    else {
        panic!("expected a job");
    };
    let plain = fx.wait_for_job(&plain.job_id).await;
    let refusal = fx
        .refused(Request::UndoJob {
            job_id: plain.job_id.clone(),
        })
        .await;
    assert!(refusal.contains("no whole-job undo"), "{refusal}");

    // An undo cannot be undone.
    let job = fx.archive_job("in:inbox").await;
    let undo = fx.undo(&job.job_id).await;
    let undo = fx.wait_for_job(&undo.job_id).await;
    let refusal = fx
        .refused(Request::UndoJob {
            job_id: undo.job_id,
        })
        .await;
    assert!(refusal.contains("is an undo"), "{refusal}");
}

// ---------------------------------------------------------------------------
// Label counts
// ---------------------------------------------------------------------------

/// What the sidebar shows for the inbox.
async fn inbox_count(fx: &Fixture) -> u32 {
    fx.state
        .store
        .list_labels_by_account(&fx.account)
        .await
        .unwrap()
        .into_iter()
        .find(|label| label.provider_id == "INBOX")
        .unwrap()
        .total_count
}

/// Refreshing label counts scans the whole mailbox, so a batch does it once
/// rather than per message (70,000 messages took over an hour that way). The
/// counts must still be right when the job ends, and after its undo.
#[tokio::test]
async fn a_job_leaves_label_counts_right_and_so_does_its_undo() {
    let fx = Fixture::new().await;
    fx.many(120, "invoice").await;
    fx.state
        .store
        .recalculate_label_counts(&fx.account)
        .await
        .unwrap();
    assert_eq!(inbox_count(&fx).await, 120);

    let job = fx.archive_job("invoice").await;
    assert_eq!(inbox_count(&fx).await, 0);

    let undo = fx.undo(&job.job_id).await;
    fx.wait_for_job(&undo.job_id).await;
    assert_eq!(inbox_count(&fx).await, 120);
}

#[tokio::test]
async fn an_immediate_batch_refreshes_label_counts_before_it_replies() {
    let fx = Fixture::new().await;
    let msgs = fx.many(5, "invoice").await;
    fx.state
        .store
        .recalculate_label_counts(&fx.account)
        .await
        .unwrap();
    assert_eq!(inbox_count(&fx).await, 5);

    let ResponseData::MutationResult { result } = fx
        .send(Request::mutation(MutationCommand::Archive {
            message_ids: msgs[..3].iter().map(|m| m.id.clone()).collect(),
        }))
        .await
    else {
        panic!("expected a mutation result");
    };
    assert_eq!(result.succeeded, 3);
    assert_eq!(inbox_count(&fx).await, 2);
    let mutation_id = result.mutation_id.unwrap();
    fx.send(Request::UndoMutation { mutation_id }).await;
    assert_eq!(inbox_count(&fx).await, 5);
}

// ---------------------------------------------------------------------------
// Scope
// ---------------------------------------------------------------------------

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

    // Not naming an account would reach every account's mail.
    assert!(check(preview(None)).await.is_err());
    assert!(check(start(None)).await.is_err());
    // Another account's mail.
    assert!(check(preview(Some(&other))).await.is_err());
    assert!(check(start(Some(&other))).await.is_err());
    // Its own.
    check(preview(Some(&fx.account))).await.unwrap();
    check(start(Some(&fx.account))).await.unwrap();

    // The job it started is readable at once, and so is its undo.
    let finished = fx.archive_job("invoice").await;
    check(Request::GetJob {
        job_id: finished.job_id.clone(),
    })
    .await
    .unwrap();
    check(Request::UndoJob {
        job_id: finished.job_id.clone(),
    })
    .await
    .unwrap();

    // A read-only profile may preview and may not start.
    assert!(enforce_safety_policy(SafetyPolicy::ReadOnly, &preview(Some(&fx.account))).is_ok());
    assert!(enforce_safety_policy(SafetyPolicy::ReadOnly, &start(Some(&fx.account))).is_err());
    assert!(enforce_safety_policy(
        SafetyPolicy::ReadOnly,
        &Request::UndoJob {
            job_id: finished.job_id
        }
    )
    .is_err());
}

#[tokio::test]
async fn a_job_over_an_account_the_daemon_cannot_reach_stops_and_says_which() {
    let fx = Fixture::new().await;
    let other = fx.other_account(true).await;
    // The fake provider serves only the first account.
    for n in 0..3 {
        fx.message(&other, &format!("invoice {n}"), n).await;
    }
    let preview = fx
        .send(Request::PreviewSelection {
            selection: Fixture::selection("invoice", Some(&other)),
            mutation: archive(),
            sample_size: None,
        })
        .await;
    let ResponseData::SelectionPreview { preview } = preview else {
        panic!("expected a preview");
    };
    let job_id = uuid::Uuid::now_v7().to_string();
    let ResponseData::JobStarted { job } = fx
        .send(Request::StartSelectionJob {
            job_id,
            preview_token: preview.preview_token,
            selection: Fixture::selection("invoice", Some(&other)),
            mutation: archive(),
            client_correlation_id: None,
        })
        .await
    else {
        panic!("expected a job");
    };
    let job = fx.wait_for_job(&job.job_id).await;
    assert_eq!(job.status, JobStatusData::Failed);
    let account = &job.result.unwrap().accounts[0];
    assert!(account
        .error
        .as_deref()
        .unwrap()
        .contains("account unavailable"));
    assert_eq!(job.progress.skipped, 3);
}

// ---------------------------------------------------------------------------
// Activity
// ---------------------------------------------------------------------------

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

    // An undo records its own pair, marked as one.
    let undo = fx.undo(&job.job_id).await;
    fx.wait_for_job(&undo.job_id).await;
    let rows = job_activity(&fx).await;
    assert_eq!(rows.len(), 4);
    assert_eq!(
        rows.iter()
            .filter(|row| row.context_json.as_deref().unwrap().contains("\"undo\""))
            .count(),
        2
    );
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
async fn a_job_records_nothing_when_activity_is_off() {
    let mut config = mxr_config::MxrConfig::default();
    config.activity.enabled = false;
    let fx = Fixture::with_config(config).await;
    fx.many(2, "invoice").await;
    fx.archive_job("invoice").await;
    assert!(job_activity(&fx).await.is_empty());
}

/// `MXR_ACTIVITY=off` is read when the recorder starts, so the check runs in
/// a child process that has it set.
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

// ---------------------------------------------------------------------------
// Scale
// ---------------------------------------------------------------------------

/// Set `MXR_BULK_SCALE` to a message count (for example 70000) and run with
/// `--ignored`. Builds a demo mailbox of that size through the real sync path,
/// previews and archives all of it as one job, and checks that no frame comes
/// near the limit and the fake provider saw each message once.
#[tokio::test]
#[ignore = "scale test: set MXR_BULK_SCALE=70000 and run with --ignored"]
async fn a_selection_job_over_a_large_mailbox_stays_small_on_the_wire_and_complete() {
    let Some(count) = std::env::var("MXR_BULK_SCALE")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
    else {
        eprintln!("MXR_BULK_SCALE not set; nothing to do");
        return;
    };
    let started_at = std::time::Instant::now();
    let index_dir = tempfile::tempdir().unwrap();
    let index = mxr_search::SearchIndex::open(index_dir.path()).unwrap();
    let (state, fake) = AppState::in_memory_with_fake_provider(
        mxr_config::MxrConfig::default(),
        index,
        |account| mxr_provider_fake::FakeProvider::with_demo_dataset(account, count),
    )
    .await
    .unwrap();
    let state = Arc::new(state);
    // One pass pulls a bounded batch, so sync until the whole dataset is in.
    let mut stored = 0usize;
    for _ in 0..100 {
        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();
        let now_stored = state.store.count_all_messages().await.unwrap() as usize;
        if now_stored == stored {
            break;
        }
        stored = now_stored;
    }
    eprintln!(
        "scale: {stored} messages synced in {:?}",
        started_at.elapsed()
    );
    assert_eq!(stored, count);

    let largest_frame = std::sync::atomic::AtomicUsize::new(0);
    let send = |request: Request| {
        let state = state.clone();
        let largest_frame = &largest_frame;
        async move {
            let msg = IpcMessage {
                expected_workspace: None,
                id: 1,
                source: ::mxr_protocol::ClientKind::default(),
                payload: IpcPayload::Request(request),
            };
            let reply = handle_request(&state, &msg).await;
            let frame = serde_json::to_vec(&reply).unwrap().len();
            largest_frame.fetch_max(frame, std::sync::atomic::Ordering::SeqCst);
            match reply.payload {
                IpcPayload::Response(Response::Ok { data }) => data,
                other => panic!("expected Ok, got {other:?}"),
            }
        }
    };

    let query = "in:anywhere";
    let preview_started = std::time::Instant::now();
    let ResponseData::SelectionPreview { preview } = send(Request::PreviewSelection {
        selection: SelectionData::Search {
            query: query.into(),
            account_id: None,
        },
        mutation: archive(),
        sample_size: None,
    })
    .await
    else {
        panic!("expected a preview");
    };
    eprintln!(
        "scale: preview of {} took {:?}",
        preview.count,
        preview_started.elapsed()
    );
    assert_eq!(preview.count as usize, count);

    let job_id = uuid::Uuid::now_v7().to_string();
    let run_started = std::time::Instant::now();
    let ResponseData::JobStarted { job } = send(Request::StartSelectionJob {
        job_id,
        preview_token: preview.preview_token,
        selection: SelectionData::Search {
            query: query.into(),
            account_id: None,
        },
        mutation: archive(),
        client_correlation_id: None,
    })
    .await
    else {
        panic!("expected a job");
    };
    let mut last_report = std::time::Instant::now();
    let job = loop {
        let ResponseData::Job { job } = send(Request::GetJob {
            job_id: job.job_id.clone(),
        })
        .await
        else {
            panic!("expected a job");
        };
        if !matches!(job.status, JobStatusData::Queued | JobStatusData::Running) {
            break job;
        }
        if last_report.elapsed() >= std::time::Duration::from_secs(30) {
            last_report = std::time::Instant::now();
            eprintln!(
                "scale: {} of {} done after {:?}",
                job.progress.completed,
                job.progress.total,
                run_started.elapsed()
            );
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    };
    eprintln!("scale: job took {:?}", run_started.elapsed());
    assert_eq!(job.status, JobStatusData::Succeeded, "{:?}", job.error);
    assert_eq!(job.progress.succeeded as usize, count);
    assert_eq!(fake.mutations().len(), count, "each message once");
    let largest = largest_frame.load(std::sync::atomic::Ordering::SeqCst);
    eprintln!("scale: largest frame {largest} bytes");
    assert!(largest < 1_000_000, "a frame was {largest} bytes");
}
