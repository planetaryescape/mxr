//! Project explicit user records and sent draft acceptances, never background detections.
use crate::{
    detect::{Detection, Origin},
    pass::{build_record, record_outcome, PassConfig, PassSummary},
    provenance::{FieldProvenance, FieldSource, FieldSources},
    text, TodoKind, RULES_VERSION,
};
use chrono::TimeZone;
use mxr_core::{types::MessageFlags, AccountId};
use mxr_store::{Store, TodoScanRow, TodoState};
use std::collections::HashMap;

pub async fn sync<Tz>(
    store: &Store,
    cfg: &PassConfig<Tz>,
    account: &AccountId,
    first_run: bool,
) -> anyhow::Result<PassSummary>
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    sync_selected(store, cfg, account, first_run, None).await
}

/// Acknowledging one explicit date does not scan unrelated account promises.
pub async fn sync_commitment<Tz>(
    store: &Store,
    cfg: &PassConfig<Tz>,
    account: &AccountId,
    id: &str,
) -> anyhow::Result<PassSummary>
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    sync_selected(store, cfg, account, false, Some(id)).await
}

async fn sync_selected<Tz>(
    store: &Store,
    cfg: &PassConfig<Tz>,
    account: &AccountId,
    first_run: bool,
    id: Option<&str>,
) -> anyhow::Result<PassSummary>
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    let promises = if let Some(id) = id {
        store
            .promise_for_todo(account, id)
            .await?
            .into_iter()
            .collect()
    } else {
        store.list_promises_for_todos(account).await?
    };
    let todos = if let Some(id) = id {
        store
            .unique_commitment_todo(account, id)
            .await?
            .into_iter()
            .collect()
    } else {
        store.list_promise_todos(account).await?
    };
    let existing: HashMap<_, _> = todos
        .into_iter()
        .filter_map(|todo| Some((todo.commitment_id.clone()?, todo)))
        .collect();
    let mut summary = PassSummary::default();
    for promise in promises {
        // Closed tasks retain the user's decision; edited fields are protected by the date-only CAS.
        let prior = existing.get(&promise.commitment_id);
        if prior.is_some_and(|todo| matches!(todo.state, TodoState::Done | TodoState::Dismissed)) {
            continue;
        }
        if promise.status != "open" {
            continue;
        }
        if prior.is_some_and(|todo| todo.due_at == promise.by_when) {
            continue;
        }
        let Some(evidence_date) = promise.evidence_date else {
            continue;
        };
        let person = promise
            .contact_name
            .as_deref()
            .map(text::strip_glued_id)
            .filter(|name| !name.is_empty())
            .unwrap_or(&promise.email);
        let explicit = promise.commitment_id.contains("::promise::")
            || promise.by_when_source == Some(mxr_store::ByWhenSource::User);
        let source = if explicit {
            FieldSource::User
        } else {
            FieldSource::Model
        };
        let mut fields = FieldSources::default();
        fields.set(
            "kind",
            FieldProvenance::with_evidence(FieldSource::User, "accepted promise"),
        );
        fields.set(
            "title",
            FieldProvenance::with_evidence(source, &promise.what),
        );
        fields.set(
            "requester",
            FieldProvenance::with_evidence(FieldSource::User, format!("you promised {person}")),
        );
        fields.set(
            "accepted_commitment",
            FieldProvenance::with_evidence(FieldSource::User, &promise.commitment_id),
        );
        if promise.by_when.is_some() {
            fields.set(
                "due_at",
                FieldProvenance::with_evidence(
                    if promise.by_when_source == Some(mxr_store::ByWhenSource::User) {
                        FieldSource::User
                    } else {
                        FieldSource::Rule
                    },
                    "accepted promise date provenance",
                ),
            );
        }
        let row = TodoScanRow {
            id: promise.evidence_msg_id.clone(),
            account_id: account.clone(),
            thread_id: promise.thread_id.clone(),
            from_name: None,
            from_email: String::new(),
            date: evidence_date,
            subject: String::new(),
            snippet: String::new(),
            flags: MessageFlags::SENT.bits(),
            outbound: true,
        };
        let what = text::strip_urls(&promise.what);
        let detection = Detection {
            kind: prior
                .and_then(|todo| TodoKind::parse(&todo.kind))
                .unwrap_or(TodoKind::Promise),
            verb: what
                .split_whitespace()
                .next()
                .unwrap_or("do")
                .to_ascii_lowercase(),
            doc_type: None,
            title: text::capitalise(&text::clip(&what, 100)),
            counterparty: Some(person.into()),
            sender_domain: None,
            amount: None,
            due: promise.by_when.map(|at| crate::dates::FoundDate {
                at,
                words: String::new(),
                checked: true,
            }),
            event_start: None,
            origin: Origin::Rule,
            reason: promise.what.clone(),
            fields,
            link: None,
            object_key: promise.commitment_id.clone(),
        };
        let (mut record, placement) = build_record(cfg, &row, detection, first_run);
        record.commitment_id = Some(promise.commitment_id.clone());
        record.dedup_key = format!("promise|{}", promise.commitment_id);
        record.rules_version = RULES_VERSION;
        // A canonical asks promise already linked to this accepted record keeps its source quote and identity.
        if let Some(prior) = prior {
            record.dedup_key = prior.dedup_key.clone();
            if prior.has_stored_asks() {
                record.title = prior.title.clone();
                record.reason = prior.reason.clone();
                record.field_sources = prior.field_sources.clone();
            }
        }
        let outcome = store
            .upsert_accepted_promise_todo(&record, &promise)
            .await?;
        record_outcome(&mut summary, &record, placement, outcome);
    }
    if summary.catchup > 0 {
        summary.catchup_overflow += store.trim_todo_catchup(cfg.catchup_max, cfg.now).await?;
    }
    Ok(summary)
}
