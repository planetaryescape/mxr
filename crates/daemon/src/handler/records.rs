//! Archive: IPC handlers, the post-sync scan and the background tick.
//!
//! Detection, grouping, the answer box's ranking and the export live in
//! `mxr-records`; this module runs them against the daemon's store and
//! clock and serves the ledger and cards. Every mutation takes `dry_run`,
//! and a preview runs the same write inside a transaction that is rolled
//! back, so what was previewed is what changes.

use super::places::scoped_accounts;
use super::{HandlerError, HandlerResult};
use crate::state::AppState;
use chrono::{DateTime, Datelike, Local, NaiveDate, TimeZone, Timelike, Utc};
use mxr_core::id::{AccountId, AttachmentId, MessageId};
use mxr_protocol::{
    archive_copy, ArchiveAskFiltersData, RecordAmountData, RecordAnswerCardData, RecordAnswerData,
    RecordAnswerListData, RecordAnswerModeData, RecordChangeData, RecordData, RecordDocumentData,
    RecordEditData, RecordExportData, RecordFacetCountData, RecordFacetsData, RecordFallbackData,
    RecordFieldData, RecordFilterData, RecordFirstRunData, RecordGroupData, RecordIssuerData,
    RecordKindData, RecordLedgerData, RecordMomentData, RecordMonthData, RecordSourceData,
    RecordUndoData, ResponseData,
};
use mxr_records::answer::{self, AnswerField, Candidate};
use mxr_records::export::{self, ExportRow};
use mxr_records::fields::FieldName;
use mxr_records::group::month_name;
use mxr_records::pass::{self, PassConfig};
use mxr_records::{RecordKind, Source, Stage};
use mxr_store::{
    ArchiveRecord, RecordDocument, RecordFieldEdit, RecordFieldValue, RecordFiled, RecordGroup,
    RecordQuery, RecordSource, TodoRecord,
};
use mxr_todo::money::format_amount;
use std::collections::{BTreeMap, HashMap};

/// History pages read per background tick, so a big mailbox is filed in
/// steps rather than one long hold on the writer.
const FIRST_RUN_PAGES_PER_TICK: u32 = 10;
const FIRST_RUN_PAGE_SIZE: u32 = 500;
/// PDFs downloaded per tick, so prefetch never crowds out sync.
const PDFS_PER_TICK: u32 = 8;
/// How many of a sender's emails "always file" reads at once.
const SENDER_BACKFILL: u32 = 1000;
/// Issuers in the facet list.
const ISSUER_FACETS: usize = 30;

pub(crate) fn pass_config(now: DateTime<Utc>) -> PassConfig<Local> {
    PassConfig { now, tz: Local }
}

fn enabled(state: &AppState) -> bool {
    state.config_snapshot().records.enabled
}

// ---------------------------------------------------------------------------
// Background work
// ---------------------------------------------------------------------------

/// File records from newly synced mail. Never errors the caller; failures
/// are logged, like the to-do scan.
pub(crate) async fn scan_messages(state: &AppState, message_ids: &[MessageId]) {
    if !enabled(state) || message_ids.is_empty() {
        return;
    }
    match pass::scan_messages(&state.store, &pass_config(Utc::now()), message_ids).await {
        Ok(summary) if summary.filed + summary.updated > 0 => {
            tracing::info!(
                filed = summary.filed,
                updated = summary.updated,
                "post-sync records scan"
            );
            reindex_record_sources(state, message_ids).await;
        }
        Ok(_) => {}
        Err(error) => tracing::warn!(%error, "records scan failed"),
    }
}

/// The semantic worker may have chunked these messages before they were
/// filed; ingesting them again adds Archive's field chunk. Unchanged chunks
/// are not re-embedded.
async fn reindex_record_sources(state: &AppState, message_ids: &[MessageId]) {
    let filed: Vec<MessageId> = match state
        .store
        .archive_record_ids_for_messages(message_ids)
        .await
    {
        Ok(rows) => {
            let mut ids: Vec<MessageId> = rows.into_iter().map(|(id, _)| id).collect();
            ids.sort_by_key(MessageId::as_str);
            ids.dedup();
            ids
        }
        Err(error) => {
            tracing::warn!(%error, "looking up filed messages for reindex failed");
            return;
        }
    };
    if filed.is_empty() {
        return;
    }
    if let Err(error) = state.semantic.enqueue_ingest_messages(&filed).await {
        tracing::warn!(%error, "semantic reindex of filed records failed to enqueue");
    }
}

/// One background tick: advance each account's first run, file delivered
/// orders, add yearly subscriptions' renewal to-dos, then prefetch record
/// PDFs within the budget. Returns whether a
/// first run is still going, so the loop can come back sooner.
pub(crate) async fn tick(state: &AppState, now: DateTime<Utc>) -> Result<bool, HandlerError> {
    if !enabled(state) {
        return Ok(false);
    }
    let cfg = pass_config(now);
    let mut in_progress = false;
    for account in scoped_accounts(state, None).await? {
        let progress = pass::run_first_run(
            &state.store,
            &cfg,
            &account,
            FIRST_RUN_PAGE_SIZE,
            FIRST_RUN_PAGES_PER_TICK,
        )
        .await
        .map_err(|error| HandlerError::Message(error.to_string()))?;
        in_progress |= !progress.complete;
        let delivered = pass::file_delivered(&state.store, &cfg, &account)
            .await
            .map_err(|error| HandlerError::Message(error.to_string()))?;
        if delivered > 0 {
            pass::regroup(&state.store, &account, now)
                .await
                .map_err(|error| HandlerError::Message(error.to_string()))?;
        }
    }
    match super::record_subscriptions::file_renewals(state, now).await {
        Ok(0) => {}
        Ok(written) => tracing::debug!(written, "subscription renewals filed in To do"),
        Err(error) => tracing::warn!(%error, "filing subscription renewals failed"),
    }
    prefetch_pdfs(state).await;
    Ok(in_progress)
}

/// Downloads record PDFs that are not on disk, newest first, while the
/// total stays inside `records.pdf_budget_mb`. The semantic index then
/// reads their text. Failures (offline, a provider error) are logged and
/// retried next tick.
pub(crate) async fn prefetch_pdfs(state: &AppState) -> u32 {
    let config = state.config_snapshot().records;
    if !config.pdf_prefetch {
        return 0;
    }
    prefetch_pdfs_within(
        state,
        config.pdf_budget_mb.saturating_mul(1024 * 1024),
        config.pdf_max_file_mb.saturating_mul(1024 * 1024),
    )
    .await
}

/// The prefetch against a budget and a per-file cap in bytes. Each fetch
/// may write at most what is left of the budget (and the per-file cap);
/// the attachment's declared size only picks candidates, and the bytes
/// actually written are what count against the budget.
pub(crate) async fn prefetch_pdfs_within(state: &AppState, budget: u64, max_file: u64) -> u32 {
    let used = match state.store.record_pdfs_on_disk().await {
        Ok((bytes, _)) => u64::try_from(bytes).unwrap_or(0),
        Err(error) => {
            tracing::warn!(%error, "record PDF budget check failed");
            return 0;
        }
    };
    let mut remaining = budget.saturating_sub(used);
    let wanted = match state
        .store
        .record_pdfs_to_fetch(i64::try_from(max_file).unwrap_or(i64::MAX), PDFS_PER_TICK)
        .await
    {
        Ok(wanted) => wanted,
        Err(error) => {
            tracing::warn!(%error, "record PDF prefetch lookup failed");
            return 0;
        }
    };
    let mut fetched = Vec::new();
    for pdf in wanted {
        let declared = u64::try_from(pdf.size_bytes).unwrap_or(0);
        if remaining == 0 || declared > remaining {
            tracing::debug!(remaining, "record PDF budget reached");
            break;
        }
        let Ok(attachment_id) = pdf.attachment_id.parse::<AttachmentId>() else {
            continue;
        };
        let cap = remaining.min(max_file);
        match super::materialize_attachment_capped(
            state,
            &pdf.message_id,
            &attachment_id,
            Some(cap),
        )
        .await
        {
            Ok(super::Materialized::Written { bytes, .. }) => {
                remaining = remaining.saturating_sub(bytes);
                fetched.push(pdf.message_id);
            }
            Ok(super::Materialized::TooLarge { bytes }) => {
                // Record the real size, so the next tick doesn't fetch it
                // again: it is left until opened.
                tracing::debug!(message = %pdf.message_id, cap, bytes, "record PDF larger than it said");
                if let Err(error) = state
                    .store
                    .set_attachment_size(
                        &pdf.attachment_id,
                        i64::try_from(bytes).unwrap_or(i64::MAX),
                    )
                    .await
                {
                    tracing::warn!(%error, "recording a PDF's real size failed");
                }
            }
            Err(error) => {
                tracing::debug!(message = %pdf.message_id, %error, "record PDF prefetch skipped");
            }
        }
    }
    let count = u32::try_from(fetched.len()).unwrap_or(u32::MAX);
    if !fetched.is_empty() {
        if let Err(error) = state.semantic.enqueue_ingest_messages(&fetched).await {
            tracing::warn!(%error, "semantic reindex of record PDFs failed to enqueue");
        }
        tracing::info!(pdfs = count, "prefetched record PDFs");
    }
    count
}

