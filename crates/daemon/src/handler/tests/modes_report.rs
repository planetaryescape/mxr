//! Counts and timings for Now, the rail, membership and done's preview on
//! a copy of a real mailbox. Ignored: run it by hand against a `.backup`
//! copy, never the live database, with
//!
//! ```text
//! MXR_MODES_REPORT_DIR=/path/to/copy-dir cargo test -p mxr --lib \
//!   modes_report -- --ignored --nocapture
//! ```
//!
//! where the directory holds `mxr.db`. It prints counts and milliseconds
//! only, never a subject, address or name.

use super::*;
use crate::handler::{mode_done, modes, now};
use chrono::{Duration, Local, Utc};
use mxr_core::id::{AccountId, ThreadId};
use mxr_protocol::ModeKindData;
use std::collections::BTreeMap;
use std::time::Instant;

#[tokio::test]
#[ignore = "reads a copy of a real mailbox named by MXR_MODES_REPORT_DIR"]
async fn modes_report() {
    let Ok(dir) = std::env::var("MXR_MODES_REPORT_DIR") else {
        eprintln!("set MXR_MODES_REPORT_DIR to a directory holding a copy of mxr.db");
        return;
    };
    let config_dir = std::env::temp_dir().join("mxr-modes-report-config");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::env::set_var("MXR_DATA_DIR", &dir);
    std::env::set_var("MXR_CONFIG_DIR", &config_dir);
    let state = AppState::new().await.unwrap();
    let at = Utc::now();

    let started = Instant::now();
    let data = now::now_at(&state, None, at, &Local).await.unwrap();
    println!("GetNow: {:.0} ms", started.elapsed().as_secs_f64() * 1000.0);
    println!(
        "  people total {} (shown {}), overload line {}, due soon total {} (shown {}), updates card {:?}, reading pick {}, item count {}",
        data.people.total,
        data.people.rows.len(),
        data.people.overload_line.is_some(),
        data.due_soon.total,
        data.due_soon.todos.len(),
        data.updates
            .as_ref()
            .map(|card| (card.message_count, card.source_count)),
        data.reading.is_some(),
        data.item_count,
    );
    let new_senders = data
        .people
        .rows
        .iter()
        .filter(|row| row.new_sender.is_some())
        .count();
    println!("  new-sender questions on shown people rows: {new_senders}");

    let started = Instant::now();
    let rail = match modes::get_rail(&state, None).await.unwrap() {
        ResponseData::Rail { rail } => rail,
        other => panic!("{other:?}"),
    };
    println!(
        "GetRail: {:.0} ms",
        started.elapsed().as_secs_f64() * 1000.0
    );
    for entry in &rail.entries {
        println!(
            "  {:<9} count {:?} badge {:?}",
            entry.id, entry.count, entry.badge
        );
    }

    // Membership over every thread with mail in the last 30 days.
    let since = (at - Duration::days(30)).timestamp();
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT DISTINCT account_id, thread_id FROM messages WHERE date >= ?1")
            .bind(since)
            .fetch_all(state.store.reader())
            .await
            .unwrap();
    let mut by_account: BTreeMap<String, Vec<ThreadId>> = BTreeMap::new();
    for (account, thread) in rows {
        by_account
            .entry(account)
            .or_default()
            .push(thread.parse().unwrap());
    }
    let mut combos: BTreeMap<String, usize> = BTreeMap::new();
    let mut per_mode: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut questions = 0usize;
    let mut no_mode: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut threads = 0usize;
    let mut slowest_ms = 0f64;
    let mut sample: Vec<ThreadId> = Vec::new();
    let started = Instant::now();
    for (account, ids) in &by_account {
        let account: AccountId = account.parse().unwrap();
        for chunk in ids.chunks(modes::MEMBERSHIP_MAX_THREADS) {
            let batch = Instant::now();
            let placed = modes::place_threads(&state, &account, chunk, at)
                .await
                .unwrap();
            slowest_ms = slowest_ms.max(batch.elapsed().as_secs_f64() * 1000.0);
            let all: Vec<mxr_store::DeskMessage> = placed
                .iter()
                .flat_map(|placement| placement.messages.iter().cloned())
                .collect();
            let senders = crate::handler::desk::Senders::load(&state, &account, &all)
                .await
                .unwrap();
            let is_self = crate::handler::desk::self_matcher(&state, &account)
                .await
                .unwrap();
            for placement in placed {
                if placement.data.modes.is_empty() {
                    *no_mode
                        .entry(no_mode_reason(&placement, &senders, &is_self, at))
                        .or_default() += 1;
                }
                threads += 1;
                let names: Vec<&str> = placement
                    .data
                    .modes
                    .iter()
                    .map(|entry| entry.mode.id())
                    .collect();
                for name in &names {
                    *per_mode.entry(name).or_default() += 1;
                }
                let key = if names.is_empty() {
                    "(none)".to_string()
                } else {
                    names.join("+")
                };
                *combos.entry(key).or_default() += 1;
                questions += usize::from(placement.data.new_sender.is_some());
                if !placement.data.modes.is_empty() && sample.len() < 20 {
                    sample.push(placement.data.thread_id.clone());
                }
            }
        }
    }
    println!(
        "Membership: {threads} threads active in 30 days, {:.0} ms in all, slowest batch of {} {:.0} ms",
        started.elapsed().as_secs_f64() * 1000.0,
        modes::MEMBERSHIP_MAX_THREADS,
        slowest_ms
    );
    for (mode, count) in &per_mode {
        println!("  in {mode}: {count}");
    }
    for (combo, count) in &combos {
        println!("  {combo}: {count}");
    }
    println!("  new-sender questions: {questions}");
    println!("No mode, by why:");
    for (reason, count) in &no_mode {
        println!("  {reason}: {count}");
    }

    // Done's preview on threads that are in a mode: plans only, no writes.
    for mode in [ModeKindData::Messages, ModeKindData::Updates] {
        let started = Instant::now();
        let response = mode_done::set_mode_done(
            &state,
            mode_done::DoneRequest {
                thread_ids: &sample,
                mode,
                dry_run: true,
                todo_ids: &[],
                sender: None,
            },
        )
        .await
        .unwrap();
        let ResponseData::ModeDone { items, .. } = response else {
            panic!("expected ModeDone")
        };
        let would_archive = items.iter().filter(|item| item.archived > 0).count();
        println!(
            "SetModeDone dry run ({}, {} threads): {:.0} ms, {would_archive} would archive",
            mode.id(),
            items.len(),
            started.elapsed().as_secs_f64() * 1000.0
        );
    }
}

