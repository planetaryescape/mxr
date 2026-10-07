//! Files Archive's records over a copy of a real mailbox and prints counts
//! and timings only: never an issuer, title, amount or reference.
//!
//! ```text
//! sqlite3 "file:$HOME/Library/Application Support/mxr/mxr.db?mode=ro" ".backup /tmp/copy.db"
//! cargo run -p mxr-records --example records_report -- /tmp/copy.db
//! ```
//!
//! It writes to the copy (migrations and record rows), so never point it
//! at the live database.

use chrono::Utc;
use mxr_records::answer::{self, Candidate};
use mxr_records::pass::{file_delivered, regroup, run_first_run, PassConfig};
use mxr_store::{RecordQuery, Store};
use std::path::PathBuf;
use std::time::Instant;

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let path: PathBuf = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: records_report <copy.db>"))?
        .into();
    if path.to_string_lossy().contains("Application Support/mxr/") {
        anyhow::bail!("refusing to run against the live database; back it up first");
    }
    let store = Store::new(&path).await?;
    let now = Utc::now();
    let cfg = PassConfig {
        now,
        tz: chrono::Local,
    };

    let started = Instant::now();
    let mut scanned = 0;
    for account in store.list_accounts().await? {
        let account_started = Instant::now();
        loop {
            let progress = run_first_run(&store, &cfg, &account.id, 500, 20).await?;
            if progress.complete {
                scanned += progress.scanned;
                break;
            }
        }
        let delivered = file_delivered(&store, &cfg, &account.id).await?;
        regroup(&store, &account.id, now).await?;
        println!(
            "first run, one account: {:.1}s ({delivered} delivered orders filed or updated)",
            account_started.elapsed().as_secs_f64()
        );
    }
    println!(
        "first run, all accounts: {scanned} messages read in {:.1}s",
        started.elapsed().as_secs_f64()
    );

    let ledger_started = Instant::now();
    let records = store.list_archive_records(&RecordQuery::default()).await?;
    println!(
        "ledger query, every record: {} rows in {:.0}ms",
        records.len(),
        ledger_started.elapsed().as_secs_f64() * 1000.0
    );

    println!("records by kind:");
    for (kind, count) in store.count_archive_records_by_kind(None).await? {
        println!("  {kind:<10} {count}");
    }

    let ids: Vec<String> = records.iter().map(|r| r.id.clone()).collect();
    let fields_started = Instant::now();
    let fields = store.archive_record_fields(&ids).await?;
    let sources = store.archive_record_sources(&ids).await?;
    let documents = store.archive_record_documents(&ids).await?;
    println!(
        "fields, sources and documents for every record: {:.0}ms",
        fields_started.elapsed().as_secs_f64() * 1000.0
    );

    let mut per_record = std::collections::HashMap::<&str, usize>::new();
    for source in &sources {
        *per_record.entry(source.record_id.as_str()).or_default() += 1;
    }
    let composite_orders = records
        .iter()
        .filter(|r| r.kind == "order" && per_record.get(r.id.as_str()).copied().unwrap_or(0) > 1)
        .count();
    let groups = store.list_record_groups(None).await?;
    println!(
        "composites: {composite_orders} orders from more than one email, {} trips, {} series",
        groups.iter().filter(|g| g.kind == "trip").count(),
        groups.iter().filter(|g| g.kind == "series").count()
    );
    println!(
        "records: {} checked, {} with an unchecked amount or date",
        records.iter().filter(|r| r.checked).count(),
        records.iter().filter(|r| !r.checked).count()
    );
    let mut seen = std::collections::HashSet::new();
    let (mut checked, mut unchecked) = (0, 0);
    for (record_id, field) in &fields {
        // Winners only: the store orders candidates best first.
        if !seen.insert((record_id.as_str(), field.field.as_str())) {
            continue;
        }
        if mxr_store::RECORD_CHECKED_FIELDS.contains(&field.field.as_str()) {
            if field.checked {
                checked += 1;
            } else {
                unchecked += 1;
            }
        }
    }
    println!("money and date fields: {checked} checked, {unchecked} unchecked");
    let mut by_source = std::collections::BTreeMap::<&str, usize>::new();
    for (_, field) in &fields {
        *by_source.entry(field.source.as_str()).or_default() += 1;
    }
    println!("field candidates by source: {by_source:?}");

    let pdfs: Vec<_> = documents.iter().filter(|d| d.is_pdf()).collect();
    let pdf_bytes: i64 = pdfs.iter().map(|d| d.size_bytes).sum();
    let (on_disk_bytes, on_disk) = store.record_pdfs_on_disk().await?;
    println!(
        "PDFs: {} on record emails, {:.1} MB; {on_disk} already on disk, {:.1} MB",
        pdfs.len(),
        pdf_bytes as f64 / 1_048_576.0,
        on_disk_bytes as f64 / 1_048_576.0
    );
    let fetchable = store.record_pdfs_to_fetch(15 * 1_048_576, 100_000).await?;
    let fetch_bytes: i64 = fetchable.iter().map(|p| p.size_bytes).sum();
    println!(
        "prefetch would fetch {} PDFs under 15 MB, {:.1} MB (budget 512 MB)",
        fetchable.len(),
        fetch_bytes as f64 / 1_048_576.0
    );

    let groups_by_id: std::collections::HashMap<&str, &str> = groups
        .iter()
        .map(|g| (g.id.as_str(), g.title.as_str()))
        .collect();
    let candidates: Vec<Candidate<'_>> = records
        .iter()
        .map(|record| Candidate {
            record,
            group_title: record
                .group_id
                .as_deref()
                .and_then(|id| groups_by_id.get(id).copied()),
        })
        .collect();
    for query in [
        "booking ref",
        "receipt 2025",
        "amazon order",
        "invoice",
        "flight",
    ] {
        let started = Instant::now();
        let parsed = answer::parse(query);
        let ranked = answer::rank(&parsed, &candidates);
        println!(
            "answer box, a {}-word query: {} matches in {:.1}ms",
            query.split_whitespace().count(),
            ranked.len(),
            started.elapsed().as_secs_f64() * 1000.0
        );
    }
    Ok(())
}