/// Files the records ticked-off to-dos leave behind. Logged, never fails
/// the tick-off: the to-do is done either way.
pub(crate) async fn file_ticked_todos(state: &AppState, todos: &[TodoRecord]) -> u32 {
    if !enabled(state) {
        return 0;
    }
    let cfg = pass_config(Utc::now());
    let mut filed = 0;
    let mut accounts: Vec<AccountId> = Vec::new();
    for todo in todos {
        match pass::file_from_todo(&state.store, &cfg, todo).await {
            Ok(Some(RecordFiled::Inserted { .. } | RecordFiled::Updated { .. })) => {
                filed += 1;
                if !accounts.contains(&todo.account_id) {
                    accounts.push(todo.account_id.clone());
                }
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(todo = %todo.id, %error, "filing a ticked-off to-do failed");
            }
        }
    }
    for account in accounts {
        if let Err(error) = pass::regroup(&state.store, &account, cfg.now).await {
            tracing::warn!(%error, "regrouping records failed");
        }
    }
    filed
}

/// Undoes what [`file_ticked_todos`] filed, for reopened to-dos.
pub(crate) async fn unfile_todos(state: &AppState, todos: &[TodoRecord]) {
    for todo in todos {
        if let Err(error) = state
            .store
            .unfile_todo_record(&todo.account_id, &todo.id)
            .await
        {
            tracing::warn!(todo = %todo.id, %error, "unfiling a reopened to-do failed");
        }
    }
}

/// Subscription signals, then records with a moment soon, soonest first,
/// at most `cap`: the strip on Archive and the line on Now.
pub(super) async fn coming_up(
    state: &AppState,
    accounts: &[AccountId],
    now: DateTime<Utc>,
    cap: usize,
) -> Result<Vec<RecordMomentData>, HandlerError> {
    // Only records with a date still ahead: Now asks on every refresh.
    let records = state
        .store
        .list_archive_records(&RecordQuery {
            moment_after: Some(now),
            ..query_for(accounts, &RecordFilterData::default())?
        })
        .await?;
    let groups: HashMap<String, RecordGroup> = state
        .store
        .list_record_groups(Some(accounts))
        .await?
        .into_iter()
        .map(|g| (g.id.clone(), g))
        .collect();
    // A price change or a missed charge leads: it may want doing
    // something about, where a moment only wants knowing. Signals leave
    // one place for a moment, so a trip tomorrow is never crowded out.
    let moments: Vec<RecordMomentData> =
        mxr_records::coming_up::moments(&records, &groups, now, &Local)
            .into_iter()
            .map(moment_data)
            .collect();
    let mut out = super::record_subscriptions::signal_moments(state, accounts, now).await?;
    if !moments.is_empty() {
        out.truncate(cap.saturating_sub(1));
    }
    out.extend(moments);
    out.truncate(cap);
    Ok(out)
}

fn moment_data(moment: mxr_records::coming_up::Moment) -> RecordMomentData {
    RecordMomentData {
        kind: moment.kind.as_str().to_string(),
        record_id: moment.record_id,
        group_id: moment.group_id,
        at: moment.at,
        label: moment.label,
    }
}

/// Whether ticking off a to-do of this kind files a record.
pub(crate) fn todo_files_record(todo_kind: &str) -> bool {
    pass::todo_record_kind(todo_kind).is_some()
}

// ---------------------------------------------------------------------------
// Ids and scopes
// ---------------------------------------------------------------------------

pub(super) async fn resolve_record_id(state: &AppState, raw: &str) -> Result<String, HandlerError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(HandlerError::InvalidRequest(
            "Give a record id.".to_string(),
        ));
    }
    if state.store.get_archive_record(raw).await?.is_some() {
        return Ok(raw.to_string());
    }
    let prefixed;
    let prefix = if raw.starts_with("rec_") {
        raw
    } else {
        prefixed = format!("rec_{raw}");
        &prefixed
    };
    match state
        .store
        .find_record_ids_by_prefix(prefix, 2)
        .await?
        .as_slice()
    {
        [only] => Ok(only.clone()),
        [] => Err(HandlerError::InvalidRequest(format!(
            "No record matches {raw}."
        ))),
        _ => Err(HandlerError::InvalidRequest(format!(
            "{raw} matches more than one record; use more of the id."
        ))),
    }
}

