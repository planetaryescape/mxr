//! Updates as a twice-daily briefing (blueprint 22, phase 4): IPC
//! handlers, the per-message fact cache and the breakthrough to To do.
//!
//! Membership is Updates' own: automated inbox mail the sender classifier
//! puts in Updates, minus its done marks (`modes::inbox_modes`), so the
//! digest, the rail, Now's card and `SetModeDone` agree on what Updates
//! holds. Each message is read once into a fact by `mxr-updates` rules and
//! cached in `update_facts`; the digest is built from those facts by
//! `updates_digest`. Letting go of a digest goes through `mode_done`, so
//! the archive-on-last-done rule, its toasts and undo are the same as `e`
//! anywhere else, and the preview selects exactly the messages the run
//! lets go.

use super::modes::inbox_modes;
use super::places::{scoped_accounts, AccountKinds, Placed};
use super::updates_digest::{self, DigestInputs, Scope};
use super::{mode_done, HandlerError, HandlerResult};
use crate::state::AppState;
use chrono::{DateTime, Duration, Local, TimeZone, Utc};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::MessageFlags;
use mxr_protocol::{
    ModeKindData, ResponseData, SenderKindData, UpdateSourceChangeData, UpdateSourceSettingData,
    UpdatesDigestData, UpdatesLetGoData,
};
use mxr_reader::{clean, ReaderConfig};
use mxr_store::{PlaceMessage, SourceLetGo, TodoRecord, UpdateFactRow};
use mxr_updates::{derive, Cuts, Fact, FactInput, RULES_VERSION};
use std::collections::{HashMap, HashSet};

/// The mode name `mode_views` and the guide use.
pub(super) const MODE: &str = "updates";
/// How far back a source's history is read for "new source" and deltas.
const HISTORY_DAYS: i64 = 365;
/// Earlier messages read per sender for that history.
const HISTORY_PER_SENDER: u32 = 20;

/// One automated message in Updates, with its fact.
pub(super) struct Item {
    pub message: PlaceMessage,
    /// The classifier's reason: "automated sender, not a person".
    pub reason: String,
    /// You chose this sender's kind.
    pub corrected: bool,
    pub fact: Fact,
}

/// The configured cuts; a bad list falls back to 08:00 and 16:30.
pub(super) fn cuts(state: &AppState) -> Cuts {
    let configured = state.config_snapshot().updates.cuts;
    Cuts::parse(&configured).unwrap_or_else(|error| {
        tracing::warn!(%error, "updates.cuts is invalid; using 08:00 and 16:30");
        Cuts::default()
    })
}

/// The facts of these messages: cached ones at the current rules version,
/// the rest derived now and cached. A failed cache write is logged; the
/// facts are still returned.
pub(super) async fn facts_for<Tz: TimeZone>(
    state: &AppState,
    messages: &[&PlaceMessage],
    tz: &Tz,
) -> Result<HashMap<MessageId, Fact>, HandlerError> {
    let ids: Vec<MessageId> = messages.iter().map(|m| m.id.clone()).collect();
    let cached = state.store.update_facts_for(&ids).await?;
    let mut facts = HashMap::with_capacity(messages.len());
    let mut stale: Vec<&PlaceMessage> = Vec::new();
    for message in messages {
        let fresh = cached
            .get(&message.id)
            .filter(|row| row.rules_version == RULES_VERSION)
            .and_then(|row| serde_json::from_str::<Fact>(&row.fact_json).ok());
        match fresh {
            Some(fact) => {
                facts.insert(message.id.clone(), fact);
            }
            None => stale.push(message),
        }
    }
    if stale.is_empty() {
        return Ok(facts);
    }
    let stale_ids: Vec<MessageId> = stale.iter().map(|m| m.id.clone()).collect();
    let bodies = state.store.update_body_texts(&stale_ids).await?;
    let mut rows = Vec::with_capacity(stale.len());
    for message in stale {
        let body = bodies.get(&message.id).map(|(plain, html)| {
            clean(plain.as_deref(), html.as_deref(), &ReaderConfig::default()).content
        });
        let fact = derive(
            &FactInput {
                subject: &message.subject,
                body: body.as_deref(),
                snippet: &message.snippet,
                from_email: &message.from_email,
                from_name: message.from_name.as_deref(),
                list_id: message.list_id.as_deref(),
                date: message.date,
            },
            tz,
        );
        match serde_json::to_string(&fact) {
            Ok(fact_json) => rows.push(UpdateFactRow {
                message_id: message.id.clone(),
                account_id: message.account_id.clone(),
                source_key: fact.source_key.clone(),
                template_key: fact.template_key.clone(),
                message_date: message.date,
                relevant_until: fact.window.as_ref().map(|window| window.until),
                fact_json,
                rules_version: RULES_VERSION,
            }),
            Err(error) => tracing::warn!(%error, "could not encode an update fact"),
        }
        facts.insert(message.id.clone(), fact);
    }
    if let Err(error) = state.store.upsert_update_facts(&rows, Utc::now()).await {
        tracing::warn!(%error, "could not cache update facts");
    }
    Ok(facts)
}

