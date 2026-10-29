//! Bulk actions over a daemon-resolved selection (`--all`).
//!
//! The client never holds the message ids. `PreviewSelection` resolves the
//! selection and keeps the ids here under a single-use token, answering with
//! counts and a sample. `StartSelectionJob` takes the token and runs the
//! action over exactly those ids as one background job, so what the user
//! confirmed is what runs, and mail that arrives later is left alone.

use super::diagnostics_impl::search_resolve::{resolve_search_selection, ResolvedSelection};
use super::{mutations, HandlerError, HandlerResult};
use crate::state::AppState;
use mxr_core::id::{AccountId, MessageId};
use mxr_core::types::LabelKind;
use mxr_protocol::{
    AccountMutationResultData, ClientKind, JobData, MutationCommand, ResponseData,
    SelectionAccountData, SelectionData, SelectionPreviewData, SelectionSampleData,
};
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How long a preview can be committed.
const PREVIEW_TTL: Duration = Duration::from_secs(10 * 60);
/// Previews kept at once; the oldest goes first.
const MAX_PREVIEWS: usize = 16;
/// Message ids kept across all previews; the oldest previews go first. At 16
/// bytes an id this is about 32 MB.
const MAX_PREVIEW_IDS: usize = 2_000_000;
const DEFAULT_SAMPLE: u32 = 20;
const MAX_SAMPLE: u32 = 100;

/// What a selection job needs of its mutation: archive, read-archive and label
/// edits. Everything else (trash, spam, flags, move) is not offered for a whole
/// selection yet, so a client cannot reach it by skipping the CLI; a label edit
/// outside the user's own labels is refused by [`require_user_labels`].
fn require_supported(mutation: &MutationCommand) -> Result<(), HandlerError> {
    let ids = match mutation {
        MutationCommand::Archive { message_ids }
        | MutationCommand::ReadAndArchive { message_ids }
        | MutationCommand::ModifyLabels { message_ids, .. } => message_ids,
        _ => {
            return Err(
                "selection jobs support archive, read-archive, label and unlabel only".into(),
            )
        }
    };
    if !ids.is_empty() {
        return Err("a selection job names its messages by selection, not by id".into());
    }
    Ok(())
}

/// The action as a string to compare: a token is for one action only.
fn action_key(mutation: &MutationCommand) -> String {
    serde_json::to_string(mutation).unwrap_or_default()
}

#[derive(Debug)]
struct PreviewEntry {
    selection: SelectionData,
    action: String,
    ids: Vec<MessageId>,
    accounts: Vec<AccountMutationResultData>,
    created: Instant,
}

/// Previews waiting to be committed, by token. Single use. A daemon restart
/// loses them, which is safe: nothing had happened yet.
pub(crate) struct SelectionPreviews {
    entries: Mutex<Vec<(String, PreviewEntry)>>,
    max_previews: usize,
    max_ids: usize,
}

impl Default for SelectionPreviews {
    fn default() -> Self {
        Self::with_limits(MAX_PREVIEWS, MAX_PREVIEW_IDS)
    }
}

