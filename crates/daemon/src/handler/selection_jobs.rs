//! Bulk actions over a daemon-resolved selection (`--all`).
//!
//! The client never holds the message ids. `PreviewSelection` resolves the
//! selection and keeps the ids here under a single-use token, answering with
//! counts and a sample. `StartSelectionJob` takes the token and runs the
//! action over exactly those ids as one background job, so what the user
//! confirmed is what runs, and mail that arrives later is left alone.

use super::diagnostics_impl::search_resolve::{resolve_search_selection, ResolvedSelection};
use super::job_records::{bounded_text, job_detail, MAX_ACCOUNT_NAME_BYTES, MAX_JOB_TEXT_BYTES};
use super::mutations::{FrozenLabelEdits, ResolvedLabelEdit};
use super::{mutations, HandlerError, HandlerResult};
use crate::state::AppState;
use mxr_core::id::{AccountId, LabelId, MessageId};
use mxr_core::types::LabelKind;
use mxr_protocol::{
    AccountMutationResultData, ClientKind, JobData, MutationCommand, ResponseData,
    SelectionAccountData, SelectionData, SelectionPreviewData, SelectionSampleData,
};
use parking_lot::Mutex;
use std::collections::HashMap;
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
/// The longest search a selection job takes. The selection is kept with the
/// job and listed with every job, so it is bounded like the job's other text.
const MAX_SELECTION_QUERY_BYTES: usize = 2048;
/// The most labels one selection label edit names, added and removed.
const MAX_SELECTION_LABELS: usize = 8;
/// The most bytes of a sample message's sender or subject a preview carries:
/// stored mail can hold a subject of any size, and 100 of them would pass
/// the frame limit.
const MAX_SAMPLE_TEXT_BYTES: usize = 256;
/// The longest `client_correlation_id` a start takes: the daemon echoes it in
/// events about the job.
const MAX_CORRELATION_ID_BYTES: usize = 128;

/// What a selection job needs of its request: archive, read-archive and label
/// edits, a query and labels of bounded size. Everything else (trash, spam,
/// flags, move) is not offered for a whole selection yet, so a client cannot
/// reach it by skipping the CLI; a label edit outside the user's own labels is
/// refused by [`freeze_labels`].
fn require_supported(
    selection: &SelectionData,
    mutation: &MutationCommand,
) -> Result<(), HandlerError> {
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
    let SelectionData::Search { query, .. } = selection;
    if query.len() > MAX_SELECTION_QUERY_BYTES {
        return Err(format!(
            "a selection's search can be at most {MAX_SELECTION_QUERY_BYTES} bytes"
        )
        .into());
    }
    if let MutationCommand::ModifyLabels { add, remove, .. } = mutation {
        if add.is_empty() && remove.is_empty() {
            return Err("name the label to add or remove".into());
        }
        if add.len() + remove.len() > MAX_SELECTION_LABELS {
            return Err(format!(
                "a selection label edit names at most {MAX_SELECTION_LABELS} labels"
            )
            .into());
        }
        if add
            .iter()
            .chain(remove)
            .any(|label| label.len() > MAX_JOB_TEXT_BYTES)
        {
            return Err(
                format!("a label name can be at most {MAX_JOB_TEXT_BYTES} bytes here").into(),
            );
        }
    }
    Ok(())
}

/// The action as a string to compare: a token is for one action only.
fn action_key(mutation: &MutationCommand) -> String {
    serde_json::to_string(mutation).unwrap_or_default()
}

/// One label a label edit names, as the preview resolved it in one account.
#[derive(Debug, Clone)]
struct FrozenLabel {
    /// What the user typed, for messages.
    reference: String,
    label_id: LabelId,
    provider_id: String,
}

#[derive(Debug, Clone, Default)]
struct FrozenAccountLabels {
    add: Vec<FrozenLabel>,
    remove: Vec<FrozenLabel>,
}

/// A label edit's labels, by account the selection reaches.
type FrozenLabels = HashMap<AccountId, FrozenAccountLabels>;

#[derive(Debug)]
struct PreviewEntry {
    selection: SelectionData,
    action: String,
    ids: Vec<MessageId>,
    accounts: Vec<AccountMutationResultData>,
    /// Set for a label edit: the labels it names, resolved in each account
    /// the selection reaches.
    labels: Option<FrozenLabels>,
    created: Instant,
}

/// A test's hold on a start between taking its token and recording its job.
#[cfg(test)]
pub(crate) type StartPause = (Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>);

/// Previews waiting to be committed, by token. Single use. A daemon restart
/// loses them, which is safe: nothing had happened yet.
pub(crate) struct SelectionPreviews {
    entries: Mutex<Vec<(String, PreviewEntry)>>,
    max_previews: usize,
    max_ids: usize,
    /// Set by a test: the next start signals the first notify once it holds
    /// its token, then waits for the second.
    #[cfg(test)]
    pub(crate) pause_after_take: Mutex<Option<StartPause>>,
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
            #[cfg(test)]
            pause_after_take: Mutex::new(None),
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