async fn resolve_record_ids(state: &AppState, raw: &[String]) -> Result<Vec<String>, HandlerError> {
    if raw.is_empty() {
        return Err(HandlerError::InvalidRequest(
            "Give at least one record id.".to_string(),
        ));
    }
    let mut ids = Vec::with_capacity(raw.len());
    for id in raw {
        let id = resolve_record_id(state, id).await?;
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    Ok(ids)
}

/// The accounts these records belong to, for request scoping.
pub(super) async fn record_accounts(
    state: &AppState,
    raw_ids: &[String],
) -> Result<Vec<AccountId>, HandlerError> {
    let ids = resolve_record_ids(state, raw_ids).await?;
    let mut accounts = Vec::new();
    for record in state.store.get_archive_records(&ids).await? {
        if !accounts.contains(&record.account_id) {
            accounts.push(record.account_id);
        }
    }
    Ok(accounts)
}

// ---------------------------------------------------------------------------
// Turning rows into what clients draw
// ---------------------------------------------------------------------------

/// Everything a set of records needs to become `RecordData`, loaded in a
/// few queries.
struct Context {
    groups: HashMap<String, RecordGroup>,
    group_counts: HashMap<String, u32>,
    fields: HashMap<String, Vec<RecordFieldValue>>,
    documents: HashMap<String, Vec<RecordDocument>>,
    sources: HashMap<String, Vec<RecordSource>>,
}

impl Context {
    /// Loads what the records need in parallel: their fields, documents and
    /// sources, and the groups they belong to with member counts.
    async fn load(
        state: &AppState,
        records: &[ArchiveRecord],
        accounts: Option<&[AccountId]>,
    ) -> Result<Self, HandlerError> {
        let ids: Vec<String> = records.iter().map(|r| r.id.clone()).collect();
        let mut group_ids: Vec<String> =
            records.iter().filter_map(|r| r.group_id.clone()).collect();
        group_ids.sort_unstable();
        group_ids.dedup();
        let store = &state.store;
        let (groups, group_counts, field_rows, document_rows, source_rows) = tokio::try_join!(
            store.list_record_groups(accounts),
            store.record_group_counts(&group_ids),
            store.archive_record_fields(&ids),
            store.archive_record_documents(&ids),
            store.archive_record_sources(&ids),
        )?;
        let mut fields: HashMap<String, Vec<RecordFieldValue>> = HashMap::new();
        for (record_id, field) in field_rows {
            fields.entry(record_id).or_default().push(field);
        }
        let mut documents: HashMap<String, Vec<RecordDocument>> = HashMap::new();
        for document in document_rows {
            documents
                .entry(document.record_id.clone())
                .or_default()
                .push(document);
        }
        let mut sources: HashMap<String, Vec<RecordSource>> = HashMap::new();
        for source in source_rows {
            sources
                .entry(source.record_id.clone())
                .or_default()
                .push(source);
        }
        Ok(Self {
            groups: groups.into_iter().map(|g| (g.id.clone(), g)).collect(),
            group_counts,
            fields,
            documents,
            sources,
        })
    }
}

/// A day stored at midday UTC (see `mxr_records::fields::day_at`).
fn is_day(at: DateTime<Utc>) -> bool {
    at.hour() == 12 && at.minute() == 0 && at.second() == 0
}

/// "3 Mar 2025", or "Thu 12 Jun 2025 07:40" for an instant.
fn date_label(at: DateTime<Utc>) -> String {
    if is_day(at) {
        at.format("%-d %b %Y").to_string()
    } else {
        at.with_timezone(&Local)
            .format("%a %-d %b %Y %H:%M")
            .to_string()
    }
}

/// "7 Mar".
fn short_day(at: DateTime<Utc>) -> String {
    if is_day(at) {
        at.format("%-d %b").to_string()
    } else {
        at.with_timezone(&Local).format("%-d %b").to_string()
    }
}

pub(super) fn amount_data(minor: i64, currency: &str) -> RecordAmountData {
    RecordAmountData {
        minor,
        currency: currency.to_string(),
        display: format_amount(minor, currency),
    }
}

fn kind_data(kind: &str) -> RecordKindData {
    RecordKindData::parse(kind).unwrap_or(RecordKindData::Receipt)
}

fn field_data(kind: RecordKind, value: &RecordFieldValue) -> Option<RecordFieldData> {
    let name = FieldName::parse(&value.field)?;
    let (shown, copy) = match name {
        FieldName::Amount => {
            let minor = value.value_int?;
            let currency = value.value_text.as_deref()?;
            (
                format_amount(minor, currency),
                mxr_todo::money::plain_amount(minor),
            )
        }
        _ if name.is_date() => {
            let at = DateTime::from_timestamp(value.value_int?, 0)?;
            let label = date_label(at);
            (label.clone(), label)
        }
        _ => {
            let text = value.value_text.clone()?;
            (text.clone(), text)
        }
    };
    let label = match name {
        FieldName::Reference => kind.reference_label().to_string(),
        FieldName::Amount => kind.amount_label().to_string(),
        other => other.label().to_string(),
    };
    let source = Source::parse(&value.source);
    Some(RecordFieldData {
        field: value.field.clone(),
        label,
        value: shown,
        copy,
        source: value.source.clone(),
        source_label: source.describe().to_string(),
        checked: value.checked,
        evidence: value.evidence.clone(),
        message_id: value.message_id.clone(),
    })
}

/// The winning candidate per field, in the store's order.
fn winners(values: &[RecordFieldValue]) -> Vec<&RecordFieldValue> {
    let mut seen: Vec<&str> = Vec::new();
    let mut out = Vec::new();
    for value in values {
        if !seen.contains(&value.field.as_str()) {
            seen.push(&value.field);
            out.push(value);
        }
    }
    out
}

fn stage_line(record: &ArchiveRecord, sources: &[RecordSource]) -> Option<String> {
    if !matches!(record.kind.as_str(), "order" | "booking" | "ticket") || sources.len() < 2 {
        return None;
    }
    let mut stages: Vec<Stage> = Vec::new();
    for source in sources {
        let stage = Stage::parse(&source.stage);
        if stage != Stage::Other && !stages.contains(&stage) {
            stages.push(stage);
        }
    }
    stages.sort_by_key(|stage| stage.order());
    if stages.len() < 2 {
        return None;
    }
    Some(
        stages
            .iter()
            .map(|stage| match (stage, record.delivered_at) {
                (Stage::Delivered, Some(at)) => format!("delivered {}", short_day(at)),
                _ => stage.word().to_string(),
            })
            .collect::<Vec<_>>()
            .join(" · "),
    )
}

fn detail_line(record: &ArchiveRecord, now: DateTime<Utc>) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(start) = record.span_start {
        parts.push(match record.span_end {
            Some(end) if end.date_naive() != start.date_naive() => {
                format!("{} to {}", date_label(start), short_day(end))
            }
            _ => date_label(start),
        });
    }
    if let Some(by) = record.return_by {
        parts.push(if by < now {
            format!("Return by {} (passed)", short_day(by))
        } else {
            format!("Return by {}", short_day(by))
        });
    }
    if let Some(until) = record.warranty_until {
        parts.push(format!("Warranty to {}", date_label(until)));
    }
    if let Some(until) = record.valid_until {
        parts.push(format!("Valid until {}", date_label(until)));
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

fn why(record: &ArchiveRecord, now: DateTime<Utc>) -> String {
    let mut line = format!(
        "Here because: {} ({}).",
        record.reason,
        if record.checked {
            "checked"
        } else {
            "unchecked"
        }
    );
    if let Some(by) = record.return_by.filter(|by| *by >= now) {
        line.push_str(&format!(
            " Return window closes {}.",
            by.with_timezone(&Local).format("%a %-d %b")
        ));
    }
    line
}

fn document_data(document: &RecordDocument) -> RecordDocumentData {
    RecordDocumentData {
        message_id: document.message_id.clone(),
        attachment_id: document.attachment_id.clone(),
        filename: document.filename.clone(),
        mime_type: document.mime_type.clone(),
        size_bytes: u64::try_from(document.size_bytes).unwrap_or(0),
        is_pdf: document.is_pdf(),
        on_disk: document
            .local_path
            .as_deref()
            .is_some_and(|path| std::path::Path::new(path).exists()),
    }
}

fn to_data(record: &ArchiveRecord, ctx: &Context, full: bool, now: DateTime<Utc>) -> RecordData {
    let kind = RecordKind::parse(&record.kind).unwrap_or(RecordKind::Receipt);
    let empty_fields = Vec::new();
    let candidates = ctx.fields.get(&record.id).unwrap_or(&empty_fields);
    let won = winners(candidates);
    let fields: Vec<RecordFieldData> = won
        .iter()
        .filter_map(|value| field_data(kind, value))
        .collect();
    // Only money and dates decide whether a record is checked.
    let unchecked_fields = fields
        .iter()
        .filter(|field| {
            !field.checked && FieldName::parse(&field.field).is_some_and(FieldName::needs_checking)
        })
        .map(|field| field.field.clone())
        .collect();
    let empty_docs = Vec::new();
    let documents = ctx.documents.get(&record.id).unwrap_or(&empty_docs);
    let empty_sources = Vec::new();
    let sources = ctx.sources.get(&record.id).unwrap_or(&empty_sources);
    let group = record.group_id.as_ref().and_then(|id| {
        ctx.groups.get(id).map(|group| RecordGroupData {
            id: group.id.clone(),
            kind: group.kind.clone(),
            title: group.title.clone(),
            count: ctx.group_counts.get(id).copied().unwrap_or(0),
            span_start: group.span_start,
            span_end: group.span_end,
        })
    });
    RecordData {
        id: record.id.clone(),
        account_id: record.account_id.clone(),
        kind: kind_data(&record.kind),
        kind_label: kind.label().to_string(),
        issuer: record.issuer.clone(),
        title: record.title.clone(),
        reference: record.reference.clone(),
        reference_label: kind.reference_label().to_string(),
        amount: record
            .amount_minor
            .zip(record.currency.as_deref())
            .map(|(minor, currency)| amount_data(minor, currency)),
        date: record.ledger_date(),
        span_start: record.span_start,
        span_end: record.span_end,
        place: record.place.clone(),
        delivered_at: record.delivered_at,
        return_by: record.return_by,
        warranty_until: record.warranty_until,
        valid_until: record.valid_until,
        checked: record.checked,
        unchecked_fields,
        stage_line: stage_line(record, sources),
        detail_line: detail_line(record, now),
        pdf: documents.iter().find(|d| d.is_pdf()).map(document_data),
        document_count: u32::try_from(documents.len()).unwrap_or(u32::MAX),
        source_count: u32::try_from(sources.len()).unwrap_or(u32::MAX),
        group,
        why: why(record, now),
        origin: record.origin.clone(),
        thread_id: record.thread_id.clone(),
        message_id: sources.last().map(|source| source.message_id.clone()),
        dismissed: record.dismissed_at.is_some(),
        fields,
        documents: if full {
            documents.iter().map(document_data).collect()
        } else {
            Vec::new()
        },
        sources: if full {
            sources
                .iter()
                .map(|source| RecordSourceData {
                    message_id: source.message_id.clone(),
                    thread_id: source.thread_id.clone(),
                    stage: source.stage.clone(),
                    date: source.message_at,
                    subject: source.subject.clone(),
                    filed_by: source.filed_by.clone(),
                })
                .collect()
        } else {
            Vec::new()
        },
        issuer_records: None,
    }
}

/// One total per currency, largest first.
fn totals<'a>(records: impl IntoIterator<Item = &'a ArchiveRecord>) -> Vec<RecordAmountData> {
    let mut sums: BTreeMap<String, i64> = BTreeMap::new();
    for record in records {
        if let (Some(minor), Some(currency)) = (record.amount_minor, record.currency.as_ref()) {
            *sums.entry(currency.clone()).or_default() += minor;
        }
    }
    let mut out: Vec<RecordAmountData> = sums
        .into_iter()
        .map(|(currency, minor)| amount_data(minor, &currency))
        .collect();
    out.sort_by_key(|amount| std::cmp::Reverse(amount.minor));
    out
}

fn query_for(
    accounts: &[AccountId],
    filter: &RecordFilterData,
) -> Result<RecordQuery, HandlerError> {
    let year_bound = |year: i32| -> Result<DateTime<Utc>, HandlerError> {
        Local
            .with_ymd_and_hms(year, 1, 1, 0, 0, 0)
            .earliest()
            .map(|at| at.with_timezone(&Utc))
            .ok_or_else(|| {
                HandlerError::InvalidRequest(format!("{year} is not a year mxr can read"))
            })
    };
    let (from, until) = match filter.year {
        Some(year) => (Some(year_bound(year)?), Some(year_bound(year + 1)?)),
        None => (None, None),
    };
    Ok(RecordQuery {
        account_ids: Some(accounts.to_vec()),
        kinds: filter
            .kinds
            .iter()
            .map(|k| k.as_str().to_string())
            .collect(),
        issuer_key: filter
            .issuer
            .as_deref()
            .map(mxr_records::issuer_key)
            .filter(|key| !key.is_empty()),
        from,
        until,
        min_amount_minor: filter.min_amount_minor,
        max_amount_minor: filter.max_amount_minor,
        has_pdf: filter.has_pdf,
        checked: filter.checked,
        group_id: filter.group_id.clone(),
        moment_after: None,
        include_dismissed: false,
    })
}

fn facet(value: String, label: String, count: u32) -> RecordFacetCountData {
    RecordFacetCountData {
        value,
        label,
        count,
    }
}

fn months(records: &[ArchiveRecord]) -> Vec<RecordMonthData> {
    let mut by_month: BTreeMap<(i32, u32), Vec<&ArchiveRecord>> = BTreeMap::new();
    for record in records {
        if let Some(date) = record.ledger_date() {
            let local = date.with_timezone(&Local);
            by_month
                .entry((local.year(), local.month()))
                .or_default()
                .push(record);
        }
    }
    by_month
        .into_iter()
        .rev()
        .map(|((year, month), members)| RecordMonthData {
            month: format!("{year}-{month:02}"),
            label: format!("{year} · {}", month_name(month)),
            count: u32::try_from(members.len()).unwrap_or(u32::MAX),
            totals: totals(members),
        })
        .collect()
}

fn facets(all: &[ArchiveRecord], with_pdf: u32) -> RecordFacetsData {
    let mut kinds: BTreeMap<String, u32> = BTreeMap::new();
    let mut issuers: HashMap<String, (String, u32)> = HashMap::new();
    let mut years: BTreeMap<i32, u32> = BTreeMap::new();
    let mut checked = 0;
    for record in all {
        *kinds.entry(record.kind.clone()).or_default() += 1;
        if let (Some(key), Some(name)) = (&record.issuer_key, &record.issuer) {
            issuers
                .entry(key.clone())
                .or_insert_with(|| (name.clone(), 0))
                .1 += 1;
        }
        if let Some(date) = record.ledger_date() {
            *years.entry(date.with_timezone(&Local).year()).or_default() += 1;
        }
        checked += u32::from(record.checked);
    }
    let mut issuers: Vec<(String, String, u32)> = issuers
        .into_iter()
        .map(|(key, (name, count))| (key, name, count))
        .collect();
    issuers.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.1.cmp(&b.1)));
    issuers.truncate(ISSUER_FACETS);
    let total = u32::try_from(all.len()).unwrap_or(u32::MAX);
    RecordFacetsData {
        kinds: kinds
            .into_iter()
            .map(|(kind, count)| {
                let label = RecordKind::parse(&kind)
                    .map_or_else(|| kind.clone(), |k| k.label().to_string());
                facet(kind, label, count)
            })
            .collect(),
        issuers: issuers
            .into_iter()
            .map(|(key, name, count)| facet(key, name, count))
            .collect(),
        years: years
            .into_iter()
            .rev()
            .map(|(year, count)| facet(year.to_string(), year.to_string(), count))
            .collect(),
        has_pdf: with_pdf,
        checked,
        unchecked: total - checked,
    }
}