/// Placed Updates mail as items with facts.
pub(super) async fn items_for<Tz: TimeZone>(
    state: &AppState,
    placed: Vec<Placed>,
    tz: &Tz,
) -> Result<Vec<Item>, HandlerError> {
    let messages: Vec<&PlaceMessage> = placed.iter().map(|item| &item.message).collect();
    let mut facts = facts_for(state, &messages, tz).await?;
    Ok(placed
        .into_iter()
        .filter_map(|item| {
            let fact = facts.remove(&item.message.id)?;
            Some(Item {
                reason: item.kind.reason,
                corrected: item.kind.corrected,
                message: item.message,
                fact,
            })
        })
        .collect())
}

/// Earlier mail from the senders of `items`, inbox or not, as facts by
/// account and source, oldest first: what "new source" and deltas compare
/// with.
pub(super) async fn histories<Tz: TimeZone>(
    state: &AppState,
    items: &[Item],
    tz: &Tz,
) -> Result<updates_digest::Histories, HandlerError> {
    let mut by_account: HashMap<&AccountId, (HashSet<String>, DateTime<Utc>)> = HashMap::new();
    for item in items {
        let entry = by_account
            .entry(&item.message.account_id)
            .or_insert_with(|| (HashSet::new(), item.message.date));
        entry.0.insert(item.message.from_email.to_ascii_lowercase());
        entry.1 = entry.1.min(item.message.date);
    }
    let mut out = updates_digest::Histories::default();
    for (account, (senders, oldest)) in by_account {
        let mut senders: Vec<String> = senders.into_iter().collect();
        senders.sort_unstable();
        let earlier: Vec<PlaceMessage> = state
            .store
            .recent_from_senders(
                account,
                &senders,
                oldest - Duration::days(HISTORY_DAYS),
                HISTORY_PER_SENDER,
            )
            .await?
            .into_iter()
            .filter(|m| m.direction != "outbound")
            .collect();
        let refs: Vec<&PlaceMessage> = earlier.iter().collect();
        let facts = facts_for(state, &refs, tz).await?;
        for message in &earlier {
            if let Some(fact) = facts.get(&message.id) {
                out.add(account, message, fact);
            }
        }
    }
    for item in items {
        out.add(&item.message.account_id, &item.message, &item.fact);
    }
    out.sort();
    Ok(out)
}