    /// Give back a taken preview whose job did not start, under its token and
    /// with its age, so whoever it was meant for can still use it.
    fn put_back(&self, token: &str, entry: PreviewEntry) {
        self.entries.lock().push((token.to_string(), entry));
    }
}

/// How a job is described in the activity log.
#[derive(Debug, Clone)]
pub(super) struct JobActivity {
    pub source: ClientKind,
    pub action: &'static str,
    pub account_id: Option<String>,
    pub query: Option<String>,
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

/// A label edit over a whole selection may only name labels the user created,
/// and the preview pins each to the label it means in each account.
///
/// A provider acts on some labels rather than just tagging: adding `TRASH` or
/// `SPAM` trashes or spams, IMAP maps a trash folder to an id of its own, and a
/// name like `DELETED` becomes the IMAP `\Deleted` flag. Those mailbox
/// actions are not offered for a selection yet, and a block-list would miss
/// aliases, so this is an allow-list: each label named, added or removed, must
/// be one the store holds for the account with kind `User`. System labels
/// (INBOX, TRASH, SPAM, IMPORTANT, STARRED, UNREAD, DRAFT, SENT, anything with
/// a role), provider folders and names that match no label are refused. The
/// name is resolved the way the runner resolves names, in each account the
/// resolved selection reaches (`accounts`) and only those.
///
/// The result is what the job applies: the label's id in each account, so a
/// label renamed after the preview, or another created under the old name,
/// does not change which label the job edits.
async fn freeze_labels(
    state: &AppState,
    accounts: &[AccountId],
    mutation: &MutationCommand,
) -> Result<Option<FrozenLabels>, HandlerError> {
    let MutationCommand::ModifyLabels { add, remove, .. } = mutation else {
        return Ok(None);
    };
    let mut frozen = FrozenLabels::new();
    for account in accounts {
        let labels = state.store.list_labels_by_account(account).await?;
        let resolve = |references: &[String]| -> Result<Vec<FrozenLabel>, String> {
            references
                .iter()
                .map(|reference| match mutations::find_label(&labels, reference) {
                    Some(label) if is_users_own(label) => Ok(FrozenLabel {
                        reference: reference.clone(),
                        label_id: label.id.clone(),
                        provider_id: label.provider_id.clone(),
                    }),
                    Some(_) => Err(format!(
                        "`{reference}` is a system label or folder; --all label edits only work on labels you created"
                    )),
                    None => Err(reference.clone()),
                })
                .collect()
        };
        let edit = match (resolve(add), resolve(remove)) {
            (Ok(add), Ok(remove)) => FrozenAccountLabels { add, remove },
            (Err(error), _) | (_, Err(error)) => {
                if error.starts_with('`') {
                    return Err(error.into());
                }
                let name = state
                    .store
                    .get_account(account)
                    .await?
                    .map_or_else(|| account.to_string(), |found| found.name);
                let name = bounded_text(&name, MAX_ACCOUNT_NAME_BYTES);
                return Err(format!(
                    "`{error}` is not a label you created in account {name}; --all label edits only work on labels you created"
                )
                .into());
            }
        };
        frozen.insert(account.clone(), edit);
    }
    Ok(Some(frozen))
}

/// A label the user created: kind `User`, and no mailbox role (Inbox, Trash,
/// Spam, …) either, in case a provider tags a role on a label it calls a user
/// label.
fn is_users_own(label: &mxr_core::types::Label) -> bool {
    label.kind == LabelKind::User && label.role.is_none()
}

/// The provider ids a starting job applies: each label the preview froze,
/// provided it is still there, still the user's, under the same provider id.
/// `Ok(Err(..))` when one is not: the preview is stale.
async fn check_frozen_labels(
    state: &AppState,
    frozen: &FrozenLabels,
) -> Result<Result<FrozenLabelEdits, String>, HandlerError> {
    let mut edits = FrozenLabelEdits::new();
    for (account, wanted) in frozen {
        let labels = state.store.list_labels_by_account(account).await?;
        let still_there = |frozen: &FrozenLabel| {
            labels.iter().any(|label| {
                label.id == frozen.label_id
                    && label.provider_id == frozen.provider_id
                    && is_users_own(label)
            })
        };
        if let Some(gone) = wanted
            .add
            .iter()
            .chain(&wanted.remove)
            .find(|frozen| !still_there(frozen))
        {
            return Ok(Err(format!(
                "the label `{}` changed since the preview (it was deleted, or is no longer one you created); preview again",
                gone.reference
            )));
        }
        let ids = |frozen: &[FrozenLabel]| {
            frozen
                .iter()
                .map(|label| label.provider_id.clone())
                .collect()
        };
        edits.insert(
            account.clone(),
            ResolvedLabelEdit {
                add: ids(&wanted.add),
                remove: ids(&wanted.remove),
            },
        );
    }
    Ok(Ok(edits))
}

pub(super) async fn preview_selection(
    state: &Arc<AppState>,
    selection: &SelectionData,
    mutation: &MutationCommand,
    sample_size: Option<u32>,
) -> HandlerResult {
    require_supported(selection, mutation)?;
    let SelectionData::Search { query, account_id } = selection;
    if query.trim().is_empty() {
        return Err("a selection needs a non-empty search query".into());
    }
    let resolved = resolve_search_selection(state, query, account_id.as_ref()).await?;
    // The labels are checked in the accounts the selection reaches, after it
    // is resolved, and kept with the preview for the job.
    let reached: Vec<AccountId> = resolved
        .accounts
        .iter()
        .map(|(account, _)| account.clone())
        .collect();
    let labels = freeze_labels(state, &reached, mutation).await?;
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
            labels,
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
            account_name: bounded_text(&account_name, MAX_ACCOUNT_NAME_BYTES),
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
            from: bounded_text(
                &envelope
                    .from
                    .name
                    .filter(|name| !name.is_empty())
                    .unwrap_or(envelope.from.email),
                MAX_SAMPLE_TEXT_BYTES,
            ),
            subject: bounded_text(&envelope.subject, MAX_SAMPLE_TEXT_BYTES),
            date: envelope.date,
        })
        .collect();
    Ok((accounts, sample))
}

