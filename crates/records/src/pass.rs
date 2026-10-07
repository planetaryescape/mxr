//! Running detection over mail and filing what it finds.
//!
//! * [`scan_messages`] files records from newly synced mail.
//! * [`run_first_run`] reads an account's history newest first, resumably.
//!   Records never expire, so it runs over everything, rules only.
//! * [`plan_manual`] reads one message filed by hand; the dry run and the
//!   real filing use the same plan.
//! * [`file_from_todo`] files the record a ticked-off to-do leaves behind.
//! * [`file_delivered`] files the order a delivered parcel completes.
//! * [`regroup`] rebuilds trips and series.
//!
//! A record's dedup key is its kind, issuer and reference, and a later
//! email with the same reference (a shipping notice, a carrier's delivery
//! email) joins the record already filed under it.

use crate::detect::{detect, manual, DetectInput, Detection, Origin, SenderView};
use crate::fields::{day_at, FieldName, Found};
use crate::group::{self, GroupInput};
use crate::{issuer_key, reference_key, RecordKind, Source, Stage, RULES_VERSION};
use chrono::{DateTime, TimeZone, Utc};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::types::MessageFlags;
use mxr_reader::{clean, ReaderConfig};
use mxr_store::{
    ArchiveRecord, DeliveryListFilter, RecordFiled, RecordFiling, RecordGroup, RecordLink,
    RecordSenderRule, Store, TodoRecord, TodoScanRow,
};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone)]
pub struct PassConfig<Tz> {
    pub now: DateTime<Utc>,
    /// The user's zone: an email's date is its day here.
    pub tz: Tz,
}

/// Counts only: no titles, amounts or references, so a summary is safe to
/// log and to record as activity.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PassSummary {
    pub scanned: u64,
    pub bodies_read: u64,
    pub filed: u64,
    pub updated: u64,
    /// Found again after the user said "not a record": left alone.
    pub kept_dismissed: u64,
    pub by_kind: BTreeMap<String, u64>,
}

impl PassSummary {
    fn add(&mut self, other: &Self) {
        self.scanned += other.scanned;
        self.bodies_read += other.bodies_read;
        self.filed += other.filed;
        self.updated += other.updated;
        self.kept_dismissed += other.kept_dismissed;
        for (kind, count) in &other.by_kind {
            *self.by_kind.entry(kind.clone()).or_default() += count;
        }
    }

    fn count(&mut self, kind: &str, filed: &RecordFiled) {
        match filed {
            RecordFiled::Inserted { .. } => {
                self.filed += 1;
                *self.by_kind.entry(kind.to_string()).or_default() += 1;
            }
            RecordFiled::Updated { .. } => self.updated += 1,
            RecordFiled::Unchanged { .. } => {}
            RecordFiled::Dismissed { .. } => self.kept_dismissed += 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FirstRunProgress {
    pub complete: bool,
    pub scanned: i64,
    pub reached: Option<DateTime<Utc>>,
    pub summary: PassSummary,
}

/// Words in a subject or snippet that earn a read of the body. Cheap on
/// headers, so the first run over a whole mailbox reads few bodies.
static WORTH_READING: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(order|receipt|invoice|statement|\bbill\b|payment|paid|booking|reservation|itinerary|ticket|e-ticket|boarding|confirm|shipped|dispatched|delivered|refund|return|warranty|contract|agreement|signed|lease|tenancy|policy|account (?:created|number|details)|purchase)")
        .expect("valid worth-reading regex")
});

/// Per-sender decisions for one account, keyed by lowercase address.
#[derive(Debug, Clone, Default)]
pub struct Senders(HashMap<String, RecordSenderRule>);

impl Senders {
    pub async fn load(store: &Store, account_id: &AccountId) -> anyhow::Result<Self> {
        Ok(Self(
            store
                .record_sender_rules(account_id)
                .await?
                .into_iter()
                .map(|rule| (rule.sender_email.to_lowercase(), rule))
                .collect(),
        ))
    }

    fn rule(&self, email: &str) -> Option<&RecordSenderRule> {
        self.0.get(&email.to_lowercase())
    }

    pub fn never(&self, email: &str) -> bool {
        self.rule(email)
            .is_some_and(|rule| rule.verdict.as_deref() == Some("never"))
    }

