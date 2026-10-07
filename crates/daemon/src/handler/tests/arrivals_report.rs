//! Counts and timings for the arrivals line on a copy of a real mailbox
//! (D119, rubric X14). Ignored: run it by hand against a `.backup` copy,
//! never the live database, with
//!
//! ```text
//! MXR_ARRIVALS_REPORT_DIR=/path/to/copy-dir cargo test -p mxr --lib \
//!   arrivals_report -- --ignored --nocapture
//! ```
//!
//! where the directory holds `mxr.db`. Opening it runs migration 70, whose
//! backfill is part of what this measures. It prints counts and
//! milliseconds only, never a subject, address or name.

use super::*;
use crate::handler::arrivals;
use chrono::{Duration, Local, Utc};
use mxr_core::id::{AccountId, MessageId};
use std::collections::BTreeMap;
use std::time::Instant;

fn ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.0
}

#[tokio::test]
#[ignore = "reads a copy of a real mailbox named by MXR_ARRIVALS_REPORT_DIR"]
async fn arrivals_report() {
    let Ok(dir) = std::env::var("MXR_ARRIVALS_REPORT_DIR") else {
        eprintln!("set MXR_ARRIVALS_REPORT_DIR to a directory holding a copy of mxr.db");
        return;
    };
    let config_dir = std::env::temp_dir().join("mxr-arrivals-report-config");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::env::set_var("MXR_DATA_DIR", &dir);
    std::env::set_var("MXR_CONFIG_DIR", &config_dir);

    let started = Instant::now();
    let state = AppState::new().await.unwrap();
    println!("open + migrate (backfill rows): {:.0} ms", ms(started));
    let backfilled: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM arrivals")
        .fetch_one(state.store.reader())
        .await
        .unwrap();
    println!("  arrivals rows after the backfill: {backfilled}");

    let accounts: Vec<AccountId> = state
        .store
        .list_accounts()
        .await
        .unwrap()
        .into_iter()
        .filter(|account| account.enabled)
        .map(|account| account.id)
        .collect();
    let started = Instant::now();
    let mut placed = 0;
    for account in &accounts {
        placed += arrivals::place_pending(&state, account).await.unwrap();
    }
    println!("placing the backfill: {placed} rows in {:.0} ms", ms(started));

    let now = Utc::now();
    for (label, hours) in [("24h", 24), ("7d", 24 * 7)] {
        let since = now - Duration::hours(hours);
        let started = Instant::now();
        let counts = state
            .store
            .arrival_counts(&accounts, since, now + Duration::seconds(1))
            .await
            .unwrap();
        let elapsed = ms(started);
        let mut parts: Vec<String> = mxr_protocol::ArrivalBucketData::ALL
            .into_iter()
            .filter_map(|bucket| {
                let n = counts.by_mode.get(bucket.id()).copied().unwrap_or(0);
                (n > 0).then(|| bucket.label(n))
            })
            .collect();
        let known: u32 = mxr_protocol::ArrivalBucketData::ALL
            .into_iter()
            .map(|bucket| counts.by_mode.get(bucket.id()).copied().unwrap_or(0))
            .sum();
        if known != counts.total {
            parts.push(format!("{} not placed", counts.total - known));
        }
        println!(
            "{label}: {} arrived. {}. Also {} in To do, {} in Archive. Sums: {} ({elapsed:.0} ms)",
            counts.total,
            parts.join(" · "),
            counts.also_todo,
            counts.also_archive,
            known == counts.total
        );
        // Each count opens exactly that many emails.
        for (bucket, n) in &counts.by_mode {
            let started = Instant::now();
            let (_, total) = state
                .store
                .list_arrivals(&accounts, since, now + Duration::seconds(1), Some(bucket), 50)
                .await
                .unwrap();
            println!(
                "  list {bucket}: {total} (count {n}) {:.0} ms",
                ms(started)
            );
            assert_eq!(total, *n, "{bucket}");
        }
        let rules: Vec<(String, i64)> = sqlx::query_as(
            "SELECT COALESCE(rule, 'sorting'), COUNT(*) FROM arrivals
             WHERE first_seen_at >= ?1 GROUP BY 1 ORDER BY 2 DESC",
        )
        .bind(since.timestamp())
        .fetch_all(state.store.reader())
        .await
        .unwrap();
        println!("  rules: {rules:?}");
        let never_bury: Vec<(String, i64)> = sqlx::query_as(
            "SELECT COALESCE(now_mode, mode), COUNT(*) FROM arrivals
             WHERE first_seen_at >= ?1 AND rule = 'written_to' GROUP BY 1",
        )
        .bind(since.timestamp())
        .fetch_all(state.store.reader())
        .await
        .unwrap();
        println!("  never-bury moves to Messages (N1): {never_bury:?}");
    }

    // "Not sure" per local day, and how many Now would ask (at most 3).
    let days: Vec<(String, i64)> = sqlx::query_as(
        "SELECT date(first_seen_at, 'unixepoch', 'localtime') AS day, COUNT(*)
         FROM arrivals WHERE not_sure IS NOT NULL AND first_seen_at >= ?1
         GROUP BY day ORDER BY day",
    )
    .bind((now - Duration::days(7)).timestamp())
    .fetch_all(state.store.reader())
    .await
    .unwrap();
    let asked: Vec<(String, i64, i64)> = days
        .into_iter()
        .map(|(day, n)| (day, n, n.min(3)))
        .collect();
    println!("not sure per day (day, conflicts, asked): {asked:?}");

    // The line as Now composes it, with its window a day back.
    state
        .store
        .set_mode_viewed("arrivals:since", now - Duration::hours(24))
        .await
        .unwrap();
    let started = Instant::now();
    let line = arrivals::arrivals_at(&state, None, false, now, &Local)
        .await
        .unwrap();
    println!("GetArrivals: {:.0} ms", ms(started));
    // The line names counts and a clock time only, never mail content.
    println!("  line: {}", line.line);
    println!("  clear: {:?}", line.clear_line);
    println!("  not sure today: {}", line.not_sure.len());
    println!("  track record: {:?}", line.track_record);

    // Inbox chips for a page of rows.
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT message_id FROM arrivals ORDER BY first_seen_at DESC LIMIT 200",
    )
    .fetch_all(state.store.reader())
    .await
    .unwrap();
    let ids: Vec<MessageId> = ids.iter().map(|id| id.parse().unwrap()).collect();
    let started = Instant::now();
    let chips = state.store.arrivals_by_ids(&ids).await.unwrap();
    println!(
        "GetArrivalModes for {} rows: {} chips, {:.0} ms",
        ids.len(),
        chips.len(),
        ms(started)
    );
    let modes: BTreeMap<String, usize> = chips.iter().fold(BTreeMap::new(), |mut acc, row| {
        *acc.entry(row.effective.clone()).or_default() += 1;
        acc
    });
    println!("  chips by mode: {modes:?}");
}
