//! Counts and timings for the Updates digest on a copy of a real mailbox.
//! Ignored: run it by hand against a `.backup` copy, never the live
//! database, with
//!
//! ```text
//! MXR_UPDATES_REPORT_DIR=/path/to/copy-dir cargo test -p mxr --lib \
//!   updates_report -- --ignored --nocapture
//! ```
//!
//! where the directory holds `mxr.db`. It writes facts into the copy and
//! prints counts and milliseconds only, never a subject, address or name.

use super::*;
use crate::handler::updates::{self, LetGo};
use chrono::{Local, Utc};
use std::time::Instant;

#[tokio::test]
#[ignore = "reads a copy of a real mailbox named by MXR_UPDATES_REPORT_DIR"]
async fn updates_report() {
    let Ok(dir) = std::env::var("MXR_UPDATES_REPORT_DIR") else {
        eprintln!("set MXR_UPDATES_REPORT_DIR to a directory holding a copy of mxr.db");
        return;
    };
    let config_dir = std::env::temp_dir().join("mxr-updates-report-config");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::env::set_var("MXR_DATA_DIR", &dir);
    std::env::set_var("MXR_CONFIG_DIR", &config_dir);
    let state = AppState::new().await.unwrap();
    let at = Utc::now();

    for run in ["first (facts derived)", "second (facts cached)"] {
        let started = Instant::now();
        let digest = updates::digest_at(&state, None, None, false, true, at, &Local)
            .await
            .unwrap();
        println!(
            "GetUpdatesDigest, {run}: {:.0} ms",
            started.elapsed().as_secs_f64() * 1000.0
        );
        let lines = |section: &[mxr_protocol::UpdateLineData]| {
            let messages: u32 = section.iter().map(|line| line.count).sum();
            (section.len(), messages)
        };
        let (needs, needs_messages) = lines(&digest.needs_a_look);
        let (changed, changed_messages) = lines(&digest.changed);
        let (routine, routine_messages) = lines(&digest.routine);
        let parcels = digest
            .needs_a_look
            .iter()
            .chain(&digest.changed)
            .chain(&digest.routine)
            .filter(|line| line.tracker.as_ref().is_some_and(|t| t.kind == "parcel"))
            .count();
        let builds = digest
            .needs_a_look
            .iter()
            .chain(&digest.changed)
            .chain(&digest.routine)
            .filter(|line| line.tracker.as_ref().is_some_and(|t| t.kind != "parcel"))
            .count();
        let in_todo = digest
            .needs_a_look
            .iter()
            .filter(|line| line.todo_id.is_some())
            .count();
        let deltas = digest
            .changed
            .iter()
            .chain(&digest.routine)
            .filter(|line| line.delta.is_some())
            .count();
        let mut expired_by_kind: std::collections::BTreeMap<&str, usize> =
            std::collections::BTreeMap::new();
        for item in &digest.expired {
            *expired_by_kind.entry(item.kind.as_str()).or_default() += 1;
        }
        println!(
            "  cut {} ({}), {} messages from {} sources shown; {} sources seen, {} muted",
            digest.cut.label,
            digest.cut.title,
            digest.message_count,
            digest.source_count,
            digest.source_total,
            digest.muted_total
        );
        println!(
            "  needs a look {needs} lines ({needs_messages} messages, {in_todo} in To do), changed {changed} lines ({changed_messages}), routine {routine} lines ({routine_messages})"
        );
        println!("  trackers: {parcels} parcels, {builds} builds or incidents; {deltas} lines with a delta");
        println!(
            "  since the cut: {} messages from {} sources",
            digest.since.message_count, digest.since.source_count
        );
        println!("  expired (never shown): {expired_by_kind:?}");
    }

    let started = Instant::now();
    let preview = updates::let_go_at(
        &state,
        &LetGo {
            account_id: None,
            cut: None,
            source_key: None,
            selection_token: None,
            dry_run: true,
        },
        at,
        &Local,
    )
    .await
    .unwrap();
    println!(
        "LetGoDigest dry run: {:.0} ms, {} messages from {} sources in {} threads, {} hidden, {} also in To do",
        started.elapsed().as_secs_f64() * 1000.0,
        preview.message_count,
        preview.source_count,
        preview.thread_ids.len(),
        preview.hidden_count,
        preview.in_todo_count
    );

    // Alerts that lead Needs a look as a suggested to-do; none is ever
    // made into a to-do without the user.
    let digest = updates::digest_at(&state, None, None, false, false, at, &Local)
        .await
        .unwrap();
    let suggested = digest
        .needs_a_look
        .iter()
        .chain(&digest.since.lines)
        .filter(|line| line.todo_suggestion.is_some())
        .count();
    println!("Suggested to-dos (digest and since): {suggested}");

    let quiet = state
        .store
        .list_deliveries(mxr_store::DeliveryListFilter::Active)
        .await
        .unwrap();
    let now_quiet = quiet
        .iter()
        .filter(|d| crate::handler::updates_digest::parcel_quiet_since(d).is_some_and(|q| q <= at))
        .count();
    println!(
        "Deliveries: {} stored as active, {now_quiet} of them quiet and no longer listed",
        quiet.len()
    );
}