    pub fn view(&self, email: &str) -> SenderView<'_> {
        let rule = self.rule(email);
        SenderView {
            always: rule
                .filter(|rule| rule.verdict.as_deref() == Some("always"))
                .map(|rule| {
                    rule.kind
                        .as_deref()
                        .and_then(RecordKind::parse)
                        .unwrap_or(RecordKind::Receipt)
                }),
            issuer_name: rule.and_then(|rule| rule.issuer_name.as_deref()),
        }
    }
}

/// File records from newly synced messages.
pub async fn scan_messages<Tz>(
    store: &Store,
    cfg: &PassConfig<Tz>,
    message_ids: &[MessageId],
) -> anyhow::Result<PassSummary>
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    let rows = store.list_todo_scan_rows(message_ids).await?;
    let mut summary = PassSummary::default();
    let mut senders: HashMap<AccountId, Senders> = HashMap::new();
    let mut touched: Vec<AccountId> = Vec::new();
    for row in &rows {
        if !senders.contains_key(&row.account_id) {
            senders.insert(
                row.account_id.clone(),
                Senders::load(store, &row.account_id).await?,
            );
        }
        let account_senders = senders.get(&row.account_id).cloned().unwrap_or_default();
        let found = classify_or_skip(store, cfg, row, &account_senders).await;
        if found.filed + found.updated > 0 && !touched.contains(&row.account_id) {
            touched.push(row.account_id.clone());
        }
        summary.add(&found);
    }
    for account in touched {
        regroup(store, &account, cfg.now).await?;
    }
    Ok(summary)
}

/// Read up to `max_pages` pages of the account's history, newest first,
/// from where the run got to.
pub async fn run_first_run<Tz>(
    store: &Store,
    cfg: &PassConfig<Tz>,
    account_id: &AccountId,
    page_size: u32,
    max_pages: u32,
) -> anyhow::Result<FirstRunProgress>
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    let mut run = match store.get_record_run(account_id).await? {
        Some(run) if run.rules_version == RULES_VERSION => run,
        _ => {
            store
                .start_record_run(account_id, RULES_VERSION, cfg.now)
                .await?;
            store
                .get_record_run(account_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("records run vanished after starting"))?
        }
    };
    let mut summary = PassSummary::default();
    if run.completed_at.is_some() {
        return Ok(FirstRunProgress {
            complete: true,
            scanned: run.scanned,
            reached: run.cursor.map(|(date, _)| date),
            summary,
        });
    }
    let senders = Senders::load(store, account_id).await?;
    for _ in 0..max_pages {
        let page = store
            .list_messages_for_todo_scan(account_id, run.cursor.as_ref(), page_size)
            .await?;
        let Some(last) = page.last() else {
            regroup(store, account_id, cfg.now).await?;
            store.complete_record_run(account_id, cfg.now).await?;
            return Ok(FirstRunProgress {
                complete: true,
                scanned: run.scanned,
                reached: run.cursor.map(|(date, _)| date),
                summary,
            });
        };
        let cursor = (last.date, last.id.clone());
        for row in &page {
            summary.add(&classify_or_skip(store, cfg, row, &senders).await);
        }
        store
            .advance_record_run(account_id, &cursor, page.len() as i64)
            .await?;
        run.scanned += page.len() as i64;
        run.cursor = Some(cursor);
    }
    if summary.filed + summary.updated > 0 {
        regroup(store, account_id, cfg.now).await?;
    }
    Ok(FirstRunProgress {
        complete: false,
        scanned: run.scanned,
        reached: run.cursor.map(|(date, _)| date),
        summary,
    })
}

async fn classify_or_skip<Tz>(
    store: &Store,
    cfg: &PassConfig<Tz>,
    row: &TodoScanRow,
    senders: &Senders,
) -> PassSummary
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    match classify(store, cfg, row, senders).await {
        Ok(summary) => summary,
        Err(error) => {
            tracing::warn!(message = %row.id, %error, "records detection skipped a message");
            PassSummary {
                scanned: 1,
                ..PassSummary::default()
            }
        }
    }
}