/// Why a thread active in the window sits in no mode, judged on its
/// newest inbound message still in the inbox. Counts only.
fn no_mode_reason(
    placement: &modes::Placement,
    senders: &crate::handler::desk::Senders,
    is_self: &dyn Fn(&str) -> bool,
    at: chrono::DateTime<Utc>,
) -> &'static str {
    use crate::handler::desk_lanes::is_outbound;
    use crate::handler::mail_kind::{classify, SenderKind};
    let messages = &placement.messages;
    if messages.iter().all(|m| m.trashed) {
        return "all in trash or spam";
    }
    let inbound: Vec<&mxr_store::DeskMessage> = messages
        .iter()
        .filter(|m| !is_outbound(m, is_self) && !m.trashed)
        .collect();
    if inbound.is_empty() {
        return if messages.iter().any(|m| m.in_inbox) {
            "sent only, in inbox"
        } else {
            "sent only, not in inbox"
        };
    }
    let in_inbox: Vec<&&mxr_store::DeskMessage> = inbound.iter().filter(|m| m.in_inbox).collect();
    let Some(latest) = in_inbox.iter().max_by_key(|m| (m.date, m.seq)) else {
        let latest = inbound.iter().max_by_key(|m| (m.date, m.seq)).unwrap();
        return match classify(&modes::signals(latest, senders, is_self)).kind {
            SenderKind::Person => "archived: person",
            SenderKind::List => "archived: list or newsletter",
            SenderKind::Automated => "archived: automated",
            SenderKind::Denied => "archived: screened out",
        };
    };
    if latest.snoozed {
        return "in inbox: snoozed";
    }
    if latest.is_delivery {
        return "in inbox: delivery";
    }
    if latest.is_invite {
        return "in inbox: invite";
    }
    let old = latest.date < at - Duration::days(7);
    match classify(&modes::signals(latest, senders, is_self)).kind {
        SenderKind::Person if old => "in inbox: person, newest inbound over 7 days old",
        SenderKind::Person => "in inbox: person, newest inbound within 7 days",
        SenderKind::List => "in inbox: list or newsletter (done mark)",
        SenderKind::Automated => "in inbox: automated (done mark)",
        SenderKind::Denied => "in inbox: screened out",
    }
}
