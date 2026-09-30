//! Scale guards for `list_owed_replies`.
//!
//! The ranking used to run in Rust over every candidate row: on a
//! 110k-message mailbox that read 88k rows per call and took about a
//! second. It now runs in SQL over `idx_messages_owed`. These tests prove
//! the SQL returns exactly what the Rust ranking did, and keep the query
//! plan on that index.

use super::*;
use crate::test_fixtures::test_account;
use crate::Store;

/// The query before the ranking moved into SQL, plus `msg_rowid`: the old
/// order of two messages sharing a thread and date was the scan order of
/// `idx_messages_account_direction_date`, which is insertion order.
const REFERENCE_SQL: &str = r#"WITH inbound_latest AS (
        SELECT
            thread_id,
            MAX(date) AS latest_inbound_at
        FROM messages
        WHERE account_id = ?1 AND direction = 'inbound'
        GROUP BY thread_id
    ),
    outbound_latest AS (
        SELECT
            thread_id,
            MAX(date) AS latest_outbound_at
        FROM messages
        WHERE account_id = ?1 AND direction = 'outbound'
        GROUP BY thread_id
    ),
    owed AS (
        SELECT
            inbound_latest.thread_id,
            inbound_latest.latest_inbound_at
        FROM inbound_latest
        LEFT JOIN outbound_latest USING (thread_id)
        WHERE outbound_latest.latest_outbound_at IS NULL
           OR outbound_latest.latest_outbound_at <= inbound_latest.latest_inbound_at
    )
    SELECT
        m.id AS msg_id,
        m.rowid AS msg_rowid,
        m.thread_id,
        m.from_email,
        m.from_name,
        m.subject,
        m.date AS latest_inbound_at,
        contacts.cadence_days_p50 AS contact_cadence,
        COALESCE(contacts.is_list_sender, 0) AS is_list_sender,
        COALESCE(screener_decisions.disposition, '') AS screener_disposition
    FROM owed
    JOIN messages m
      ON m.thread_id = owed.thread_id
     AND m.date = owed.latest_inbound_at
     AND m.account_id = ?1
     AND m.direction = 'inbound'
    -- Bare columns on the table side, so the (account_id, email)
    -- keys are used: contacts are stored lowercase, and
    -- screener_decisions.sender_email is COLLATE NOCASE. Wrapping
    -- them in LOWER() scanned every contact per candidate thread.
    LEFT JOIN contacts
      ON contacts.account_id = m.account_id
     AND contacts.email = LOWER(m.from_email)
    LEFT JOIN screener_decisions
      ON screener_decisions.account_id = m.account_id
     AND screener_decisions.sender_email = LOWER(m.from_email)
    WHERE COALESCE(contacts.is_list_sender, 0) = 0
      AND COALESCE(screener_decisions.disposition, '') != 'deny'
      -- The waiting window, when asked for: a large mailbox has tens of
      -- thousands of unanswered threads, most of them years old.
      AND (?2 IS NULL OR m.date >= ?2)
      AND (?3 IS NULL OR m.date <= ?3)
    "#;

