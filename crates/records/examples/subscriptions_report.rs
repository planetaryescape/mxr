//! Finds subscriptions in a copy of a real mailbox's records and prints
//! counts and timings only: never an issuer, title or amount.
//!
//! ```text
//! sqlite3 "file:$HOME/Library/Application Support/mxr/mxr.db?mode=ro" ".backup /tmp/copy.db"
//! cargo run -p mxr-records --example subscriptions_report -- /tmp/copy.db
//! ```
//!
//! Opening the copy runs migrations on it, so never point it at the live
//! database.

use chrono::{Datelike, Utc};
use mxr_records::subscriptions::{self, Cadence, Status};
use mxr_store::{RecordQuery, Store};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::Instant;

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let path: PathBuf = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: subscriptions_report <copy.db>"))?
        .into();
    if path.to_string_lossy().contains("Application Support/mxr/") {
        anyhow::bail!("refusing to run against the live database; back it up first");
    }
    let store = Store::new(&path).await?;
    let now = Utc::now();

    let started = Instant::now();
    let loaded = subscriptions::load(&store, None, now, &chrono::Local).await?;
    let load_ms = started.elapsed().as_secs_f64() * 1000.0;
    let found = &loaded.detection.subscriptions;
    println!(
        "load and detect, every account: {load_ms:.0}ms over {} receipts and invoices",
        loaded.records.len()
    );

    let mut by_cadence: BTreeMap<&str, usize> = BTreeMap::new();
    let mut by_status: BTreeMap<&str, usize> = BTreeMap::new();
    for subscription in found {
        *by_cadence.entry(subscription.cadence.as_str()).or_default() += 1;
        *by_status.entry(subscription.status.as_str()).or_default() += 1;
    }
    println!(
        "subscriptions: {} ({by_cadence:?}; {by_status:?})",
        found.len()
    );
    println!(
        "  with a product named: {}, told apart by amount: {}",
        found.iter().filter(|s| s.product.is_some()).count(),
        found.iter().filter(|s| s.product.is_none()).count()
    );
    println!(
        "  yearly seen twice (unconfirmed): {}",
        found
            .iter()
            .filter(|s| s.cadence == Cadence::Yearly && !s.confirmed())
            .count()
    );
    println!(
        "  ended by a cancellation email: {}",
        found.iter().filter(|s| s.cancellation.is_some()).count()
    );
    let changes: usize = found.iter().map(|s| s.price_changes.len()).sum();
    println!(
        "price changes: {changes} across {} subscriptions",
        found.iter().filter(|s| !s.price_changes.is_empty()).count()
    );
    let today = subscriptions::local_day(now, &chrono::Local);
    let signals = subscriptions::signals(found, today);
    println!(
        "signals now: {} price changes, {} missed charges, {} renewals approaching",
        signals
            .iter()
            .filter(|s| matches!(s, subscriptions::Signal::PriceChange { .. }))
            .count(),
        signals
            .iter()
            .filter(|s| matches!(s, subscriptions::Signal::MissedCharge { .. }))
            .count(),
        signals
            .iter()
            .filter(|s| matches!(s, subscriptions::Signal::RenewalApproaching { .. }))
            .count()
    );
    let one_offs: usize = loaded.detection.one_offs.values().map(Vec::len).sum();
    println!(
        "one-offs from issuers with a subscription: {one_offs} records from {} issuers",
        loaded.detection.one_offs.len()
    );
    println!(
        "totals: {} currencies, live {}",
        subscriptions::totals(found).len(),
        found.iter().filter(|s| s.status != Status::Ended).count()
    );

    // The old rule: an issuer with three receipts, invoices or statements
    // in three different months was a series.
    let records = store.list_archive_records(&RecordQuery::default()).await?;
    // (account, issuer) -> (count, the (year, month) pairs seen).
    type SeriesCounts = BTreeMap<(String, String), (usize, BTreeSet<(i32, u32)>)>;
    let mut old: SeriesCounts = BTreeMap::new();
    let mut statement_issuers: BTreeSet<(String, String)> = BTreeSet::new();
    for record in &records {
        let Some(issuer) = record.issuer_key.clone().filter(|k| !k.is_empty()) else {
            continue;
        };
        if !matches!(record.kind.as_str(), "invoice" | "statement" | "receipt") {
            continue;
        }
        let key = (record.account_id.as_str(), issuer);
        let entry = old.entry(key.clone()).or_default();
        entry.0 += 1;
        if let Some(date) = record.ledger_date() {
            entry.1.insert((date.year(), date.month()));
        }
        if record.kind == "statement" {
            statement_issuers.insert(key);
        }
    }
    let old_series: Vec<&(String, String)> = old
        .iter()
        .filter(|(_, (count, months))| *count >= 3 && months.len() >= 3)
        .map(|(key, _)| key)
        .collect();
    let live_issuers: BTreeSet<&str> = found.iter().map(|s| s.issuer_key.as_str()).collect();
    let groups = store.list_record_groups(None).await?;
    let survived_as_subscription = old_series
        .iter()
        .filter(|(_, issuer)| live_issuers.contains(issuer.as_str()))
        .count();
    let survived_as_statements = old_series
        .iter()
        .filter(|key| statement_issuers.contains(**key))
        .count();
    println!(
        "old series rule: {} series; {survived_as_subscription} have a subscription now, {survived_as_statements} have statements (still a series if they recur), {} stored series groups",
        old_series.len(),
        groups.iter().filter(|g| g.kind == "series").count()
    );

    // The new rule, written to the copy: series are statements only.
    for account in store.list_accounts().await? {
        mxr_records::pass::regroup(&store, &account.id, now).await?;
    }
    let regrouped = store.list_record_groups(None).await?;
    println!(
        "new series rule (statements only): {} series",
        regrouped.iter().filter(|g| g.kind == "series").count()
    );

    let started = Instant::now();
    for _ in 0..10 {
        let _ = subscriptions::load(&store, None, now, &chrono::Local).await?;
    }
    println!(
        "load and detect, mean of 10: {:.0}ms",
        started.elapsed().as_secs_f64() * 100.0
    );
    Ok(())
}