/// The digest at `now` in `tz`, over `accounts`, built from Updates'
/// placed mail. Shared by `GetUpdatesDigest`, Now's card and the rail.
pub(super) async fn digest_from<Tz: TimeZone>(
    state: &AppState,
    accounts: &[AccountId],
    placed: Vec<Placed>,
    scope: &Scope,
    now: DateTime<Utc>,
    tz: &Tz,
) -> Result<(UpdatesDigestData, Vec<Item>, updates_digest::Selection), HandlerError>
where
    Tz::Offset: std::fmt::Display,
{
    let started = std::time::Instant::now();
    let placed = without_let_go(state, accounts, placed).await?;
    let items = items_for(state, placed, tz).await?;
    let histories = histories(state, &items, tz).await?;
    let mut sources = HashMap::new();
    let mut todos: Vec<TodoRecord> = Vec::new();
    let mut deliveries = Vec::new();
    let all_deliveries = state
        .store
        .list_deliveries(mxr_store::DeliveryListFilter::All)
        .await?;
    for account in accounts {
        for (key, row) in state.store.update_sources(account).await? {
            sources.insert((account.clone(), key), row);
        }
        let mut threads: Vec<_> = items
            .iter()
            .filter(|item| &item.message.account_id == account)
            .map(|item| item.message.thread_id.clone())
            .collect();
        threads.extend(
            all_deliveries
                .iter()
                .filter(|d| &d.account_id == account)
                .filter_map(|d| d.thread_id.clone()),
        );
        threads.sort_unstable_by_key(ThreadId::as_str);
        threads.dedup();
        for chunk in threads.chunks(500) {
            todos.extend(state.store.open_todos_for_threads(account, chunk).await?);
        }
    }
    for delivery in all_deliveries {
        if accounts.contains(&delivery.account_id) && delivery.dismissed_at.is_none() {
            deliveries.push(delivery);
        }
    }
    let never_had_any = items.is_empty() && !state.store.any_update_facts(accounts).await?;
    let seen_key = seen_key(scope.account_id.as_ref());
    let last_seen = state.store.mode_last_viewed(&seen_key).await?;
    let (digest, selection) = updates_digest::compose(&DigestInputs {
        items: &items,
        histories: &histories,
        sources: &sources,
        todos: &todos,
        deliveries: &deliveries,
        cuts: &cuts(state),
        scope,
        last_seen,
        never_had_any,
        now,
        tz,
    });
    tracing::debug!(
        items = items.len(),
        elapsed_ms = started.elapsed().as_secs_f64() * 1000.0,
        "updates digest composed"
    );
    Ok((digest, items, selection))
}

/// Drop each message a thread's Updates mark already saw. Membership
/// keeps a thread in Updates while any of its Updates mail is new; the
/// digest shows only that new mail, so a let go of one cut never brings
/// its messages back with the next.
async fn without_let_go(
    state: &AppState,
    accounts: &[AccountId],
    placed: Vec<Placed>,
) -> Result<Vec<Placed>, HandlerError> {
    let mut marks = HashMap::new();
    for account in accounts {
        marks.insert(
            account.clone(),
            state.store.mode_done_marks(account, MODE).await?,
        );
    }
    Ok(placed
        .into_iter()
        .filter(|item| {
            marks
                .get(&item.message.account_id)
                .and_then(|marks| marks.get(&item.message.thread_id))
                .is_none_or(|mark| !mark.saw(item.message.date, &item.message.id))
        })
        .collect())
}

fn seen_key(account_id: Option<&AccountId>) -> String {
    account_id.map_or_else(|| MODE.to_string(), |account| format!("{MODE}:{account}"))
}

/// Updates' inbox mail across `accounts`, after its done marks.
async fn placed_updates(
    state: &AppState,
    accounts: &[AccountId],
) -> Result<Vec<Placed>, HandlerError> {
    Ok(inbox_modes(state, accounts).await?.updates)
}

pub(super) async fn get_digest(
    state: &AppState,
    account_id: Option<&AccountId>,
    cut: Option<DateTime<Utc>>,
    mark_seen: bool,
    expired: bool,
) -> HandlerResult {
    let digest = digest_at(
        state,
        account_id,
        cut,
        mark_seen,
        expired,
        Utc::now(),
        &Local,
    )
    .await?;
    Ok(ResponseData::UpdatesDigest { digest })
}