/// The ranking before it moved into SQL: every candidate read into Rust,
/// scored, sorted and truncated.
async fn reference_list(
    store: &Store,
    account_id: &AccountId,
    older_than_days: Option<u32>,
    within_days: Option<u32>,
    limit: u32,
    now_unix: i64,
) -> Vec<OwedReplyRow> {
    let global_p50: Option<f64> = sqlx::query_scalar(
        "SELECT AVG(cadence_days_p50)
         FROM contacts
         WHERE account_id = ? AND cadence_days_p50 IS NOT NULL",
    )
    .bind(account_id.as_str())
    .fetch_optional(store.reader())
    .await
    .unwrap()
    .flatten();
    let global_p50 = global_p50.unwrap_or(DEFAULT_EXPECTED_DAYS);

    let day = 86_400_i64;
    let rows = sqlx::query(REFERENCE_SQL)
        .bind(account_id.as_str())
        .bind(within_days.map(|days| now_unix - i64::from(days) * day))
        .bind(older_than_days.map(|days| now_unix - i64::from(days) * day))
        .fetch_all(store.reader())
        .await
        .unwrap();

    let mut owed = Vec::new();
    for row in rows {
        let inbound_at_secs: i64 = row.get("latest_inbound_at");
        let waiting_secs = now_unix - inbound_at_secs;
        if waiting_secs < 0 {
            continue;
        }
        let waiting_days = waiting_secs as f64 / 86_400.0;
        if older_than_days.is_some_and(|min| waiting_days < f64::from(min)) {
            continue;
        }
        if within_days.is_some_and(|max| waiting_days > f64::from(max)) {
            continue;
        }
        let contact_cadence: Option<f64> = row.try_get("contact_cadence").ok();
        let expected_days = contact_cadence
            .filter(|v| v.is_finite() && *v > 0.0)
            .unwrap_or(global_p50)
            .max(0.5);
        let msg_rowid: i64 = row.get("msg_rowid");
        owed.push((
            msg_rowid,
            OwedReplyRow {
                thread_id: decode_id(row.get::<&str, _>("thread_id")).unwrap(),
                latest_inbound_msg_id: decode_id(row.get::<&str, _>("msg_id")).unwrap(),
                from_email: row.get("from_email"),
                from_name: row.get("from_name"),
                subject: row.get("subject"),
                latest_inbound_at: decode_timestamp(inbound_at_secs).unwrap(),
                waiting_days,
                expected_days,
                overdue_score: waiting_days / expected_days,
            },
        ));
    }
    owed.sort_by(|(a_rowid, a), (b_rowid, b)| {
        b.overdue_score
            .partial_cmp(&a.overdue_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.latest_inbound_at.cmp(&a.latest_inbound_at))
            .then(a.thread_id.as_str().cmp(&b.thread_id.as_str()))
            .then(a_rowid.cmp(b_rowid))
    });
    owed.truncate(limit as usize);
    owed.into_iter().map(|(_, row)| row).collect()
}

/// Small deterministic generator (xorshift64), so a failure reproduces.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

const SENDERS: u64 = 200;

fn sender_email(sender: u64) -> String {
    // Some senders arrive with capitals; contacts are keyed lowercase.
    if sender.is_multiple_of(7) {
        format!("Sender{sender}@Example.com")
    } else {
        format!("sender{sender}@example.com")
    }
}

/// Seeds `threads` threads shaped like a real mailbox's edge cases:
/// several inbound messages per thread, two latest inbound messages
/// sharing a date, replies before, at and after the latest inbound,
/// future-dated mail, `unknown` direction, list senders, screener
/// decisions, and cadences that are missing, zero, negative, tiny, huge
/// or not a number.
async fn seed_mailbox(store: &Store, threads: u64, now_unix: i64) -> AccountId {
    let account = test_account();
    store.insert_account(&account).await.unwrap();
    let account_id = account.id;

    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let day = 86_400_i64;
    let mut message = 0_u128;
    let mut tx = store.writer().begin().await.unwrap();
    for thread in 0..threads {
        let thread_id = uuid::Uuid::from_u128(u128::from(thread) + 1).to_string();
        let mut latest = now_unix - (rng.below(900) as i64) * day - rng.below(day as u64) as i64;
        if rng.below(100) == 0 {
            latest = now_unix + 3_600;
        }
        let mut sent: Vec<(i64, &str, String)> = Vec::new();
        for earlier in 0..rng.below(3) {
            sent.push((
                latest - (earlier as i64 + 1) * day,
                "inbound",
                sender_email(rng.below(SENDERS)),
            ));
        }
        sent.push((latest, "inbound", sender_email(rng.below(SENDERS))));
        if rng.below(20) == 0 {
            sent.push((latest, "inbound", sender_email(rng.below(SENDERS))));
        }
        match rng.below(10) {
            0 => sent.push((latest - day, "outbound", "me@example.com".into())),
            1 => sent.push((latest, "outbound", "me@example.com".into())),
            2 => sent.push((latest + 60, "outbound", "me@example.com".into())),
            3 => sent.push((latest + 60, "unknown", "me@example.com".into())),
            _ => {}
        }
        for (date, direction, from_email) in sent {
            message += 1;
            let id = uuid::Uuid::from_u128((1_u128 << 64) + message).to_string();
            sqlx::query(
                "INSERT INTO messages
                    (id, account_id, provider_id, thread_id, from_name, from_email,
                     to_addrs, subject, date, direction)
                 VALUES (?, ?, ?, ?, ?, ?, '[]', ?, ?, ?)",
            )
            .bind(&id)
            .bind(account_id.as_str())
            .bind(format!("p-{message}"))
            .bind(&thread_id)
            .bind(message.is_multiple_of(3).then(|| format!("Name {message}")))
            .bind(from_email)
            .bind(format!("subject {thread}"))
            .bind(date)
            .bind(direction)
            .execute(&mut *tx)
            .await
            .unwrap();
        }
    }

    for sender in 0..SENDERS {
        if sender.is_multiple_of(9) {
            continue;
        }
        let cadence = match sender % 8 {
            0 => "NULL".to_string(),
            1 => "0.0".to_string(),
            2 => "-3.0".to_string(),
            3 => "0.1".to_string(),
            4 => "'not a number'".to_string(),
            5 => "1e300".to_string(),
            _ => format!("{}.25", sender % 40),
        };
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO contacts
                (account_id, email, first_seen_at, last_seen_at, refreshed_at,
                 cadence_days_p50, is_list_sender)
             VALUES (?, ?, 0, 0, 0, {cadence}, ?)"
        )))
        .bind(account_id.as_str())
        .bind(sender_email(sender).to_lowercase())
        .bind(i64::from(sender.is_multiple_of(17)))
        .execute(&mut *tx)
        .await
        .unwrap();
        let disposition = match sender % 23 {
            0 => Some("deny"),
            1 => Some("allow"),
            _ => None,
        };
        if let Some(disposition) = disposition {
            sqlx::query(
                "INSERT INTO screener_decisions
                    (account_id, sender_email, disposition, decided_at)
                 VALUES (?, ?, ?, 0)",
            )
            .bind(account_id.as_str())
            // Mixed case on purpose: sender_email is COLLATE NOCASE.
            .bind(sender_email(sender).to_uppercase())
            .bind(disposition)
            .execute(&mut *tx)
            .await
            .unwrap();
        }
    }
    tx.commit().await.unwrap();
    account_id
}