async fn first_run_data(
    state: &AppState,
    accounts: &[AccountId],
    found: u32,
) -> Result<Option<RecordFirstRunData>, HandlerError> {
    let mut complete = true;
    let mut scanned = 0u64;
    let mut reached: Option<DateTime<Utc>> = None;
    for account in accounts {
        match state.store.get_record_run(account).await? {
            Some(run) => {
                complete &= run.completed_at.is_some();
                scanned += u64::try_from(run.scanned).unwrap_or(0);
                if let Some((date, _)) = run.cursor {
                    reached = Some(reached.map_or(date, |r| r.max(date)));
                }
            }
            None => complete = false,
        }
    }
    if complete {
        return Ok(None);
    }
    let back_to = reached.map(|at| {
        let local = at.with_timezone(&Local);
        format!(", back to {} {}", month_name(local.month()), local.year())
    });
    Ok(Some(RecordFirstRunData {
        complete,
        scanned,
        reached,
        line: format!(
            "Filing your records. {found} found so far{}.",
            back_to.unwrap_or_default()
        ),
    }))
}

// ---------------------------------------------------------------------------
// Reads
// ---------------------------------------------------------------------------

pub(super) async fn list_records(
    state: &AppState,
    account_id: Option<&AccountId>,
    filter: &RecordFilterData,
    limit: u32,
    offset: u32,
) -> HandlerResult {
    let now = Utc::now();
    let accounts = scoped_accounts(state, account_id).await?;
    let all = state
        .store
        .list_archive_records(&query_for(&accounts, &RecordFilterData::default())?)
        .await?;
    let matching = state
        .store
        .list_archive_records(&query_for(&accounts, filter)?)
        .await?;
    let with_pdf = state
        .store
        .count_archive_records_with_pdf(&accounts)
        .await?;
    let start = usize::try_from(offset)
        .unwrap_or(usize::MAX)
        .min(matching.len());
    let end = start
        .saturating_add(usize::try_from(limit.max(1)).unwrap_or(usize::MAX))
        .min(matching.len());
    let page = &matching[start..end];
    let ctx = Context::load(state, page, Some(&accounts)).await?;
    let mut coming_up = super::record_subscriptions::signal_moments(state, &accounts, now).await?;
    coming_up.extend(
        mxr_records::coming_up::moments(&all, &ctx.groups, now, &Local)
            .into_iter()
            .map(moment_data),
    );
    let issuer = filter.issuer.as_ref().and_then(|_| {
        let first = matching.first()?;
        Some(RecordIssuerData {
            name: first.issuer.clone().unwrap_or_default(),
            count: u32::try_from(matching.len()).unwrap_or(u32::MAX),
            first: matching.iter().filter_map(ArchiveRecord::ledger_date).min(),
            last: matching.iter().filter_map(ArchiveRecord::ledger_date).max(),
            totals: totals(&matching),
        })
    });
    let total = u32::try_from(all.len()).unwrap_or(u32::MAX);
    let empty_state = if all.is_empty() {
        Some(format!(
            "{} {}",
            archive_copy::NEVER_HAD_ANY,
            archive_copy::ADD_ONE_KEYS
        ))
    } else if matching.is_empty() {
        Some("No records match these filters.".to_string())
    } else {
        None
    };
    Ok(ResponseData::RecordLedger {
        ledger: RecordLedgerData {
            header: archive_copy::HEADER.to_string(),
            total,
            matching: u32::try_from(matching.len()).unwrap_or(u32::MAX),
            records: page.iter().map(|r| to_data(r, &ctx, false, now)).collect(),
            months: months(&matching),
            facets: facets(&all, with_pdf),
            coming_up,
            filter: filter.clone(),
            issuer,
            empty_state,
            first_run: first_run_data(state, &accounts, total).await?,
        },
    })
}