/// The digest as it stands at `now` in `tz` (tests move the clock).
pub(super) async fn digest_at<Tz: TimeZone>(
    state: &AppState,
    account_id: Option<&AccountId>,
    cut: Option<DateTime<Utc>>,
    mark_seen: bool,
    expired: bool,
    now: DateTime<Utc>,
    tz: &Tz,
) -> Result<UpdatesDigestData, HandlerError>
where
    Tz::Offset: std::fmt::Display,
{
    let accounts = scoped_accounts(state, account_id).await?;
    let placed = placed_updates(state, &accounts).await?;
    let scope = Scope {
        account_id: account_id.cloned(),
        cut,
        source_key: None,
        list_expired: expired,
    };
    // Boxed: the digest's future is deep, and a debug build's request
    // future overflowed a worker's stack without it.
    let (digest, _, _) = Box::pin(digest_from(state, &accounts, placed, &scope, now, tz)).await?;
    if mark_seen {
        state
            .store
            .set_mode_viewed(&seen_key(account_id), now)
            .await?;
        for line in digest
            .needs_a_look
            .iter()
            .chain(&digest.changed)
            .chain(&digest.routine)
            .filter(|line| line.suggestion.is_some())
        {
            state
                .store
                .mark_update_source_suggested(&line.account_id, &line.source_key, now)
                .await?;
        }
    }
    Ok(digest)
}

/// Let go of a cut, or of one source in it. The selection is the digest's
/// own, so a dry run and the run that follows act on the same messages;
/// a token from the preview makes the run refuse if the cut changed.
pub(super) async fn let_go(
    state: &AppState,
    account_id: Option<&AccountId>,
    cut: Option<DateTime<Utc>>,
    source_key: Option<&str>,
    selection_token: Option<&str>,
    dry_run: bool,
) -> HandlerResult {
    let result = let_go_at(
        state,
        &LetGo {
            account_id,
            cut,
            source_key,
            selection_token,
            dry_run,
        },
        Utc::now(),
        &Local,
    )
    .await?;
    Ok(ResponseData::UpdatesLetGo { result })
}

/// One `LetGoDigest`, as the handler hands it over.
pub(super) struct LetGo<'a> {
    pub account_id: Option<&'a AccountId>,
    pub cut: Option<DateTime<Utc>>,
    pub source_key: Option<&'a str>,
    pub selection_token: Option<&'a str>,
    pub dry_run: bool,
}