async fn classify<Tz>(
    store: &Store,
    cfg: &PassConfig<Tz>,
    row: &TodoScanRow,
    senders: &Senders,
) -> anyhow::Result<PassSummary>
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    let mut summary = PassSummary {
        scanned: 1,
        ..PassSummary::default()
    };
    let flags = MessageFlags::from_bits_truncate(row.flags);
    if row.outbound
        || flags.intersects(MessageFlags::SPAM | MessageFlags::TRASH)
        || senders.never(&row.from_email)
    {
        return Ok(summary);
    }
    let view = senders.view(&row.from_email);
    if view.always.is_none()
        && !WORTH_READING.is_match(&row.subject)
        && !WORTH_READING.is_match(&row.snippet)
    {
        return Ok(summary);
    }
    let Some(message) = MessageText::load(store, row).await? else {
        return Ok(summary);
    };
    summary.bodies_read = 1;
    for detection in detect(&message.input(row), &cfg.tz, view) {
        let filing = plan_filing(store, cfg, row_ref(row), &detection, "detector").await?;
        let filed = store.file_record(&filing).await?;
        summary.count(detection.kind.as_str(), &filed);
    }
    Ok(summary)
}

/// The text of one message, cleaned for the rules.
struct MessageText {
    text: String,
    html: Option<String>,
    list_mail: bool,
}

impl MessageText {
    async fn load(store: &Store, row: &TodoScanRow) -> anyhow::Result<Option<Self>> {
        let Some(body) = store.get_body(&row.id).await? else {
            return Ok(None);
        };
        let text = clean(
            body.text_plain.as_deref(),
            body.text_html.as_deref(),
            &ReaderConfig::default(),
        )
        .content;
        Ok(Some(Self {
            text,
            html: body.text_html,
            list_mail: body.metadata.list_id.is_some(),
        }))
    }

    fn input<'a>(&'a self, row: &'a TodoScanRow) -> DetectInput<'a> {
        DetectInput {
            subject: &row.subject,
            body_text: &self.text,
            body_html: self.html.as_deref(),
            from_name: row.from_name.as_deref(),
            from_email: &row.from_email,
            sent: row.date,
            list_mail: self.list_mail,
        }
    }
}

/// The message a filing links.
#[derive(Debug, Clone)]
pub struct MessageRef {
    pub account_id: AccountId,
    pub message_id: MessageId,
    pub thread_id: ThreadId,
    pub date: DateTime<Utc>,
}

fn row_ref(row: &TodoScanRow) -> MessageRef {
    MessageRef {
        account_id: row.account_id.clone(),
        message_id: row.id.clone(),
        thread_id: row.thread_id.clone(),
        date: row.date,
    }
}

/// Kinds a reference may join across issuer names: a carrier's delivery
/// email names the shop's order; a booking site's email names the hotel's.
const JOINS_ACROSS_ISSUERS: &[&str] = &["order", "booking", "ticket"];

/// The record key for a detection: an existing record with the same
/// reference, else kind, issuer and reference, else the message itself.
async fn dedup_key(
    store: &Store,
    account_id: &AccountId,
    detection: &Detection,
    message_id: &MessageId,
) -> anyhow::Result<String> {
    let issuer = issuer_key(&detection.issuer);
    let Some(reference) = detection
        .reference
        .as_deref()
        .filter(|r| !r.trim().is_empty())
    else {
        return Ok(format!(
            "msg|{}|{}",
            message_id.as_str(),
            detection.kind.as_str()
        ));
    };
    let family: &[&str] = match detection.kind {
        RecordKind::Order | RecordKind::Receipt => &["order", "receipt"],
        RecordKind::Booking | RecordKind::Ticket => &["booking", "ticket"],
        other => &[other.as_str()],
    };
    let reference_trimmed = reference.trim();
    if let Some(existing) = store
        .find_record_by_reference(account_id, family, reference_trimmed)
        .await?
    {
        let same_issuer = existing.issuer_key.as_deref() == Some(issuer.as_str());
        if same_issuer || JOINS_ACROSS_ISSUERS.contains(&existing.kind.as_str()) {
            return Ok(existing.dedup_key);
        }
    }
    Ok(format!(
        "{}|{issuer}|{}",
        detection.kind.as_str(),
        reference_key(reference)
    ))
}