async fn record_data(
    state: &AppState,
    record: &ArchiveRecord,
    full: bool,
) -> Result<RecordData, HandlerError> {
    let ctx = Context::load(
        state,
        std::slice::from_ref(record),
        Some(std::slice::from_ref(&record.account_id)),
    )
    .await?;
    let mut data = to_data(record, &ctx, full, Utc::now());
    if full {
        if let Some(key) = &record.issuer_key {
            let others = state
                .store
                .list_archive_records(&RecordQuery {
                    account_ids: Some(vec![record.account_id.clone()]),
                    issuer_key: Some(key.clone()),
                    ..RecordQuery::default()
                })
                .await?
                .len();
            data.issuer_records = Some(u32::try_from(others.saturating_sub(1)).unwrap_or(0));
        }
    }
    Ok(data)
}

pub(super) async fn get_record(state: &AppState, raw_id: &str) -> HandlerResult {
    let id = resolve_record_id(state, raw_id).await?;
    let record = state
        .store
        .get_archive_record(&id)
        .await?
        .ok_or_else(|| HandlerError::InvalidRequest(format!("No record matches {raw_id}.")))?;
    Ok(ResponseData::Record {
        record: record_data(state, &record, true).await?,
    })
}

/// What the answer box was asked to return besides the query.
#[derive(Debug, Clone, Copy)]
pub(super) struct AnswerAsk {
    /// Fall back to `mxr ask` over all mail when no record matches.
    pub fallback: bool,
    /// Records in "also matching".
    pub limit: u32,
    /// Every match as a list, whatever the query asks for.
    pub list: bool,
    pub offset: u32,
    pub list_limit: u32,
}

/// Every match of a list-mode query, newest first like the ledger, with
/// this page's rows.
fn answer_list(
    query: &str,
    matched: Vec<ArchiveRecord>,
    top_record_id: String,
    ask: AnswerAsk,
) -> (RecordAnswerListData, Vec<ArchiveRecord>) {
    let mut members = matched;
    members.sort_by(|a, b| {
        b.ledger_date()
            .cmp(&a.ledger_date())
            .then_with(|| b.id.cmp(&a.id))
    });
    let issuer = members
        .first()
        .and_then(|first| first.issuer_key.as_ref().zip(first.issuer.as_ref()))
        .filter(|(key, _)| {
            members
                .iter()
                .all(|record| record.issuer_key.as_ref() == Some(*key))
        })
        .map(|(_, name)| name.clone());
    let totals = totals(&members);
    let mut header = vec![
        issuer.clone().unwrap_or_else(|| format!("\"{query}\"")),
        plural(members.len(), "record"),
    ];
    if !totals.is_empty() {
        header.push(
            totals
                .iter()
                .map(|total| total.display.as_str())
                .collect::<Vec<_>>()
                .join(" + "),
        );
    }
    let start = usize::try_from(ask.offset)
        .unwrap_or(usize::MAX)
        .min(members.len());
    let end = start
        .saturating_add(usize::try_from(ask.list_limit.max(1)).unwrap_or(usize::MAX))
        .min(members.len());
    let page = members[start..end].to_vec();
    let list = RecordAnswerListData {
        header: header.join(" \u{b7} "),
        count: u32::try_from(members.len()).unwrap_or(u32::MAX),
        offset: u32::try_from(start).unwrap_or(u32::MAX),
        records: Vec::new(),
        months: months(&members),
        totals,
        first: members.iter().filter_map(ArchiveRecord::ledger_date).min(),
        last: members.iter().filter_map(ArchiveRecord::ledger_date).max(),
        top_record_id,
        issuer,
    };
    (list, page)
}

pub(super) async fn answer_query(
    state: &AppState,
    query: &str,
    account_id: Option<&AccountId>,
    ask: AnswerAsk,
) -> HandlerResult {
    let text = query.trim();
    if text.is_empty() {
        return Err(HandlerError::InvalidRequest(
            "Ask for something, like \"lisbon booking ref\".".to_string(),
        ));
    }
    let now = Utc::now();
    let accounts = scoped_accounts(state, account_id).await?;
    let records = state
        .store
        .list_archive_records(&query_for(&accounts, &RecordFilterData::default())?)
        .await?;
    let groups: HashMap<String, RecordGroup> = state
        .store
        .list_record_groups(Some(&accounts))
        .await?
        .into_iter()
        .map(|g| (g.id.clone(), g))
        .collect();
    let parsed = answer::parse(text);
    let candidates: Vec<Candidate<'_>> = records
        .iter()
        .map(|record| Candidate {
            record,
            group_title: record
                .group_id
                .as_ref()
                .and_then(|id| groups.get(id))
                .map(|g| g.title.as_str()),
        })
        .collect();
    let ranked = answer::rank(&parsed, &candidates);
    let asked = match parsed.asked {
        answer::Asked::Reference => "reference",
        answer::Asked::Amount => "amount",
        answer::Asked::Date => "date",
        answer::Asked::Document => "document",
        answer::Asked::Any => "any",
    };
    let take = usize::try_from(ask.limit).unwrap_or(4) + 1;
    let top: Vec<&ArchiveRecord> = ranked
        .iter()
        .take(take)
        .map(|ranked| &records[ranked.index])
        .collect();
    if let Some(best) = top.first() {
        let mode = if ask.list {
            answer::Mode::List
        } else {
            answer::mode(&parsed, &ranked)
        };
        let (list, page) = match mode {
            answer::Mode::Answer => (None, Vec::new()),
            answer::Mode::List => {
                let matched = ranked
                    .iter()
                    .map(|ranked| records[ranked.index].clone())
                    .collect();
                let (list, page) = answer_list(text, matched, best.id.clone(), ask);
                (Some(list), page)
            }
        };
        // One load covers the card, "also matching" and the list's page.
        let mut owned: Vec<ArchiveRecord> = top.iter().map(|r| (*r).clone()).collect();
        for record in &page {
            if !owned.iter().any(|known| known.id == record.id) {
                owned.push(record.clone());
            }
        }
        let ctx = Context::load(state, &owned, Some(&accounts)).await?;
        let list = list.map(|list| {
            Box::new(RecordAnswerListData {
                records: page.iter().map(|r| to_data(r, &ctx, false, now)).collect(),
                ..list
            })
        });
        let card = to_data(best, &ctx, false, now);
        let field = answer::answer_field(parsed.asked, best);
        let (label, value, copy, provenance) = match field {
            AnswerField::Document => match &card.pdf {
                Some(pdf) => (
                    "Document".to_string(),
                    pdf.filename.clone(),
                    pdf.filename.clone(),
                    None,
                ),
                None => (
                    "Document".to_string(),
                    "No PDF on this record".to_string(),
                    String::new(),
                    None,
                ),
            },
            other => {
                let provenance = card
                    .fields
                    .iter()
                    .find(|f| f.field == other.as_str())
                    .cloned();
                match &provenance {
                    Some(found) => (
                        found.label.clone(),
                        found.value.clone(),
                        found.copy.clone(),
                        provenance.clone(),
                    ),
                    None => (
                        "Date".to_string(),
                        card.date.map(date_label).unwrap_or_default(),
                        card.date.map(date_label).unwrap_or_default(),
                        None,
                    ),
                }
            }
        };
        return Ok(ResponseData::RecordAnswer {
            answer: RecordAnswerData {
                query: text.to_string(),
                asked: asked.to_string(),
                mode: match mode {
                    answer::Mode::Answer => RecordAnswerModeData::Answer,
                    answer::Mode::List => RecordAnswerModeData::List,
                },
                answer: Some(RecordAnswerCardData {
                    record: card,
                    field: field.as_str().to_string(),
                    label,
                    value,
                    copy,
                    provenance,
                }),
                also: top
                    .iter()
                    .skip(1)
                    .map(|record| to_data(record, &ctx, false, now))
                    .collect(),
                matching: u32::try_from(ranked.len()).unwrap_or(u32::MAX),
                list,
                fallback: None,
            },
        });
    }
    let fallback = if ask.fallback {
        let filters = ArchiveAskFiltersData {
            account_id: account_id.cloned(),
            ..ArchiveAskFiltersData::default()
        };
        let (answer, error) = match super::archive_ask::ask(state, text, &filters, 6).await {
            Ok(ResponseData::ArchiveAnswer { answer }) => (Some(answer), None),
            Ok(_) => (None, Some("unexpected answer from mxr ask".to_string())),
            Err(error) => (None, Some(error.to_string())),
        };
        Some(RecordFallbackData {
            note: archive_copy::no_match(text),
            answer,
            error,
        })
    } else {
        Some(RecordFallbackData {
            note: format!("No record matches \"{text}\"."),
            answer: None,
            error: None,
        })
    };
    Ok(ResponseData::RecordAnswer {
        answer: RecordAnswerData {
            query: text.to_string(),
            asked: asked.to_string(),
            mode: RecordAnswerModeData::Answer,
            answer: None,
            also: Vec::new(),
            matching: 0,
            list: None,
            fallback,
        },
    })
}