/// Let go as it stands at `now` in `tz` (tests move the clock).
pub(super) async fn let_go_at<Tz: TimeZone>(
    state: &AppState,
    request: &LetGo<'_>,
    now: DateTime<Utc>,
    tz: &Tz,
) -> Result<UpdatesLetGoData, HandlerError>
where
    Tz::Offset: std::fmt::Display,
{
    let LetGo {
        account_id,
        cut,
        source_key,
        selection_token,
        dry_run,
    } = *request;
    let accounts = scoped_accounts(state, account_id).await?;
    let placed = placed_updates(state, &accounts).await?;
    let scope = Scope {
        account_id: account_id.cloned(),
        cut,
        source_key: source_key.map(str::to_string),
        list_expired: false,
    };
    let (digest, items, selection) =
        Box::pin(digest_from(state, &accounts, placed, &scope, now, tz)).await?;
    // The run lets go of exactly what a preview listed: its token hashes
    // the cut and that message id set, and a run without one is refused.
    match selection_token.filter(|token| !token.is_empty()) {
        Some(expected) if expected != selection.token => {
            return Err(HandlerError::InvalidRequest(
                "This digest changed since the preview. Preview it again, then let go.".into(),
            ));
        }
        None if !dry_run => {
            return Err(HandlerError::InvalidRequest(
                "Preview first (dry_run), then let go with the preview's selection_token.".into(),
            ));
        }
        _ => {}
    }
    if selection.thread_ids.is_empty() {
        return Ok(UpdatesLetGoData {
            dry_run,
            cut_at: digest.cut.at,
            line: "Nothing to let go of in this digest.".to_string(),
            message_count: 0,
            source_count: 0,
            hidden_count: 0,
            in_todo_count: 0,
            thread_ids: Vec::new(),
            message_ids: Vec::new(),
            selection_token: selection.token,
            items: Vec::new(),
            mutation_id: None,
            undo_unavailable: false,
        });
    }
    let chosen: HashSet<MessageId> = selection.message_ids.iter().cloned().collect();
    let done = Box::pin(mode_done::let_go_updates(
        state,
        &selection.thread_ids,
        &chosen,
        dry_run,
    ))
    .await?;
    let ResponseData::ModeDone {
        items: outcomes,
        mutation_id,
        undo_unavailable,
        ..
    } = done
    else {
        return Err(HandlerError::from("unexpected response from mode done"));
    };
    let in_todo_count = outcomes
        .iter()
        .filter(|outcome| outcome.still_in.contains(&ModeKindData::Todo))
        .count();
    if !dry_run {
        record_streaks(state, &items, &selection, digest.cut.at, now).await;
        super::mode_guide::retire(state, MODE).await?;
    }
    Ok(UpdatesLetGoData {
        dry_run,
        cut_at: digest.cut.at,
        line: updates_digest::let_go_line(
            selection.message_ids.len(),
            selection.source_count,
            selection.hidden,
            in_todo_count,
        ),
        message_count: super::now::count(selection.message_ids.len()),
        source_count: super::now::count(selection.source_count),
        hidden_count: super::now::count(selection.hidden),
        in_todo_count: super::now::count(in_todo_count),
        thread_ids: selection.thread_ids,
        message_ids: selection.message_ids,
        selection_token: selection.token,
        items: outcomes,
        mutation_id,
        undo_unavailable,
    })
}

/// Count this let go toward each source's "let go without opening"
/// streak. A failure is logged: the streak only feeds a suggestion.
async fn record_streaks(
    state: &AppState,
    items: &[Item],
    selection: &updates_digest::Selection,
    cut_at: DateTime<Utc>,
    now: DateTime<Utc>,
) {
    let chosen: HashSet<&MessageId> = selection.message_ids.iter().collect();
    let mut by_account: HashMap<&AccountId, HashMap<&str, bool>> = HashMap::new();
    for item in items
        .iter()
        .filter(|item| chosen.contains(&item.message.id))
    {
        let opened = item.message.flags.contains(MessageFlags::READ);
        *by_account
            .entry(&item.message.account_id)
            .or_default()
            .entry(item.fact.source_key.as_str())
            .or_default() |= opened;
    }
    for (account, sources) in by_account {
        let sources: Vec<SourceLetGo> = sources
            .into_iter()
            .map(|(key, opened)| SourceLetGo {
                source_key: key.to_string(),
                opened,
            })
            .collect();
        if let Err(error) = state
            .store
            .record_update_let_go(account, &sources, cut_at, now)
            .await
        {
            tracing::warn!(%error, "could not count the let go toward the mute suggestion");
        }
    }
}