impl SelectionPreviews {
    pub(crate) fn with_limits(max_previews: usize, max_ids: usize) -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
            max_previews,
            max_ids,
        }
    }

    fn insert(&self, entry: PreviewEntry) -> String {
        let token = uuid::Uuid::now_v7().to_string();
        let mut entries = self.entries.lock();
        entries.retain(|(_, kept)| kept.created.elapsed() < PREVIEW_TTL);
        // Oldest first, so eviction takes from the front. One preview larger
        // than the whole budget is kept alone rather than refused.
        let mut held: usize = entries.iter().map(|(_, kept)| kept.ids.len()).sum();
        while !entries.is_empty()
            && (entries.len() >= self.max_previews || held + entry.ids.len() > self.max_ids)
        {
            let (_, evicted) = entries.remove(0);
            held -= evicted.ids.len();
        }
        entries.push((token.clone(), entry));
        token
    }

    /// The preview behind `token` for this selection and action, once. A
    /// request for another selection or action leaves the token usable.
    /// What the preview behind `token` covers, without using it up: the same
    /// checks as [`Self::take`].
    fn peek(
        &self,
        token: &str,
        selection: &SelectionData,
        action: &str,
    ) -> Result<(u32, Vec<AccountMutationResultData>), HandlerError> {
        let entries = self.entries.lock();
        match entries.iter().find(|(held, _)| held == token) {
            Some((_, entry)) if entry.created.elapsed() >= PREVIEW_TTL => {}
            Some((_, entry)) if &entry.selection != selection || entry.action != action => {
                return Err("the preview_token belongs to a different selection or action".into());
            }
            Some((_, entry)) => return Ok((entry.ids.len() as u32, entry.accounts.clone())),
            None => {}
        }
        Err("that preview expired or was already used; preview again".into())
    }

    fn take(
        &self,
        token: &str,
        selection: &SelectionData,
        action: &str,
    ) -> Result<PreviewEntry, HandlerError> {
        self.take_at(token, selection, action, Instant::now())
    }

    fn take_at(
        &self,
        token: &str,
        selection: &SelectionData,
        action: &str,
        now: Instant,
    ) -> Result<PreviewEntry, HandlerError> {
        let mut entries = self.entries.lock();
        let position = entries.iter().position(|(held, _)| held == token);
        if let Some(position) = position {
            let (_, entry) = &entries[position];
            if now.saturating_duration_since(entry.created) >= PREVIEW_TTL {
                entries.remove(position);
            } else if &entry.selection != selection || entry.action != action {
                return Err("the preview_token belongs to a different selection or action".into());
            } else {
                return Ok(entries.remove(position).1);
            }
        }
        Err("that preview expired or was already used; preview again".into())
    }
}

/// How a job is described in the activity log.
#[derive(Debug, Clone)]
pub(super) struct JobActivity {
    pub source: ClientKind,
    pub action: &'static str,
    pub account_id: Option<String>,
    pub query: Option<String>,
    /// Set on an undo job: the job it reverses.
    pub undo_of: Option<String>,
}

fn action_token(mutation: &MutationCommand) -> &'static str {
    match mutation {
        MutationCommand::ModifyLabels { add, remove, .. }
            if add.is_empty() && !remove.is_empty() =>
        {
            "mail.unlabel"
        }
        MutationCommand::ModifyLabels { .. } => "mail.label",
        _ => "mail.archive",
    }
}

/// One entry when the daemon accepts the job. Counts only: no message ids,
/// subjects or bodies (a job over 500k messages would put megabytes of ids in
/// the log). `query` is dropped by the recorder unless search queries are
/// tracked.
pub(super) fn record_job_accepted(state: &AppState, activity: &JobActivity, job: &JobData) {
    let mut context = serde_json::json!({
        "count": job.progress.total,
        "job_id": job.job_id,
        "phase": "accepted",
    });
    add_description(&mut context, activity);
    record(state, activity, job, context);
}

/// One entry when the job ends, whatever the outcome.
pub(super) fn record_job_finished(state: &AppState, activity: &JobActivity, job: &JobData) {
    let mut context = serde_json::json!({
        "job_id": job.job_id,
        "phase": "finished",
        "status": job.status,
        "succeeded": job.progress.succeeded,
        "skipped": job.progress.skipped,
        "failed": job.progress.failed,
    });
    add_description(&mut context, activity);
    record(state, activity, job, context);
}

fn add_description(context: &mut serde_json::Value, activity: &JobActivity) {
    let Some(fields) = context.as_object_mut() else {
        return;
    };
    if let Some(query) = &activity.query {
        fields.insert("query".into(), query.clone().into());
    }
    if let Some(undo_of) = &activity.undo_of {
        fields.insert("kind".into(), "undo".into());
        fields.insert("undo_of".into(), undo_of.clone().into());
    }
}

fn record(state: &AppState, activity: &JobActivity, job: &JobData, context: serde_json::Value) {
    state.activity.record(crate::activity::OwnedEntry {
        ts: crate::activity::current_unix_ms(),
        account_id: activity.account_id.clone(),
        source: activity.source,
        action: activity.action.into(),
        target_kind: Some("job".into()),
        target_id: Some(job.job_id.clone()),
        tier: crate::activity::tier::tier_for(activity.action),
        context: Some(context),
    });
}