// ---------------------------------------------------------------------------
// Mutations
// ---------------------------------------------------------------------------

/// A value the user typed, as the store keeps it for `field`.
fn parse_value(
    state: &AppState,
    field: FieldName,
    raw: &str,
    record: &ArchiveRecord,
) -> Result<(Option<String>, Option<i64>), HandlerError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(HandlerError::InvalidRequest(format!(
            "Give a value for {}.",
            field.label().to_lowercase()
        )));
    }
    match field {
        FieldName::Kind => RecordKind::parse(raw)
            .map(|kind| (Some(kind.as_str().to_string()), None))
            .ok_or_else(|| {
                HandlerError::InvalidRequest(format!(
                    "{raw} is not a kind of record; use receipt, order, booking, invoice, statement, ticket, contract, warranty or account."
                ))
            }),
        FieldName::Amount => {
            if let Some((_, amount)) = mxr_todo::money::find_amounts(raw).into_iter().next() {
                return Ok((Some(amount.currency), Some(amount.minor)));
            }
            let currency = record.currency.clone().ok_or_else(|| {
                HandlerError::InvalidRequest(format!(
                    "Give the currency too, like \"£{raw}\" or \"{raw} EUR\"."
                ))
            })?;
            let cleaned = raw.replace(',', "");
            let (whole, frac) = cleaned.split_once('.').unwrap_or((&cleaned, ""));
            let whole: i64 = whole.parse().map_err(|_| {
                HandlerError::InvalidRequest(format!("{raw} is not an amount."))
            })?;
            let frac: i64 = match frac.len() {
                0 => 0,
                1 => frac.parse::<i64>().unwrap_or(0) * 10,
                _ => frac.get(..2).and_then(|f| f.parse().ok()).unwrap_or(0),
            };
            Ok((Some(currency), Some(whole * 100 + frac)))
        }
        _ if field.is_date() => {
            let today = Local::now().date_naive();
            let day = NaiveDate::parse_from_str(raw, "%Y-%m-%d")
                .ok()
                .or_else(|| {
                    // A date with a year reads as written; without one, a
                    // past field (issued) means the last such day and a
                    // future one the next.
                    let past = mxr_records::rules::past_day(raw, today);
                    let has_year = raw.split_whitespace().any(|w| w.len() == 4 && w.chars().all(|c| c.is_ascii_digit()));
                    if has_year || field == FieldName::IssuedAt || field == FieldName::DeliveredAt {
                        past.or_else(|| mxr_records::rules::past_day(raw, today + chrono::Duration::days(3660)))
                    } else {
                        None
                    }
                });
            let at = match day {
                Some(day) => mxr_records::fields::day_at(day),
                None => match super::time::resolve_in_zone(state, raw, None, None)? {
                    Ok(resolution) => resolution.at,
                    Err(error) => return Err(HandlerError::InvalidRequest(error.message)),
                },
            };
            Ok((None, Some(at.timestamp())))
        }
        _ => Ok((Some(raw.to_string()), None)),
    }
}

fn change(
    dry_run: bool,
    action: &str,
    records: Vec<RecordData>,
    message: String,
    undo: Option<RecordUndoData>,
) -> ResponseData {
    ResponseData::RecordChange {
        change: RecordChangeData {
            dry_run,
            action: action.to_string(),
            records,
            message,
            undo,
        },
    }
}

/// The record built from a preview's rows, as a card.
async fn preview_data(
    state: &AppState,
    record: &ArchiveRecord,
    fields: Vec<RecordFieldValue>,
) -> Result<RecordData, HandlerError> {
    let mut ctx = Context::load(
        state,
        std::slice::from_ref(record),
        Some(std::slice::from_ref(&record.account_id)),
    )
    .await?;
    ctx.fields.insert(record.id.clone(), fields);
    Ok(to_data(record, &ctx, true, Utc::now()))
}

pub(super) async fn set_field(
    state: &AppState,
    raw_id: &str,
    edit: &RecordEditData,
    apply_to_sender: bool,
    dry_run: bool,
) -> HandlerResult {
    let now = Utc::now();
    let id = resolve_record_id(state, raw_id).await?;
    let record = state
        .store
        .get_archive_record(&id)
        .await?
        .ok_or_else(|| HandlerError::InvalidRequest(format!("No record matches {raw_id}.")))?;
    let field_of = |name: &str| {
        FieldName::parse(name).ok_or_else(|| {
            HandlerError::InvalidRequest(format!(
                "{name} is not a record field; use issuer, title, reference, amount, date, kind, return_by, warranty_until or valid_until."
            ))
        })
    };
    let (store_edit, message, undo_fields) = match edit {
        RecordEditData::Set { field, value } => {
            let name = field_of(field)?;
            let key = name.as_str().to_string();
            let (value_text, value_int) = parse_value(state, name, value, &record)?;
            (
                RecordFieldEdit::Set {
                    field: key.clone(),
                    value_text,
                    value_int,
                },
                format!(
                    "Fixed {}. It stays as you set it.",
                    name.label().to_lowercase()
                ),
                vec![key],
            )
        }
        RecordEditData::Confirm { field } => {
            let name = field_of(field)?;
            let key = name.as_str().to_string();
            (
                RecordFieldEdit::Confirm { field: key.clone() },
                format!("Confirmed {}.", name.label().to_lowercase()),
                vec![key],
            )
        }
        RecordEditData::ConfirmAll => (
            RecordFieldEdit::ConfirmUnchecked,
            "Marked checked.".to_string(),
            mxr_store::RECORD_CHECKED_FIELDS
                .iter()
                .map(|f| (*f).to_string())
                .collect(),
        ),
        RecordEditData::Clear { field } => {
            let name = field_of(field)?;
            (
                RecordFieldEdit::Clear {
                    field: name.as_str().to_string(),
                },
                format!(
                    "Back to the {} read from the email.",
                    name.label().to_lowercase()
                ),
                Vec::new(),
            )
        }
    };
    // A sender-wide edit is checked before anything is written, then
    // applied to every record of the sender in one transaction.
    let sender = if apply_to_sender {
        let issuer = match edit {
            RecordEditData::Set { field, value } if field_of(field)? == FieldName::Issuer => {
                value.trim().to_string()
            }
            _ => {
                return Err(HandlerError::InvalidRequest(
                    "Only a new issuer name applies to a sender.".to_string(),
                ))
            }
        };
        Some(mxr_store::SenderIssuer {
            account_id: record.account_id.clone(),
            sender_email: issuer_sender(state, &record).await?,
            issuer_name: issuer,
        })
    } else {
        None
    };
    let mut ids = vec![id.clone()];
    if let Some(sender) = &sender {
        ids.extend(
            state
                .store
                .archive_record_ids_from_sender(&record.account_id, &sender.sender_email)
                .await?
                .into_iter()
                .filter(|other| *other != id),
        );
    }
    let edited = state
        .store
        .edit_archive_records(&ids, &store_edit, now, sender.as_ref(), !dry_run)
        .await?;
    if edited.is_empty() {
        return Err(HandlerError::InvalidRequest(format!(
            "No record matches {raw_id}."
        )));
    }
    let mut records = Vec::with_capacity(edited.len());
    for (after, fields) in edited {
        records.push(preview_data(state, &after, fields).await?);
    }
    let mut message = message;
    if let Some(sender) = &sender {
        if !dry_run {
            pass::regroup(&state.store, &record.account_id, now)
                .await
                .map_err(|error| HandlerError::Message(error.to_string()))?;
        }
        message = format!(
            "{message} Applied to {} from {}, now and later.",
            plural(records.len(), "record"),
            sender.sender_email
        );
    }
    let undo = (!dry_run && !undo_fields.is_empty()).then(|| RecordUndoData {
        kind: "clear_fields".to_string(),
        record_ids: records.iter().map(|r| r.id.clone()).collect(),
        fields: undo_fields,
    });
    if dry_run {
        message = format!("Would do: {message}");
    }
    Ok(change(dry_run, "set_field", records, message, undo))
}