/// What filing `detection` from `message` writes. The dry run of a manual
/// filing shows this plan; the real filing writes it.
pub async fn plan_filing<Tz>(
    store: &Store,
    cfg: &PassConfig<Tz>,
    message: MessageRef,
    detection: &Detection,
    filed_by: &str,
) -> anyhow::Result<RecordFiling>
where
    Tz: TimeZone,
{
    let dedup_key = dedup_key(store, &message.account_id, detection, &message.message_id).await?;
    let source_key = message.message_id.as_str();
    Ok(RecordFiling {
        id: new_record_id(),
        account_id: message.account_id.clone(),
        dedup_key,
        kind: detection.kind.as_str().to_string(),
        origin: detection.origin.as_str().to_string(),
        reason: detection.reason.clone(),
        rules_version: RULES_VERSION,
        links: vec![RecordLink {
            message_id: message.message_id.clone(),
            thread_id: Some(message.thread_id.clone()),
            stage: detection.stage.as_str().to_string(),
            message_at: message.date,
            filed_by: filed_by.to_string(),
        }],
        fields: detection
            .fields
            .iter()
            .map(|found| found.to_store(&source_key, Some(&message.message_id), message.date))
            .collect(),
        now: cfg.now,
    })
}

pub fn new_record_id() -> String {
    format!("rec_{}", uuid::Uuid::new_v4().simple())
}

fn new_group_id() -> String {
    format!("grp_{}", uuid::Uuid::new_v4().simple())
}

/// The plan for filing one message by hand, as `kind` when given.
pub async fn plan_manual<Tz>(
    store: &Store,
    cfg: &PassConfig<Tz>,
    message_id: &MessageId,
    kind: Option<RecordKind>,
) -> anyhow::Result<RecordFiling>
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    let row = store
        .list_todo_scan_rows(std::slice::from_ref(message_id))
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("message {message_id} not found"))?;
    let senders = Senders::load(store, &row.account_id).await?;
    let message = MessageText::load(store, &row)
        .await?
        .unwrap_or_else(|| MessageText {
            text: row.snippet.clone(),
            html: None,
            list_mail: false,
        });
    let detection = manual(
        &message.input(&row),
        &cfg.tz,
        senders.view(&row.from_email),
        kind,
    );
    plan_filing(store, cfg, row_ref(&row), &detection, "manual").await
}

/// The kind of record a ticked-off to-do leaves, if any: a paid bill is an
/// invoice, a signed lease or renewed policy a contract.
pub fn todo_record_kind(todo_kind: &str) -> Option<RecordKind> {
    match todo_kind {
        "bill" | "payment_failed" => Some(RecordKind::Invoice),
        "renewal" | "sign" | "lease" => Some(RecordKind::Contract),
        "document" => Some(RecordKind::Account),
        _ => None,
    }
}

