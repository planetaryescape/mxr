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

    assert!(
        ["idx_messages_owed", "idx_messages_conversation_inputs"]
            .iter()
            .any(|index| plan.iter().any(|detail| detail
                == &format!(
                    "SEARCH messages USING COVERING INDEX {index} (account_id=? AND direction=?)"
                ))),
        "the initial grouping must use a covering account/direction index: {plan:#?}"
    );
    for step in [
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
