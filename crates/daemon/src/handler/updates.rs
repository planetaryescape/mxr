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
use mxr_core::id::{AccountId, MessageId};
use mxr_core::MessageFlags;
use mxr_protocol::{
    ModeKindData, ResponseData, SenderKindData, UpdateSourceChangeData, UpdateSourceSettingData,
    UpdatesDigestData, UpdatesLetGoData,
};
use mxr_reader::{clean, ReaderConfig};
use mxr_store::{PlaceMessage, SourceLetGo, TodoRecord, TodoState, UpdateFactRow};
use mxr_todo::provenance::{FieldProvenance, FieldSource, FieldSources};
use mxr_updates::{derive, Cuts, Fact, FactInput, NeedsYou, RULES_VERSION};
use std::collections::{HashMap, HashSet};

/// The mode name `mode_views` and the guide use.
pub(super) const MODE: &str = "updates";
/// How far back a source's history is read for "new source" and deltas.
const HISTORY_DAYS: i64 = 365;
/// Earlier messages read per sender for that history.
const HISTORY_PER_SENDER: u32 = 20;
/// Only mail this recent breaks through to To do: a backfill page of old
/// alerts never floods it.
const BREAKTHROUGH_MAX_AGE_DAYS: i64 = 2;

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
) -> Result<(UpdatesDigestData, Vec<Item>), HandlerError>
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
        threads.sort_unstable_by(|a, b| a.as_str().cmp(&b.as_str()));
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
    let digest = updates_digest::compose(&DigestInputs {
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
    Ok((digest, items))
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
    let (digest, _) = digest_from(state, &accounts, placed, &scope, now, tz).await?;
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
    let (digest, items) = digest_from(state, &accounts, placed, &scope, now, tz).await?;
    let selection = updates_digest::selection(&items, &scope, &digest);
    if let Some(expected) = selection_token.filter(|token| !token.is_empty()) {
        if expected != selection.token {
            return Err(HandlerError::InvalidRequest(
                "This digest changed since the preview. Preview it again, then let go.".into(),
            ));
        }
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
    let done =
        mode_done::let_go_updates(state, &selection.thread_ids, &selection.keep, dry_run).await?;
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
        .and_then(|row| parse_setting(&row.setting))
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

pub(super) fn parse_setting(value: &str) -> Option<UpdateSourceSettingData> {
    UpdateSourceSettingData::parse(value)
}

// ---------------------------------------------------------------------------
// On arrival
// ---------------------------------------------------------------------------

/// Read newly synced Updates mail into facts and send what needs you to
/// To do at once. Never errors the caller; failures are logged, like the
/// delivery and to-do scans.
pub(crate) async fn scan_messages(state: &AppState, message_ids: &[MessageId]) {
    if message_ids.is_empty() {
        return;
    }
    match scan(state, message_ids, Utc::now()).await {
        Ok(created) if created > 0 => {
            tracing::info!(created, "updates broke through to To do");
        }
        Ok(_) => {}
        Err(error) => tracing::warn!(%error, "updates scan failed"),
    }
}

/// Facts for new Updates mail, then the breakthroughs. Returns how many
/// to-dos it made.
pub(super) async fn scan(
    state: &AppState,
    message_ids: &[MessageId],
    now: DateTime<Utc>,
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
    let mut created = 0;
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
        if updates.is_empty() {
            continue;
        }
        let facts = facts_for(state, &updates, &Local).await?;
        let settings = state.store.update_sources(&account).await?;
        for message in updates {
            let Some(fact) = facts.get(&message.id) else {
                continue;
            };
            let breakthrough = settings
                .get(&fact.source_key)
                .is_some_and(|row| row.setting == UpdateSourceSettingData::Breakthrough.as_str());
            if fact.needs_you.is_none() && !breakthrough {
                continue;
            }
            if break_through(state, message, fact, now).await? {
                created += 1;
            }
        }
    }
    created += deliveries_break_through(state, now).await?;
    Ok(created)
}

/// The to-do an update becomes on arrival, claimed by its dedup key so a
/// re-run or a re-sync never makes a second. Expired or old mail never
/// breaks through, and mail To do's own rules already caught is left to
/// them.
async fn break_through(
    state: &AppState,
    message: &PlaceMessage,
    fact: &Fact,
    now: DateTime<Utc>,
) -> Result<bool, HandlerError> {
    if message.date < now - Duration::days(BREAKTHROUGH_MAX_AGE_DAYS)
        || fact
            .window
            .as_ref()
            .is_some_and(|window| window.until < now)
    {
        return Ok(false);
    }
    let dedup_key = format!("update|{}", message.id);
    if state
        .store
        .get_todo_by_dedup(&message.account_id, &dedup_key)
        .await?
        .is_some()
    {
        return Ok(false);
    }
    let already = state
        .store
        .open_todos_for_threads(
            &message.account_id,
            std::slice::from_ref(&message.thread_id),
        )
        .await?
        .iter()
        .any(|todo| todo.source_message_id.as_ref() == Some(&message.id));
    if already {
        return Ok(false);
    }
    let (kind, verb, label) = match fact.needs_you {
        Some(NeedsYou::PaymentFailed) => ("payment_failed", "fix", NeedsYou::PaymentFailed.label()),
        Some(needs) => ("other", "check", needs.label()),
        None => ("other", "check", "a source you set to breakthrough"),
    };
    let until = fact.window.as_ref().map_or(
        message.date + Duration::days(BREAKTHROUGH_MAX_AGE_DAYS),
        |w| w.until,
    );
    let record = breakthrough_record(BreakthroughRow {
        account_id: &message.account_id,
        thread_id: Some(&message.thread_id),
        message_id: Some(&message.id),
        date: message.date,
        title: &fact.todo_title,
        kind,
        verb,
        counterparty: &fact.source_name,
        sender_domain: mxr_updates::email_domain(&message.from_email),
        reason: format!("{label} from {} (rule)", fact.source_name),
        relevant_until: until,
        dedup_key,
        now,
    });
    state.store.insert_todo(&record).await?;
    Ok(true)
}

struct BreakthroughRow<'a> {
    account_id: &'a AccountId,
    thread_id: Option<&'a mxr_core::id::ThreadId>,
    message_id: Option<&'a MessageId>,
    date: DateTime<Utc>,
    title: &'a str,
    kind: &'a str,
    verb: &'a str,
    counterparty: &'a str,
    sender_domain: Option<String>,
    reason: String,
    relevant_until: DateTime<Utc>,
    dedup_key: String,
    now: DateTime<Utc>,
}

fn breakthrough_record(row: BreakthroughRow<'_>) -> TodoRecord {
    let mut fields = FieldSources::default();
    let rule = || FieldProvenance::with_evidence(FieldSource::Rule, "Updates breakthrough rule");
    fields.set("title", rule());
    fields.set("kind", rule());
    fields.set(
        "relevant_until",
        FieldProvenance::with_evidence(FieldSource::Table, "the update's relevancy window"),
    );
    TodoRecord {
        id: mxr_todo::pass::new_todo_id(),
        account_id: row.account_id.clone(),
        thread_id: row.thread_id.cloned(),
        source_message_id: row.message_id.cloned(),
        source_date: Some(row.date),
        kind: row.kind.to_string(),
        verb: row.verb.to_string(),
        doc_type: None,
        title: mxr_updates::text::clip(row.title, 120),
        counterparty: Some(row.counterparty.to_string()),
        sender_domain: row.sender_domain,
        amount_minor: None,
        currency: None,
        due_at: None,
        due_words: None,
        act_by_at: Some(row.now),
        surface_at: Some(row.now),
        scheduled_for: None,
        action_url: None,
        action_domain: None,
        relevant_until: Some(row.relevant_until),
        window_source: Some("rule".to_string()),
        state: TodoState::Open,
        expired_at: None,
        expired_at_birth: false,
        catchup: None,
        looks_done_message_id: None,
        looks_done_reason: None,
        origin: "rule".to_string(),
        reason: row.reason,
        field_sources: fields.to_json(),
        user_edited: false,
        commitment_id: None,
        rules_version: mxr_todo::RULES_VERSION,
        dedup_key: row.dedup_key,
        surfaced_at: None,
        created_at: row.now,
        updated_at: row.now,
        done_at: None,
        dismissed_at: None,
    }
}

/// A parcel that went wrong (an exception, a failed attempt, a return)
/// goes to To do once per state, while the news is fresh.
async fn deliveries_break_through(
    state: &AppState,
    now: DateTime<Utc>,
) -> Result<usize, HandlerError> {
    let mut created = 0;
    for delivery in state
        .store
        .list_deliveries(mxr_store::DeliveryListFilter::All)
        .await?
    {
        let Some(status) = mxr_deliveries::DeliveryStatus::parse(&delivery.status) else {
            continue;
        };
        if !updates_digest::parcel_went_wrong(status)
            || delivery.last_event_at < now - Duration::days(BREAKTHROUGH_MAX_AGE_DAYS)
        {
            continue;
        }
        let dedup_key = format!("update|delivery|{}|{}", delivery.id, delivery.status);
        if state
            .store
            .get_todo_by_dedup(&delivery.account_id, &dedup_key)
            .await?
            .is_some()
        {
            continue;
        }
        let name = updates_digest::parcel_name(&delivery);
        let message_id = state
            .store
            .delivery_message_ids(&delivery.id)
            .await?
            .into_iter()
            .last();
        let record = breakthrough_record(BreakthroughRow {
            account_id: &delivery.account_id,
            thread_id: delivery.thread_id.as_ref(),
            message_id: message_id.as_ref(),
            date: delivery.last_event_at,
            title: &format!("Check delivery from {name}"),
            kind: "other",
            verb: "check",
            counterparty: &name,
            sender_domain: None,
            reason: format!("{} from {name} (rule)", NeedsYou::DeliveryException.label()),
            relevant_until: delivery.last_event_at + Duration::days(7),
            dedup_key,
            now,
        });
        state.store.insert_todo(&record).await?;
        created += 1;
    }
    Ok(created)
}