#[tokio::test]
async fn sql_ranking_returns_exactly_what_the_rust_ranking_did() {
    let store = Store::in_memory().await.unwrap();
    let now_unix = Utc::now().timestamp();
    let account_id = seed_mailbox(&store, 3_000, now_unix).await;

    let mut compared = 0;
    for (older_than_days, within_days) in [
        (None, None),
        (Some(3), None),
        (None, Some(30)),
        (Some(10), Some(200)),
        (Some(0), Some(0)),
    ] {
        let everything = reference_list(
            &store,
            &account_id,
            older_than_days,
            within_days,
            u32::MAX,
            now_unix,
        )
        .await;
        for limit in [0, 1, 7, 50, u32::MAX] {
            let expected = &everything[..everything.len().min(limit as usize)];
            let actual = store
                .list_owed_replies_at(&account_id, older_than_days, within_days, limit, now_unix)
                .await
                .unwrap();
            assert_eq!(
                actual, expected,
                "older_than={older_than_days:?} within={within_days:?} limit={limit}"
            );
            compared += actual.len();
        }
    }
    // The fixture must exercise the ranking, not compare empty lists.
    assert!(compared > 5_000, "only {compared} rows compared");
}

/// The per-thread work stays inside `idx_messages_owed`, and each sender's
/// contact and screener rows are found by key: wrapping the table's column
/// in LOWER() once scanned every contact per thread and took the query past
/// two minutes. Asserted on a populated store because the planner's choice
/// can depend on the data.
#[tokio::test]
async fn owed_query_plan_uses_the_owed_index_without_scans() {
    let store = Store::in_memory().await.unwrap();
    let account_id = seed_mailbox(&store, 3_000, Utc::now().timestamp()).await;
    let plan: Vec<String> = sqlx::query(sqlx::AssertSqlSafe(format!(
        "EXPLAIN QUERY PLAN {OWED_REPLIES_SQL}"
    )))
    .bind(account_id.as_str())
    .fetch_all(store.reader())
    .await
    .unwrap()
    .into_iter()
    .map(|row| row.get::<String, _>("detail"))
    .collect();

    for step in [
        "SEARCH messages USING COVERING INDEX idx_messages_owed (account_id=? AND direction=?)",
        "SEARCH m USING COVERING INDEX idx_messages_owed (account_id=? AND direction=? AND thread_id=? AND date=?)",
        "SEARCH outbound USING COVERING INDEX idx_messages_owed (account_id=? AND direction=? AND thread_id=? AND date>?)",
        "SEARCH contacts USING INDEX sqlite_autoindex_contacts_1 (account_id=? AND email=?) LEFT-JOIN",
        "SEARCH screener_decisions USING INDEX sqlite_autoindex_screener_decisions_1 (account_id=? AND sender_email=?) LEFT-JOIN",
    ] {
        assert!(
            plan.iter().any(|detail| detail == step),
            "expected `{step}`: {plan:#?}"
        );
    }
    for bad in [
        "SCAN messages",
        "SCAN m",
        "SCAN outbound",
        "SCAN contacts",
        "SCAN screener_decisions",
    ] {
        assert!(
            !plan
                .iter()
                .any(|detail| detail == bad || detail.starts_with(&format!("{bad} "))),
            "{bad} must not appear: {plan:#?}"
        );
    }
    assert!(
        !plan
            .iter()
            .any(|detail| detail.contains("AUTOMATIC") || detail.contains("FOR GROUP BY")),
        "no automatic index or GROUP BY sort: {plan:#?}"
    );
}

