//! Reads what detection needs from the store and runs it per account.

use super::{detect, CancellationInput, ChargeInput, Detection};
use crate::RecordKind;
use chrono::{DateTime, TimeZone, Utc};
use mxr_core::id::AccountId;
use mxr_store::{ArchiveRecord, RecordQuery, Store};
use std::collections::{BTreeMap, HashMap, HashSet};

/// What [`load`] read and found.
#[derive(Debug, Clone, Default)]
pub struct Loaded {
    pub detection: Detection,
    /// The receipts and invoices it read, by id, for the clients' cards.
    pub records: HashMap<String, ArchiveRecord>,
}

/// Every account's subscriptions, ended last, and its one-offs. `None`
/// accounts means every account. Each account is detected on its own, so
/// two mailboxes paying one issuer are two subscriptions.
pub async fn load<Tz: TimeZone>(
    store: &Store,
    accounts: Option<&[AccountId]>,
    now: DateTime<Utc>,
    tz: &Tz,
) -> anyhow::Result<Loaded> {
    let records = store
        .list_archive_records(&RecordQuery {
            account_ids: accounts.map(<[AccountId]>::to_vec),
            kinds: vec![
                RecordKind::Receipt.as_str().to_string(),
                RecordKind::Invoice.as_str().to_string(),
            ],
            ..RecordQuery::default()
        })
        .await?;
    let ids: Vec<String> = records.iter().map(|r| r.id.clone()).collect();
    let stated = store.records_with_stated_title(&ids).await?;
    let mut by_account: BTreeMap<String, (AccountId, Vec<ChargeInput>)> = BTreeMap::new();
    for record in &records {
        if let Some(input) = charge_input(record, &stated) {
            by_account
                .entry(record.account_id.as_str())
                .or_insert_with(|| (record.account_id.clone(), Vec::new()))
                .1
                .push(input);
        }
    }
    let today = super::local_day(now, tz);

    // A first pass finds the subscriptions; their senders' later mail is
    // then read for cancellations, and the second pass applies them.
    let mut first: Vec<(&AccountId, &[ChargeInput], Detection)> = Vec::new();
    for (account, inputs) in by_account.values() {
        first.push((account, inputs, detect(inputs, &[], today, tz)));
    }
    let cancellations = cancellations(store, &records, &first).await?;

    let mut out = Detection::default();
    for (account, inputs, _) in first {
        let mine = cancellations.get(account).map_or(&[][..], Vec::as_slice);
        let found = detect(inputs, mine, today, tz);
        out.subscriptions.extend(found.subscriptions);
        for (issuer, ids) in found.one_offs {
            out.one_offs.entry(issuer).or_default().extend(ids);
        }
    }
    super::sort(&mut out.subscriptions);
    Ok(Loaded {
        detection: out,
        records: records.into_iter().map(|r| (r.id.clone(), r)).collect(),
    })
}

fn charge_input(record: &ArchiveRecord, stated: &HashSet<String>) -> Option<ChargeInput> {
    Some(ChargeInput {
        record_id: record.id.clone(),
        kind: RecordKind::parse(&record.kind)?,
        issuer_key: record.issuer_key.clone().filter(|k| !k.is_empty())?,
        issuer: record.issuer.clone(),
        title: record.title.clone(),
        title_stated: stated.contains(&record.id),
        amount_minor: record.amount_minor,
        currency: record.currency.clone(),
        at: record.ledger_date()?,
        checked: record.checked,
    })
}

/// Mail from the senders of each subscription's charges, sent on or after
/// its oldest last charge, that might end it, keyed by account.
async fn cancellations(
    store: &Store,
    records: &[ArchiveRecord],
    first: &[(&AccountId, &[ChargeInput], Detection)],
) -> anyhow::Result<HashMap<AccountId, Vec<CancellationInput>>> {
    let issuer_of: HashMap<&str, (&AccountId, &str)> = records
        .iter()
        .filter_map(|r| Some((r.id.as_str(), (&r.account_id, r.issuer_key.as_deref()?))))
        .collect();
    let mut members = Vec::new();
    let mut since: Option<DateTime<Utc>> = None;
    for (_, _, detection) in first {
        for subscription in &detection.subscriptions {
            members.extend(subscription.record_ids().map(str::to_string));
            let last = subscription.last_charge().at();
            since = Some(since.map_or(last, |s| s.min(last)));
        }
    }
    let Some(since) = since else {
        return Ok(HashMap::new());
    };
    // (account, sender) -> issuer key
    let mut issuer_by_sender: HashMap<(String, String), String> = HashMap::new();
    for (record_id, sender) in store.record_sender_emails(&members).await? {
        if let Some((account, issuer)) = issuer_of.get(record_id.as_str()) {
            issuer_by_sender.insert((account.as_str(), sender), (*issuer).to_string());
        }
    }
    let mut senders: Vec<String> = issuer_by_sender.keys().map(|k| k.1.clone()).collect();
    senders.sort();
    senders.dedup();
    // A day early: `since` is a midday-UTC day, and the email that ends it
    // may have come that morning.
    let since = since - chrono::Duration::days(1);
    let mut out: HashMap<AccountId, Vec<CancellationInput>> = HashMap::new();
    for message in store.ending_mail_from_senders(&senders, since).await? {
        let key = (message.account_id.as_str(), message.from_email.clone());
        if let Some(issuer) = issuer_by_sender.get(&key) {
            out.entry(message.account_id.clone())
                .or_default()
                .push(CancellationInput {
                    issuer_key: issuer.clone(),
                    subject: message.subject,
                    at: message.date,
                    message_id: message.message_id.as_str(),
                });
        }
    }
    Ok(out)
}