/// Files the record a ticked-off to-do leaves behind: the source email's
/// record when the rules find one, else a record built from the to-do,
/// with the day it was ticked off as its date. Returns `None` for to-dos
/// that leave no record (a verify link, an RSVP, a promise).
pub async fn file_from_todo<Tz>(
    store: &Store,
    cfg: &PassConfig<Tz>,
    todo: &TodoRecord,
) -> anyhow::Result<Option<RecordFiled>>
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    let Some(kind) = todo_record_kind(&todo.kind) else {
        return Ok(None);
    };
    let source_key = format!("todo:{}", todo.id);
    let row = match &todo.source_message_id {
        Some(id) => store
            .list_todo_scan_rows(std::slice::from_ref(id))
            .await?
            .into_iter()
            .next(),
        None => None,
    };
    let detected = match &row {
        Some(row) => {
            let senders = Senders::load(store, &row.account_id).await?;
            match MessageText::load(store, row).await? {
                Some(message) => {
                    detect(&message.input(row), &cfg.tz, senders.view(&row.from_email))
                        .into_iter()
                        .next()
                }
                None => None,
            }
        }
        None => None,
    };
    let todo_sources = mxr_todo::provenance::FieldSources::from_json(&todo.field_sources);
    let amount_checked = todo_sources.get("amount").is_some_and(|provenance| {
        matches!(
            provenance.source,
            mxr_todo::provenance::FieldSource::Schema | mxr_todo::provenance::FieldSource::User
        )
    });
    let done_at = todo.done_at.unwrap_or(cfg.now);
    let mut todo_fields = vec![Found {
        // The tick-off is the user's own act, so its day is checked.
        checked: true,
        ..Found::at(
            FieldName::IssuedAt,
            Source::Todo,
            day_at(done_at.with_timezone(&cfg.tz).date_naive()),
            "the day you ticked it off",
        )
    }];
    if let (Some(minor), Some(currency)) = (todo.amount_minor, todo.currency.as_deref()) {
        let mut found = Found::money(Source::Todo, minor, currency, "the to-do's amount");
        found.checked = amount_checked;
        todo_fields.push(found);
    }
    let (mut filing, filed_by) = match (detected, &row) {
        (Some(detection), Some(row)) => (
            plan_filing(store, cfg, row_ref(row), &detection, "todo").await?,
            "todo",
        ),
        _ => {
            let mut fields = vec![Found::text(
                FieldName::Title,
                Source::Todo,
                strip_verb(&todo.title),
            )];
            if let Some(counterparty) = &todo.counterparty {
                fields.push(Found::text(
                    FieldName::Issuer,
                    Source::Todo,
                    counterparty.clone(),
                ));
            }
            let filing = RecordFiling {
                id: new_record_id(),
                account_id: todo.account_id.clone(),
                dedup_key: format!("todo|{}", todo.id),
                kind: kind.as_str().to_string(),
                origin: "todo".to_string(),
                reason: "you ticked off the to-do it came from".to_string(),
                rules_version: RULES_VERSION,
                links: row
                    .as_ref()
                    .map(|row| RecordLink {
                        message_id: row.id.clone(),
                        thread_id: Some(row.thread_id.clone()),
                        stage: Stage::Receipt.as_str().to_string(),
                        message_at: row.date,
                        filed_by: "todo".to_string(),
                    })
                    .into_iter()
                    .collect(),
                fields: fields
                    .iter()
                    .map(|found| {
                        found.to_store(&source_key, todo.source_message_id.as_ref(), done_at)
                    })
                    .collect(),
                now: cfg.now,
            };
            (filing, "todo")
        }
    };
    for link in &mut filing.links {
        link.filed_by = filed_by.to_string();
    }
    filing.fields.extend(
        todo_fields
            .iter()
            .map(|found| found.to_store(&source_key, todo.source_message_id.as_ref(), done_at)),
    );
    Ok(Some(store.file_record(&filing).await?))
}

/// "Pay council tax" is filed as "Council tax".
fn strip_verb(title: &str) -> String {
    let mut words = title.splitn(2, ' ');
    let first = words.next().unwrap_or_default();
    let rest = words.next().unwrap_or_default();
    let verbs = [
        "pay", "fix", "renew", "sign", "give", "send", "return", "do", "verify",
    ];
    if verbs.contains(&first.to_ascii_lowercase().as_str()) && !rest.is_empty() {
        crate_capitalise(rest)
    } else {
        title.to_string()
    }
}

fn crate_capitalise(value: &str) -> String {
    let mut chars = value.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().collect::<String>() + chars.as_str()
    })
}