/// A label edit over a whole selection may only name labels the user created.
///
/// A provider acts on some labels rather than just tagging: adding `TRASH` or
/// `SPAM` trashes or spams, IMAP maps a trash folder to an id of its own, and a
/// name like `DELETED` becomes the IMAP `\Deleted` flag. Those mailbox
/// actions are not offered for a selection yet, and a block-list would miss
/// aliases, so this is an allow-list: each label named, added or removed, must
/// be one the store holds for the account with kind `User`. System labels
/// (INBOX, TRASH, SPAM, IMPORTANT, STARRED, UNREAD, DRAFT, SENT, anything with
/// a role), provider folders and names that match no label are refused. The
/// name is resolved the way the runner resolves it, and must resolve to a user
/// label in every account the selection can reach.
async fn require_user_labels(
    state: &AppState,
    selection: &SelectionData,
    mutation: &MutationCommand,
) -> Result<(), HandlerError> {
    let MutationCommand::ModifyLabels { add, remove, .. } = mutation else {
        return Ok(());
    };
    if add.is_empty() && remove.is_empty() {
        return Err("name the label to add or remove".into());
    }
    let SelectionData::Search { account_id, .. } = selection;
    let accounts = match account_id {
        Some(account_id) => vec![account_id.clone()],
        None => state
            .store
            .list_cached_mail_accounts()
            .await?
            .into_iter()
            .map(|account| account.id)
            .collect(),
    };
    for account in accounts {
        let labels = state.store.list_labels_by_account(&account).await?;
        for reference in add.iter().chain(remove) {
            match mutations::find_label(&labels, reference) {
                Some(label) if label.kind == LabelKind::User => {}
                Some(_) => {
                    return Err(format!(
                        "`{reference}` is a system label or folder; --all label edits only work on labels you created"
                    )
                    .into())
                }
                None => {
                    let name = state
                        .store
                        .get_account(&account)
                        .await?
                        .map_or_else(|| account.to_string(), |found| found.name);
                    return Err(format!(
                        "`{reference}` is not a label you created in account {name}; --all label edits only work on labels you created"
                    )
                    .into());
                }
            }
        }
    }
    Ok(())
}

pub(super) async fn preview_selection(
    state: &Arc<AppState>,
    selection: &SelectionData,
    mutation: &MutationCommand,
    sample_size: Option<u32>,
) -> HandlerResult {
    require_supported(mutation)?;
    require_user_labels(state, selection, mutation).await?;
    let SelectionData::Search { query, account_id } = selection;
    if query.trim().is_empty() {
        return Err("a selection needs a non-empty search query".into());
    }
    let resolved = resolve_search_selection(state, query, account_id.as_ref()).await?;
    let sample_size = sample_size.unwrap_or(DEFAULT_SAMPLE).min(MAX_SAMPLE);
    let (accounts, sample) = describe(state, &resolved, sample_size as usize).await?;
    let count = resolved.ids.len() as u32;

    let (preview_token, expires_at) = if resolved.ids.is_empty() {
        // Nothing to commit, so no token to hold.
        (String::new(), 0)
    } else {
        let results = accounts
            .iter()
            .map(|account| AccountMutationResultData {
                account_id: account.account_id.clone(),
                account_name: account.account_name.clone(),
                succeeded: 0,
                skipped: 0,
                failed: 0,
                error: None,
            })
            .collect();
        let token = state.selection_previews.insert(PreviewEntry {
            selection: selection.clone(),
            action: action_key(mutation),
            ids: resolved.ids,
            accounts: results,
            created: Instant::now(),
        });
        let expires_at = chrono::Utc::now().timestamp_millis()
            + i64::try_from(PREVIEW_TTL.as_millis()).unwrap_or(i64::MAX);
        (token, expires_at)
    };

    Ok(ResponseData::SelectionPreview {
        preview: SelectionPreviewData {
            count,
            threads: resolved.threads,
            accounts,
            sample,
            sample_size,
            capped: false,
            preview_token,
            expires_at,
        },
    })
}