/// Start the job a preview counted.
///
/// The order is what makes a job id mean a real job: (1) the token is taken
/// from memory, so of any number of requests holding it one gets it; (2) the
/// job's full row is inserted as `queued` with an insert that fails when the
/// id exists; (3) the job is spawned. A job id is only ever returned for a
/// row that is a real queued, running or finished job. When the insert finds
/// the id taken, the token goes back and the request gets that job if it is
/// the same selection and action (a retry of an accepted start), or an error.
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
    require_supported(selection, mutation)?;
    uuid::Uuid::parse_str(job_id).map_err(|_| {
        HandlerError::from("job_id must be a UUID chosen by the client".to_string())
    })?;
    if client_correlation_id.is_some_and(|id| id.len() > MAX_CORRELATION_ID_BYTES) {
        return Err(format!(
            "client_correlation_id can be at most {MAX_CORRELATION_ID_BYTES} bytes"
        )
        .into());
    }

    // A job with this id already exists: the first request got through and
    // its reply was lost. Hand the job back instead of starting another.
    if let Some(job) = existing_job(state, job_id, selection, mutation).await? {
        return Ok(ResponseData::JobStarted { job });
    }

    let action = action_key(mutation);
    let previewed = match state
        .selection_previews
        .take(preview_token, selection, &action)
    {
        Ok(previewed) => previewed,
        // Another request took it. If that was this job's own start, its row
        // may be there by now; otherwise this request did not start a job.
        Err(error) => {
            return match existing_job(state, job_id, selection, mutation).await? {
                Some(job) => Ok(ResponseData::JobStarted { job }),
                None => Err(error),
            };
        }
    };
    #[cfg(test)]
    {
        let pause = state.selection_previews.pause_after_take.lock().take();
        if let Some((reached, resume)) = pause {
            reached.notify_one();
            resume.notified().await;
        }
    }

    // The labels the preview froze must still be there; a stale preview is
    // dropped, a store error leaves it usable.
    let labels = match previewed.labels.as_ref() {
        None => None,
        Some(frozen) => match check_frozen_labels(state, frozen).await {
            Ok(Ok(edits)) => Some(edits),
            Ok(Err(stale)) => return Err(stale.into()),
            Err(error) => {
                state.selection_previews.put_back(preview_token, previewed);
                return Err(error);
            }
        },
    };

    let job = mutations::queued_job(
        job_id.to_string(),
        mutation,
        previewed.ids.len() as u32,
        Some(selection),
        &previewed.accounts,
    );
    match mutations::claim_job(state, &job).await {
        Ok(true) => {}
        Ok(false) => {
            state.selection_previews.put_back(preview_token, previewed);
            return match existing_job(state, job_id, selection, mutation).await? {
                Some(job) => Ok(ResponseData::JobStarted { job }),
                None => Err(format!("job_id {job_id} is taken; try again").into()),
            };
        }
        Err(error) => {
            state.selection_previews.put_back(preview_token, previewed);
            return Err(error);
        }
    }

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
    spec.labels = labels;
    spec.activity = Some(JobActivity {
        source,
        action: action_token(mutation),
        account_id: account_id.as_ref().map(AccountId::as_str),
        query: Some(query.clone()),
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
        return Ok(Some(job_detail(job)));
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
            labels: None,
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