/// Records timing on a generated mailbox the size of the real one that
/// took a second (~90k candidate threads). Run with
/// `cargo test -p mxr-store --release --lib owed_replies_timing -- --ignored --nocapture`.
#[tokio::test]
#[ignore = "timing record; run in release"]
async fn owed_replies_timing_on_a_large_generated_mailbox() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(&dir.path().join("owed.db")).await.unwrap();
    let now_unix = Utc::now().timestamp();
    let seeded = Instant::now();
    let account_id = seed_mailbox(&store, 90_000, now_unix).await;
    eprintln!("seeded 90000 threads in {:?}", seeded.elapsed());

    let mut best = std::time::Duration::MAX;
    for _ in 0..5 {
        let started = Instant::now();
        let rows = store
            .list_owed_replies(&account_id, None, None, 50)
            .await
            .unwrap();
        best = best.min(started.elapsed());
        assert_eq!(rows.len(), 50);
    }
    eprintln!("list_owed_replies limit 50, best of 5: {best:?}");
    assert!(best.as_millis() < 500, "owed took {best:?}");
}

/// Compares the SQL ranking with the Rust ranking on a disposable copy of
/// a real store, and times both. Prints counts, timings and a hash, never
/// mail content. The copy is migrated, so never point it at a live store:
///
/// ```sh
/// cp -c "$HOME/Library/Application Support/mxr/mxr.db" /tmp/owed-copy.db
/// MXR_OWED_BENCH_DB=/tmp/owed-copy.db MXR_OWED_BENCH_ACCOUNT=<account id> \
///   cargo test -p mxr-store --release --lib owed_replies_on_a_store_copy -- --ignored --nocapture
/// ```
#[tokio::test]
#[ignore = "needs MXR_OWED_BENCH_DB pointing at a disposable copy of a real store"]
async fn owed_replies_on_a_store_copy() {
    use std::hash::{Hash, Hasher};

    let path = std::env::var("MXR_OWED_BENCH_DB").expect("MXR_OWED_BENCH_DB");
    let account_id: AccountId = std::env::var("MXR_OWED_BENCH_ACCOUNT")
        .expect("MXR_OWED_BENCH_ACCOUNT")
        .parse()
        .unwrap();
    let store = Store::new(std::path::Path::new(&path)).await.unwrap();
    let now_unix = Utc::now().timestamp();

    for limit in [50, u32::MAX] {
        let started = Instant::now();
        let expected = reference_list(&store, &account_id, None, None, limit, now_unix).await;
        let reference_elapsed = started.elapsed();
        let started = Instant::now();
        let actual = store
            .list_owed_replies_at(&account_id, None, None, limit, now_unix)
            .await
            .unwrap();
        let elapsed = started.elapsed();
        assert!(actual == expected, "limit {limit}: rows differ");
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        for row in &actual {
            row.thread_id.as_str().hash(&mut hash);
            row.latest_inbound_msg_id.as_str().hash(&mut hash);
            row.latest_inbound_at.timestamp().hash(&mut hash);
            row.expected_days.to_bits().hash(&mut hash);
        }
        eprintln!(
            "limit {limit}: {} identical rows, hash {:016x}, rust ranking {reference_elapsed:?}, sql ranking {elapsed:?}",
            actual.len(),
            hash.finish()
        );
    }
    // The rubric's number is a warm call, so time repeats of the public path.
    for run in 0..3 {
        let started = Instant::now();
        let rows = store
            .list_owed_replies(&account_id, None, None, 50)
            .await
            .unwrap();
        eprintln!(
            "warm run {run}: {} rows in {:?}",
            rows.len(),
            started.elapsed()
        );
    }
}