/// Account names and the newest messages, for the confirmation table.
async fn describe(
    state: &AppState,
    resolved: &ResolvedSelection,
    sample_size: usize,
) -> Result<(Vec<SelectionAccountData>, Vec<SelectionSampleData>), HandlerError> {
    let mut accounts = Vec::with_capacity(resolved.accounts.len());
    for (account_id, count) in &resolved.accounts {
        let account_name = state
            .store
            .get_account(account_id)
            .await?
            .map_or_else(|| account_id.to_string(), |account| account.name);
        accounts.push(SelectionAccountData {
            account_id: account_id.clone(),
            account_name,
            count: *count,
        });
    }
    accounts.sort_by(|left, right| {
        left.account_name
            .to_lowercase()
            .cmp(&right.account_name.to_lowercase())
            .then_with(|| left.account_id.as_str().cmp(&right.account_id.as_str()))
    });

    let newest = &resolved.ids[..sample_size.min(resolved.ids.len())];
    let sample = state
        .store
        .list_envelopes_by_ids(newest)
        .await?
        .into_iter()
        .map(|envelope| SelectionSampleData {
            message_id: envelope.id,
            account_id: envelope.account_id,
            thread_id: envelope.thread_id,
            from: envelope
                .from
                .name
                .filter(|name| !name.is_empty())
                .unwrap_or(envelope.from.email),
            subject: envelope.subject,
            date: envelope.date,
        })
        .collect();
    Ok((accounts, sample))
}

#[allow(clippy::too_many_arguments, reason = "mirrors the request's fields")]
pub(super) async fn start_selection_job(
    state: &Arc<AppState>,
    source: ClientKind,
    job_id: &str,
    preview_token: &str,
    selection: &SelectionData,
    mutation: &MutationCommand,
    client_correlation_id: Option<&str>,
) -> HandlerResult {
    require_supported(mutation)?;
    require_user_labels(state, selection, mutation).await?;
    uuid::Uuid::parse_str(job_id).map_err(|_| {
        HandlerError::from("job_id must be a UUID chosen by the client".to_string())
    })?;

    // A job with this id already exists: the first request got through and
    // its reply was lost. Hand the job back instead of starting another.
    if let Some(job) = existing_job(state, job_id, selection, mutation).await? {
        return Ok(ResponseData::JobStarted { job });
    }

    // Claim the id before spending the token or starting anything. The claim
    // is an insert that only one of any number of concurrent requests with
    // this id wins; the others get the winner's job (or an error if it is for
    // something else). The token is only looked at here, so a loser leaves it
    // for whoever it was meant for.
    let (total, accounts) =
        state
            .selection_previews
            .peek(preview_token, selection, &action_key(mutation))?;
    let job = mutations::queued_job(
        job_id.to_string(),
        mutation,
        total,
        Some(selection),
        &accounts,
    );
    if !mutations::claim_job(state, &job).await? {
        return match existing_job(state, job_id, selection, mutation).await? {
            Some(job) => Ok(ResponseData::JobStarted { job }),
            None => Err(format!("job_id {job_id} could not be claimed; try again").into()),
        };
    }

    // Now the token: two requests with different ids can hold the same one,
    // and only one of them gets to use it.
    let previewed =
        match state
            .selection_previews
            .take(preview_token, selection, &action_key(mutation))
        {
            Ok(previewed) => previewed,
            Err(error) => {
                mutations::release_job_claim(state, job_id).await;
                return Err(error);
            }
        };

    let SelectionData::Search { query, account_id } = selection;
    let cmd = with_ids(mutation, previewed.ids);
    let mut spec = mutations::JobSpec::new(
        cmd,
        client_correlation_id.map(str::to_string),
        mutations::ChunkGuard::None,
    );
    spec.job_id = Some(job_id.to_string());
    spec.selection = Some(selection.clone());
    spec.accounts = previewed.accounts;
    spec.activity = Some(JobActivity {
        source,
        action: action_token(mutation),
        account_id: account_id.as_ref().map(AccountId::as_str),
        query: Some(query.clone()),
        undo_of: None,
    });
    Ok(mutations::run_claimed_job(state.clone(), job, spec))
}

/// The job already started under `job_id`, if any. It must be the same
/// selection and action; a reused id for something else is an error.
async fn existing_job(
    state: &AppState,
    job_id: &str,
    selection: &SelectionData,
    mutation: &MutationCommand,
) -> Result<Option<JobData>, HandlerError> {
    let Some(json) = state.store.get_mutation_job(job_id).await? else {
        return Ok(None);
    };
    let job: JobData =
        serde_json::from_str(&json).map_err(|e| HandlerError::from(e.to_string()))?;
    // The whole action, not just its kind: `label Work` and `unlabel Work`
    // under one id are different requests.
    if job.selection.as_ref() == Some(selection) && job.action.as_ref() == Some(mutation) {
        return Ok(Some(mutations::bound_for_wire(job)));
    }
    Err(format!("job_id {job_id} is already used by a different job").into())
}

