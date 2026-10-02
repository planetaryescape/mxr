//! Runs To do's first run over a copy of a real mailbox and prints counts
//! and timings only: never a title, amount, due phrase or address.
//!
//! ```text
//! sqlite3 "file:$HOME/Library/Application Support/mxr/mxr.db?mode=ro" ".backup /tmp/copy.db"
//! cargo run -p mxr-todo --example first_run_report -- /tmp/copy.db
//! ```
//!
//! It writes to the copy (migrations and to-do rows), so never point it at
//! the live database.

use chrono::{Duration, Utc};
use mxr_store::Store;
use mxr_todo::pass::{run_first_run, sweep, PassConfig};
use sqlx::Row;
use std::path::PathBuf;
use std::time::Instant;

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let path: PathBuf = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: first_run_report <copy.db>"))?
        .into();
    if path.to_string_lossy().contains("Application Support/mxr/") {
        anyhow::bail!("refusing to run against the live database; back it up first");
    }
    let store = Store::new(&path).await?;
    let now = Utc::now();
    // Gmail and the fake provider stamp `mx.google.com`; others get no
    // trusted id, as in the daemon.
    let trusted_authserv = store
        .list_accounts()
        .await?
        .into_iter()
        .map(|account| {
            let google = account.sync_backend.is_some_and(|backend| {
                matches!(
                    backend.provider_kind,
                    mxr_core::types::ProviderKind::Gmail | mxr_core::types::ProviderKind::Fake
                )
            });
            let ids = if google {
                vec!["mx.google.com".to_string()]
            } else {
                Vec::new()
            };
            (account.id, ids)
        })
        .collect();
    let cfg = PassConfig {
        now,
        tz: chrono::Local,
        morning_hour: 9,
        catchup_days: 14,
        catchup_max: 25,
        trusted_authserv,
    };

    let started = Instant::now();
    for account in store.list_accounts().await? {
        let account_started = Instant::now();
        loop {
            let progress = run_first_run(&store, &cfg, &account.id, 500, 20).await?;
            if progress.complete {
                break;
            }
        }
        println!(
            "first run, one account: {:.1}s",
            account_started.elapsed().as_secs_f64()
        );
    }
    println!(
        "first run, all accounts: {:.1}s",
        started.elapsed().as_secs_f64()
    );

    let sweep_started = Instant::now();
    let swept = sweep(&store, now).await?;
    println!(
        "sweep: {} answered RSVPs, {} expired, {} surfaced, {:.0}ms",
        swept.rsvp_answered,
        swept.expired,
        swept.surfaced.len(),
        sweep_started.elapsed().as_secs_f64() * 1000.0
    );

    let pool = sqlx::SqlitePool::connect(&format!("sqlite:{}?mode=ro", path.display())).await?;
    let week = (now + Duration::days(30)).timestamp();
    let bands = [
        (
            "Now",
            "state = 'open' AND COALESCE(catchup, '') <> 'pending'
             AND COALESCE(scheduled_for, surface_at) IS NOT NULL
             AND COALESCE(scheduled_for, surface_at) <= ?1",
        ),
        (
            "Coming up (30 days)",
            "state = 'open' AND COALESCE(catchup, '') <> 'pending'
             AND COALESCE(scheduled_for, surface_at) > ?1 AND COALESCE(scheduled_for, surface_at) <= ?2",
        ),
        (
            "Later",
            "state = 'open' AND COALESCE(catchup, '') <> 'pending'
             AND COALESCE(scheduled_for, surface_at) > ?2",
        ),
        (
            "Whenever",
            "state = 'open' AND COALESCE(catchup, '') <> 'pending'
             AND COALESCE(scheduled_for, surface_at) IS NULL",
        ),
        ("Catch-up", "state = 'open' AND catchup = 'pending'"),
        ("Catch-up overflow (expired)", "catchup = 'overflow'"),
        ("Expired at birth", "state = 'expired' AND expired_at_birth = 1 AND COALESCE(catchup, '') <> 'overflow'"),
        ("Expired by sweep", "state = 'expired' AND expired_at_birth = 0"),
        (
            "Past window in Now (must be 0)",
            "state = 'open' AND COALESCE(catchup, '') <> 'pending'
             AND COALESCE(scheduled_for, surface_at) <= ?1
             AND relevant_until IS NOT NULL AND relevant_until < ?1",
        ),
    ];
    println!("\nband | kind | count");
    for (band, condition) in bands {
        let sql = format!(
            "SELECT kind, COUNT(*) AS n FROM todos WHERE {condition} GROUP BY kind ORDER BY kind"
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(now.timestamp())
            .bind(week)
            .fetch_all(&pool)
            .await?;
        let total: i64 = rows.iter().map(|row| row.get::<i64, _>("n")).sum();
        println!("{band} | all | {total}");
        for row in rows {
            println!(
                "{band} | {} | {}",
                row.get::<String, _>("kind"),
                row.get::<i64, _>("n")
            );
        }
    }
    let gated: (i64, i64) = sqlx::query_as(
        "SELECT COALESCE(SUM(action_url IS NOT NULL), 0), COALESCE(SUM(action_trusted), 0)
         FROM todos WHERE state = 'open'",
    )
    .fetch_one(&pool)
    .await?;
    println!(
        "\nopen rows with a link: {}, of them one-click: {}",
        gated.0, gated.1
    );
    let unchecked: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM todos WHERE state = 'open' AND field_sources LIKE '%\"checked\":false%'",
    )
    .fetch_one(&pool)
    .await?;
    println!("open rows with an unchecked date: {unchecked}");

    println!("\nquery | ms");
    let account = store
        .list_accounts()
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("no accounts"))?;
    macro_rules! timed {
        ($label:expr, $body:expr) => {{
            let started = Instant::now();
            let _ = $body;
            println!(
                "{} | {:.1}",
                $label,
                started.elapsed().as_secs_f64() * 1000.0
            );
        }};
    }
    timed!(
        "list_runway_todos",
        store
            .list_runway_todos(None, now - Duration::days(7))
            .await?
    );
    timed!("list_catchup_todos", store.list_catchup_todos(None).await?);
    timed!(
        "count_todos_expired_since",
        store
            .count_todos_expired_since(None, now - Duration::days(7))
            .await?
    );
    timed!(
        "list_todos_in_state(expired, 200)",
        store
            .list_todos_in_state(None, mxr_store::TodoState::Expired, 200)
            .await?
    );
    timed!(
        "list_messages_for_todo_scan(500)",
        store
            .list_messages_for_todo_scan(&account.id, None, 500)
            .await?
    );
    timed!(
        "sender_history",
        store
            .sender_history(&account.id, "example-not-a-sender.test", now)
            .await?
    );
    timed!(
        "established_sender_hosts",
        store.established_sender_hosts(&account.id, 3, 60).await?
    );
    timed!(
        "list_promises_for_todos",
        store.list_promises_for_todos(&account.id).await?
    );
    timed!(
        "promise_fingerprint",
        store.promise_fingerprint(&account.id).await?
    );
    timed!("expire_lapsed_todos", store.expire_lapsed_todos(now).await?);
    timed!(
        "claim_surfaced_todos",
        store.claim_surfaced_todos(now).await?
    );
    timed!(
        "complete_answered_rsvp_todos",
        store.complete_answered_rsvp_todos(now).await?
    );

    let rerun_started = Instant::now();
    store
        .start_todo_run(&account.id, mxr_todo::RULES_VERSION, now)
        .await?;
    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM todos WHERE surfaced_at IS NOT NULL OR state <> 'open' OR catchup IS NOT NULL")
        .fetch_one(&pool)
        .await?;
    let mut rerun = mxr_todo::pass::PassSummary::default();
    loop {
        let progress = run_first_run(&store, &cfg, &account.id, 500, 20).await?;
        rerun.created += progress.summary.created;
        rerun.updated += progress.summary.updated;
        rerun.reopened += progress.summary.reopened;
        rerun.catchup += progress.summary.catchup;
        if progress.complete {
            break;
        }
    }
    println!(
        "\nre-run: {} created, {} updated, {} reopened, {} new catch-up, {:.1}s",
        rerun.created,
        rerun.updated,
        rerun.reopened,
        rerun.catchup,
        rerun_started.elapsed().as_secs_f64()
    );
    let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM todos WHERE surfaced_at IS NOT NULL OR state <> 'open' OR catchup IS NOT NULL")
        .fetch_one(&pool)
        .await?;
    println!(
        "rows with a claim, expiry or catch-up place: {before} before the re-run, {after} after"
    );
    Ok(())
}