/// Tune a source. `source` is a key or a sender address; an address maps
/// to the key its mail derives.
pub(super) async fn set_source(
    state: &AppState,
    account_id: Option<&AccountId>,
    source: &str,
    setting: UpdateSourceSettingData,
    dry_run: bool,
) -> HandlerResult {
    let source = source.trim();
    if source.is_empty() {
        return Err(HandlerError::InvalidRequest(
            "Name a source, like github.com/acme/api or an address it sends from.".into(),
        ));
    }
    let key = if source.contains('@') {
        mxr_updates::email_domain(source).unwrap_or_else(|| source.to_ascii_lowercase())
    } else {
        source.to_ascii_lowercase()
    };
    let account = match account_id {
        Some(account) => account.clone(),
        None => {
            let accounts = scoped_accounts(state, None).await?;
            match accounts.as_slice() {
                [only] => only.clone(),
                [] => return Err(HandlerError::InvalidRequest("No account is set up.".into())),
                _ => {
                    return Err(HandlerError::InvalidRequest(
                        "More than one account: name one with --account.".into(),
                    ))
                }
            }
        }
    };
    let prior_row = state.store.update_sources(&account).await?.remove(&key);
    let prior = prior_row
        .as_ref()
        .and_then(|row| UpdateSourceSettingData::parse(&row.setting))
        .unwrap_or_default();
    if !dry_run {
        state
            .store
            .set_update_source(&account, &key, setting.as_str(), Utc::now())
            .await?;
    }
    Ok(ResponseData::UpdateSource {
        change: UpdateSourceChangeData {
            dry_run,
            account_id: account,
            copy: format!("{key}: {}.", setting.effect()),
            source_key: key,
            setting,
            prior,
        },
    })
}

/// Derive and cache the facts the digest will read: Updates' inbox mail
/// and its senders' history. Returns whether it finished; a failure is
/// logged and tried again on the next tick.
pub(crate) async fn warm(state: &AppState) -> bool {
    let started = std::time::Instant::now();
    let result = async {
        let accounts = scoped_accounts(state, None).await?;
        let placed = placed_updates(state, &accounts).await?;
        let placed = without_let_go(state, &accounts, placed).await?;
        let items = items_for(state, placed, &Local).await?;
        histories(state, &items, &Local).await?;
        Ok::<usize, HandlerError>(items.len())
    }
    .await;
    match result {
        Ok(items) => {
            tracing::info!(
                items,
                elapsed_ms = started.elapsed().as_secs_f64() * 1000.0,
                "updates facts warmed"
            );
            true
        }
        Err(error) => {
            tracing::warn!(%error, "could not warm updates facts");
            false
        }
    }
}

// ---------------------------------------------------------------------------
// On arrival
// ---------------------------------------------------------------------------

/// Read newly synced Updates mail into facts, so the next digest doesn't
/// pay for them. Nothing here makes a to-do: an alert that needs you is a
/// suggestion at the top of Needs a look, and `t` is yours to press.
/// Never errors the caller; failures are logged, like the delivery and
/// to-do scans.
pub(crate) async fn scan_messages(state: &AppState, message_ids: &[MessageId]) {
    if message_ids.is_empty() {
        return;
    }
    if let Err(error) = scan(state, message_ids).await {
        tracing::warn!(%error, "updates scan failed");
    }
}

/// Facts for new Updates mail. Returns how many messages it read.
pub(super) async fn scan(
    state: &AppState,
    message_ids: &[MessageId],
) -> Result<usize, HandlerError> {
    let mut by_account: HashMap<AccountId, Vec<PlaceMessage>> = HashMap::new();
    for id in message_ids {
        if let Some(message) = state.store.place_message(id).await? {
            by_account
                .entry(message.account_id.clone())
                .or_default()
                .push(message);
        }
    }
    let mut read = 0;
    for (account, messages) in by_account {
        let mut senders: Vec<String> = messages
            .iter()
            .map(|m| m.from_email.to_ascii_lowercase())
            .collect();
        senders.sort_unstable();
        senders.dedup();
        let kinds = AccountKinds::load(state, &account, &senders).await?;
        let updates: Vec<&PlaceMessage> = messages
            .iter()
            .filter(|m| !m.is_delivery && !m.is_invite && !kinds.is_outbound(m))
            .filter(|m| {
                super::mail_kind::classify(&kinds.signals(m)).kind.to_data()
                    == SenderKindData::PaperTrail
            })
            .collect();
        read += facts_for(state, &updates, &Local).await?.len();
    }
    Ok(read)
}