fn plural(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

/// The address of the first email a record came from: the issuer's own
/// (a carrier's delivery email comes later).
async fn issuer_sender(state: &AppState, record: &ArchiveRecord) -> Result<String, HandlerError> {
    let sources = state
        .store
        .archive_record_sources(std::slice::from_ref(&record.id))
        .await?;
    let first = sources
        .first()
        .ok_or_else(|| HandlerError::Message("This record has no email left.".to_string()))?;
    let envelope = state
        .store
        .get_envelope(&first.message_id)
        .await?
        .ok_or_else(|| HandlerError::Message("This record's email is gone.".to_string()))?;
    Ok(envelope.from.email.to_lowercase())
}

pub(super) async fn dismiss(
    state: &AppState,
    raw_ids: &[String],
    restore: bool,
    dry_run: bool,
) -> HandlerResult {
    let now = Utc::now();
    let ids = resolve_record_ids(state, raw_ids).await?;
    let selected: Vec<ArchiveRecord> = state
        .store
        .get_archive_records(&ids)
        .await?
        .into_iter()
        .filter(|record| record.dismissed_at.is_some() == restore)
        .collect();
    let selected_ids: Vec<String> = selected.iter().map(|r| r.id.clone()).collect();
    let changed = if dry_run {
        selected
            .into_iter()
            .map(|mut record| {
                record.dismissed_at = (!restore).then_some(now);
                record
            })
            .collect::<Vec<_>>()
    } else {
        let changed_ids = state
            .store
            .set_archive_records_dismissed(&selected_ids, !restore, now)
            .await?;
        state.store.get_archive_records(&changed_ids).await?
    };
    let ctx = Context::load(state, &changed, None).await?;
    let records: Vec<RecordData> = changed
        .iter()
        .map(|r| to_data(r, &ctx, false, now))
        .collect();
    let message = match (restore, dry_run, records.len()) {
        (_, _, 0) => "Nothing to change.".to_string(),
        (false, false, 1) => archive_copy::NOT_A_RECORD.to_string(),
        (false, false, n) => format!("{n} marked not a record. The emails are untouched."),
        (false, true, n) => format!(
            "Would mark {} not a record. The emails stay as they are.",
            plural(n, "record")
        ),
        (true, false, n) => format!("{} back in Archive.", plural(n, "record")),
        (true, true, n) => format!("Would bring back {}.", plural(n, "record")),
    };
    let undo = (!dry_run && !records.is_empty()).then(|| RecordUndoData {
        kind: if restore { "dismiss" } else { "restore" }.to_string(),
        record_ids: records.iter().map(|r| r.id.clone()).collect(),
        fields: Vec::new(),
    });
    Ok(change(
        dry_run,
        if restore { "restore" } else { "dismiss" },
        records,
        message,
        undo,
    ))
}

/// How a filing says the email was already in Archive; Move reads it to
/// know nothing was made.
pub(super) const ALREADY_FILED: &str = "Already in Archive";

pub(super) async fn file(
    state: &AppState,
    message_id: &MessageId,
    kind: Option<RecordKindData>,
    dry_run: bool,
) -> HandlerResult {
    let now = Utc::now();
    let cfg = pass_config(now);
    let kind = kind.and_then(|kind| RecordKind::parse(kind.as_str()));
    let filing = pass::plan_manual(&state.store, &cfg, message_id, kind)
        .await
        .map_err(|error| HandlerError::InvalidRequest(error.to_string()))?;
    // The dry run and the filing run the same restore-then-file write; the
    // dry run's transaction is rolled back.
    let (filed, restored, after) = state.store.file_record_by_hand(&filing, !dry_run).await?;
    let (record, fields) =
        after.ok_or_else(|| HandlerError::Message("The filed record vanished.".to_string()))?;
    if !dry_run {
        pass::regroup(&state.store, &filing.account_id, now)
            .await
            .map_err(|error| HandlerError::Message(error.to_string()))?;
    }
    let data = if dry_run {
        preview_data(state, &record, fields).await?
    } else {
        record_data(state, &record, true).await?
    };
    let new_here = restored || matches!(filed, RecordFiled::Inserted { .. });
    let message = match (dry_run, restored, new_here) {
        (true, true, _) => format!(
            "You marked this not a record; filing it brings it back: {}.",
            describe(&data)
        ),
        (true, false, true) => format!("Would file in Archive: {}.", describe(&data)),
        (false, _, true) => archive_copy::FILED.to_string(),
        (_, _, false) => format!("{ALREADY_FILED}: {}.", describe(&data)),
    };
    let undo = (!dry_run && new_here).then(|| RecordUndoData {
        kind: "dismiss".to_string(),
        record_ids: vec![record.id.clone()],
        fields: Vec::new(),
    });
    Ok(change(dry_run, "file", vec![data], message, undo))
}

/// "Dell, XPS 14 laptop".
fn describe(record: &RecordData) -> String {
    match (&record.issuer, &record.title) {
        (Some(issuer), Some(title)) => format!("{issuer}, {title}"),
        (Some(one), None) | (None, Some(one)) => one.clone(),
        (None, None) => record.kind_label.to_lowercase(),
    }
}

pub(super) async fn set_sender(
    state: &AppState,
    message_id: &MessageId,
    verdict: Option<&str>,
    kind: Option<RecordKindData>,
    dry_run: bool,
) -> HandlerResult {
    let now = Utc::now();
    let verdict = match verdict.map(str::trim) {
        Some("always") => Some("always"),
        Some("never") => Some("never"),
        None | Some("" | "clear") => None,
        Some(other) => {
            return Err(HandlerError::InvalidRequest(format!(
                "{other} is not a choice; use always or never."
            )))
        }
    };
    let envelope = state
        .store
        .get_envelope(message_id)
        .await?
        .ok_or_else(|| HandlerError::InvalidRequest(format!("No message {message_id}.")))?;
    let sender = envelope.from.email.to_lowercase();
    let account = envelope.account_id.clone();
    let from_sender = state
        .store
        .message_ids_from_sender(&account, &sender, SENDER_BACKFILL)
        .await?;
    let filed_ids = state
        .store
        .archive_record_ids_from_sender(&account, &sender)
        .await?;
    let message = match (verdict, dry_run) {
        (Some("always"), true) => format!(
            "Would file mail from {sender} in Archive from now on, starting with {} already here.",
            plural(from_sender.len(), "email")
        ),
        (Some("always"), false) => format!("Filing mail from {sender} in Archive from now on."),
        (Some(_), true) => format!(
            "Would stop filing mail from {sender} and take {} out of Archive. The emails stay as they are.",
            plural(filed_ids.len(), "record")
        ),
        (Some(_), false) => format!(
            "Mail from {sender} is never filed. {} out of Archive; the emails are untouched.",
            plural(filed_ids.len(), "record")
        ),
        (None, true) => format!("Would let the rules decide for {sender} again."),
        (None, false) => format!("The rules decide for {sender} again."),
    };
    if dry_run {
        return Ok(change(true, "sender", Vec::new(), message, None));
    }
    state
        .store
        .set_record_sender_verdict(
            &account,
            &sender,
            verdict,
            kind.map(RecordKindData::as_str),
            now,
        )
        .await?;
    match verdict {
        Some("always") => {
            pass::scan_messages(&state.store, &pass_config(now), &from_sender)
                .await
                .map_err(|error| HandlerError::Message(error.to_string()))?;
        }
        Some(_) => {
            state
                .store
                .set_archive_records_dismissed(&filed_ids, true, now)
                .await?;
        }
        None => {}
    }
    let after = state
        .store
        .get_archive_records(
            &state
                .store
                .archive_record_ids_from_sender(&account, &sender)
                .await?,
        )
        .await?;
    let ctx = Context::load(state, &after, Some(std::slice::from_ref(&account))).await?;
    Ok(change(
        false,
        "sender",
        after.iter().map(|r| to_data(r, &ctx, false, now)).collect(),
        message,
        Some(RecordUndoData {
            kind: "sender".to_string(),
            record_ids: Vec::new(),
            fields: Vec::new(),
        }),
    ))
}

pub(super) async fn export_records(
    state: &AppState,
    account_id: Option<&AccountId>,
    filter: &RecordFilterData,
    attachments_dir: Option<&str>,
    dry_run: bool,
) -> HandlerResult {
    let accounts = scoped_accounts(state, account_id).await?;
    let records = state
        .store
        .list_archive_records(&query_for(&accounts, filter)?)
        .await?;
    let ctx = Context::load(state, &records, Some(&accounts)).await?;
    let now = Utc::now();
    let rows: Vec<ExportRow> = records
        .iter()
        .map(|record| {
            let data = to_data(record, &ctx, false, now);
            ExportRow {
                record_id: record.id.clone(),
                date: record.ledger_date(),
                kind: record.kind.clone(),
                issuer: record.issuer.clone(),
                title: record.title.clone(),
                amount_minor: record.amount_minor,
                currency: record.currency.clone(),
                reference: record.reference.clone(),
                checked: record.checked,
                unchecked_fields: data.unchecked_fields.clone(),
                pdf: data.pdf.as_ref().map(|pdf| pdf.filename.clone()),
                source_message_id: data.message_id.as_ref().map(MessageId::as_str),
            }
        })
        .collect();
    let preview = export::preview(&rows);
    let totals: Vec<RecordAmountData> = {
        let mut out: Vec<RecordAmountData> = preview
            .totals
            .iter()
            .map(|(currency, minor)| amount_data(*minor, currency))
            .collect();
        out.sort_by_key(|amount| std::cmp::Reverse(amount.minor));
        out
    };
    let summary = {
        let amounts = totals
            .iter()
            .map(|total| total.display.clone())
            .collect::<Vec<_>>();
        let amounts = match amounts.as_slice() {
            [] => String::new(),
            [one] => format!(", {one}"),
            [rest @ .., last] => format!(", {} and {last}", rest.join(", ")),
        };
        format!(
            "{}{amounts}. {} an unchecked amount or date; {} no PDF.",
            plural(rows.len(), "record"),
            match preview.unchecked {
                1 => "1 has".to_string(),
                n => format!("{n} have"),
            },
            match preview.missing_pdfs {
                1 => "1 has".to_string(),
                n => format!("{n} have"),
            }
        )
    };
    let mut data = RecordExportData {
        dry_run,
        rows: preview.rows,
        totals,
        unchecked: preview.unchecked,
        missing_pdfs: preview.missing_pdfs,
        by_kind: preview
            .by_kind
            .iter()
            .map(|(kind, count)| {
                let label =
                    RecordKind::parse(kind).map_or_else(|| kind.clone(), |k| k.label().to_string());
                facet(kind.clone(), label, *count)
            })
            .collect(),
        summary,
        csv: None,
        attachments_dir: attachments_dir.map(str::to_string),
        pdfs_copied: 0,
        pdf_errors: Vec::new(),
    };
    if dry_run {
        return Ok(ResponseData::RecordExport { export: data });
    }
    data.csv = Some(
        export::to_csv(&rows, &Local).map_err(|error| HandlerError::Message(error.to_string()))?,
    );
    if let Some(dir) = attachments_dir {
        let dir = std::path::PathBuf::from(dir);
        if !dir.is_absolute() {
            return Err(HandlerError::InvalidRequest(
                "The folder for PDFs must be an absolute path.".to_string(),
            ));
        }
        tokio::fs::create_dir_all(&dir).await.map_err(|error| {
            HandlerError::Message(format!("Can't create {}: {error}", dir.display()))
        })?;
        for record in &records {
            let Some(pdf) = ctx
                .documents
                .get(&record.id)
                .and_then(|docs| docs.iter().find(|d| d.is_pdf()))
            else {
                continue;
            };
            match copy_pdf(state, record, pdf, &dir).await {
                Ok(()) => data.pdfs_copied += 1,
                Err(error) => data.pdf_errors.push(format!("{}: {error}", pdf.filename)),
            }
        }
    }
    Ok(ResponseData::RecordExport { export: data })
}

/// Copies a record's PDF into `dir` as "2025-03-03 Dell invoice.pdf",
/// downloading it first when it isn't on disk.
async fn copy_pdf(
    state: &AppState,
    record: &ArchiveRecord,
    pdf: &RecordDocument,
    dir: &std::path::Path,
) -> Result<(), String> {
    let attachment_id: AttachmentId = pdf
        .attachment_id
        .parse()
        .map_err(|_| "bad attachment id".to_string())?;
    let file = super::materialize_attachment_file(state, &pdf.message_id, &attachment_id)
        .await
        .map_err(|error| error.to_string())?;
    let date = record
        .ledger_date()
        .map(|at| at.with_timezone(&Local).format("%Y-%m-%d").to_string())
        .unwrap_or_default();
    let issuer = record.issuer.clone().unwrap_or_default();
    let safe = |text: &str| -> String {
        text.chars()
            .map(|c| {
                if c.is_alphanumeric() || " .-_".contains(c) {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>()
            .trim()
            .to_string()
    };
    let name = format!("{date} {} {}", safe(&issuer), safe(&pdf.filename));
    let mut target = dir.join(name.trim());
    let mut n = 2;
    while tokio::fs::try_exists(&target).await.unwrap_or(false) {
        target = dir.join(format!(
            "{} ({n}).pdf",
            name.trim().trim_end_matches(".pdf")
        ));
        n += 1;
    }
    tokio::fs::copy(&file.path, &target)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn the_chip_says_unchecked_for_a_rule_read_reference() {
        let value = RecordFieldValue {
            field: "reference".to_string(),
            source_key: "m".to_string(),
            message_id: None,
            source: "rule".to_string(),
            rank: 1,
            value_text: Some("402-118".to_string()),
            value_int: None,
            checked: false,
            evidence: Some("Order number: 402-118".to_string()),
            observed_at: Utc::now(),
        };
        let chip = field_data(RecordKind::Order, &value).expect("a field");
        assert!(!chip.checked, "a rule's reference is not checked");
    }

    #[test]
    fn days_print_as_days_and_instants_with_their_time() {
        let day = mxr_records::fields::day_at(NaiveDate::from_ymd_opt(2025, 3, 3).expect("day"));
        assert_eq!(date_label(day), "3 Mar 2025");
        let instant = Utc
            .with_ymd_and_hms(2025, 6, 12, 6, 40, 0)
            .single()
            .expect("time");
        assert!(date_label(instant).contains("2025"));
        assert_eq!(mxr_todo::money::plain_amount(124_900), "1249.00");
    }
}