fn with_ids(mutation: &MutationCommand, message_ids: Vec<MessageId>) -> MutationCommand {
    match mutation {
        MutationCommand::Archive { .. } => MutationCommand::Archive { message_ids },
        MutationCommand::ReadAndArchive { .. } => MutationCommand::ReadAndArchive { message_ids },
        MutationCommand::ModifyLabels { add, remove, .. } => MutationCommand::ModifyLabels {
            message_ids,
            add: add.clone(),
            remove: remove.clone(),
        },
        // `require_supported` has refused every other command.
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(ids: usize, selection: &SelectionData, action: &str) -> PreviewEntry {
        PreviewEntry {
            selection: selection.clone(),
            action: action.into(),
            ids: (0..ids).map(|_| MessageId::new()).collect(),
            accounts: Vec::new(),
            created: Instant::now(),
        }
    }

    fn search(query: &str) -> SelectionData {
        SelectionData::Search {
            query: query.into(),
            account_id: None,
        }
    }

    #[test]
    fn a_token_works_once() {
        let previews = SelectionPreviews::default();
        let selection = search("from:x");
        let token = previews.insert(entry(3, &selection, "archive"));
        assert_eq!(
            previews
                .take(&token, &selection, "archive")
                .unwrap()
                .ids
                .len(),
            3
        );
        assert!(previews
            .take(&token, &selection, "archive")
            .unwrap_err()
            .to_string()
            .contains("already used"));
    }

    #[test]
    fn a_token_for_another_selection_or_action_is_refused_and_stays_usable() {
        let previews = SelectionPreviews::default();
        let selection = search("from:x");
        let token = previews.insert(entry(2, &selection, "archive"));
        for (other, action) in [(search("from:y"), "archive"), (selection.clone(), "label")] {
            assert!(previews
                .take(&token, &other, action)
                .unwrap_err()
                .to_string()
                .contains("different"));
        }
        assert!(previews.take(&token, &selection, "archive").is_ok());
    }

    #[test]
    fn a_token_expires_after_ten_minutes() {
        let previews = SelectionPreviews::default();
        let selection = search("from:x");
        let token = previews.insert(entry(2, &selection, "archive"));
        let just_before = Instant::now() + PREVIEW_TTL - Duration::from_secs(1);
        let later = Instant::now() + PREVIEW_TTL + Duration::from_secs(1);
        // Not consumed by a look that finds it too late.
        assert!(previews
            .take_at(&token, &selection, "archive", later)
            .is_err());
        let token = previews.insert(entry(2, &selection, "archive"));
        assert!(previews
            .take_at(&token, &selection, "archive", just_before)
            .is_ok());
    }

    #[test]
    fn the_id_budget_evicts_the_oldest_preview_first() {
        let previews = SelectionPreviews::with_limits(10, 10);
        let selection = search("from:x");
        let first = previews.insert(entry(4, &selection, "archive"));
        let second = previews.insert(entry(4, &selection, "archive"));
        // 8 held; 4 more would pass 10, so the oldest goes.
        let third = previews.insert(entry(4, &selection, "archive"));
        assert!(previews.take(&first, &selection, "archive").is_err());
        assert!(previews.take(&second, &selection, "archive").is_ok());
        assert!(previews.take(&third, &selection, "archive").is_ok());
    }

    #[test]
    fn the_entry_cap_evicts_the_oldest_preview_first() {
        let previews = SelectionPreviews::with_limits(2, 1_000);
        let selection = search("from:x");
        let first = previews.insert(entry(1, &selection, "archive"));
        let second = previews.insert(entry(1, &selection, "archive"));
        let third = previews.insert(entry(1, &selection, "archive"));
        assert!(previews.take(&first, &selection, "archive").is_err());
        assert!(previews.take(&second, &selection, "archive").is_ok());
        assert!(previews.take(&third, &selection, "archive").is_ok());
    }

    #[test]
    fn one_preview_over_the_whole_budget_is_kept_alone() {
        let previews = SelectionPreviews::with_limits(10, 5);
        let selection = search("from:x");
        let small = previews.insert(entry(2, &selection, "archive"));
        let big = previews.insert(entry(9, &selection, "archive"));
        assert!(previews.take(&small, &selection, "archive").is_err());
        assert_eq!(
            previews
                .take(&big, &selection, "archive")
                .unwrap()
                .ids
                .len(),
            9
        );
    }
}