/// Files the order each delivered parcel completes, joining the record its
/// order number already has. Returns how many records changed.
pub async fn file_delivered<Tz>(
    store: &Store,
    cfg: &PassConfig<Tz>,
    account_id: &AccountId,
) -> anyhow::Result<u64>
where
    Tz: TimeZone,
{
    let mut changed = 0;
    for delivery in store.list_deliveries(DeliveryListFilter::Delivered).await? {
        if &delivery.account_id != account_id
            || delivery.dismissed_at.is_some()
            || delivery.delivered_at.is_none()
        {
            continue;
        }
        let message_ids = store.delivery_message_ids(&delivery.id).await?;
        let rows = store.list_todo_scan_rows(&message_ids).await?;
        // Newest first: the delivered email is the latest.
        let Some(latest) = rows.first() else { continue };
        let issuer = delivery.merchant.clone().unwrap_or_else(|| {
            crate::rules::issuer_from_sender(latest.from_name.as_deref(), &latest.from_email)
        });
        let detection = Detection {
            kind: RecordKind::Order,
            stage: Stage::Delivered,
            origin: Origin::Rule,
            reason: "a delivered parcel".to_string(),
            issuer: issuer.clone(),
            reference: delivery.order_number.clone(),
            account_ref: None,
            fields: Vec::new(),
        };
        let dedup_key = match &delivery.order_number {
            Some(_) => dedup_key(store, account_id, &detection, &latest.id).await?,
            None => format!("delivery|{}", delivery.id.as_str()),
        };
        let source_key = format!("delivery:{}", latest.id.as_str());
        let mut fields = vec![Found::text(FieldName::Issuer, Source::Delivery, issuer)];
        if let Some(number) = &delivery.order_number {
            fields.push(Found::text(
                FieldName::Reference,
                Source::Delivery,
                number.clone(),
            ));
        }
        let items: Vec<&str> = delivery
            .items
            .iter()
            .map(|item| item.name.as_str())
            .collect();
        if let Some(first) = items.first() {
            let title = match items.len() {
                1 => (*first).to_string(),
                n => format!("{first} and {} more", n - 1),
            };
            fields.push(Found::text(FieldName::Title, Source::Delivery, title));
        }
        if let Some(delivered) = delivery.delivered_at {
            let mut found = Found::at(
                FieldName::DeliveredAt,
                Source::Delivery,
                delivered,
                "the delivery tracker",
            );
            found.checked = delivery.source == "schema";
            fields.push(found);
        }
        let filing = RecordFiling {
            id: new_record_id(),
            account_id: account_id.clone(),
            dedup_key,
            kind: RecordKind::Order.as_str().to_string(),
            origin: "delivery".to_string(),
            reason: "a delivered parcel".to_string(),
            rules_version: RULES_VERSION,
            links: rows
                .iter()
                .enumerate()
                .map(|(index, row)| RecordLink {
                    message_id: row.id.clone(),
                    thread_id: Some(row.thread_id.clone()),
                    stage: if index == 0 {
                        Stage::Delivered
                    } else {
                        Stage::Shipped
                    }
                    .as_str()
                    .to_string(),
                    message_at: row.date,
                    filed_by: "delivery".to_string(),
                })
                .collect(),
            fields: fields
                .iter()
                .map(|found| found.to_store(&source_key, Some(&latest.id), latest.date))
                .collect(),
            now: cfg.now,
        };
        if matches!(
            store.file_record(&filing).await?,
            RecordFiled::Inserted { .. } | RecordFiled::Updated { .. }
        ) {
            changed += 1;
        }
    }
    Ok(changed)
}

/// Rebuilds the account's trips and series from its records.
pub async fn regroup(
    store: &Store,
    account_id: &AccountId,
    now: DateTime<Utc>,
) -> anyhow::Result<()> {
    let records = store
        .list_archive_records(&mxr_store::RecordQuery {
            account_ids: Some(vec![account_id.clone()]),
            ..mxr_store::RecordQuery::default()
        })
        .await?;
    let inputs: Vec<GroupInput> = records.iter().filter_map(group_input).collect();
    let plans = group::plan(&inputs);
    let groups: Vec<(RecordGroup, Vec<String>)> = plans
        .into_iter()
        .map(|plan| {
            (
                RecordGroup {
                    id: new_group_id(),
                    account_id: account_id.clone(),
                    kind: plan.kind.as_str().to_string(),
                    group_key: plan.key,
                    title: plan.title,
                    span_start: plan.span_start,
                    span_end: plan.span_end,
                },
                plan.members,
            )
        })
        .collect();
    store
        .replace_record_groups(account_id, &groups, now)
        .await?;
    Ok(())
}

fn group_input(record: &ArchiveRecord) -> Option<GroupInput> {
    Some(GroupInput {
        id: record.id.clone(),
        kind: RecordKind::parse(&record.kind)?,
        issuer_key: record.issuer_key.clone(),
        issuer: record.issuer.clone(),
        date: record.ledger_date(),
        span_start: record.span_start,
        span_end: record.span_end,
        place: record.place.clone(),
    })
}

#[cfg(test)]
mod tests;
