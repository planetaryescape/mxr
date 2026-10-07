//! Counts and timings for Messages on a copy of a real mailbox. Ignored:
//! run it by hand against a `.backup` copy, never the live database, with
//!
//! ```text
//! MXR_MODES_REPORT_DIR=/path/to/copy-dir cargo test -p mxr --lib \
//!   messages_report -- --ignored --nocapture
//! ```
//!
//! where the directory holds `mxr.db`. It prints counts and milliseconds
//! only, never a subject, address, name or any message text.

use super::*;
use crate::handler::conversation_shape::Shape;
use crate::handler::desk::{self_matcher, Senders};
use crate::handler::desk_lanes::AccountInputs;
use crate::handler::desk_timers::DeskTimers;
use crate::handler::messages;
use chrono::{Duration, Utc};
use mxr_core::id::AccountId;
use mxr_reader::{clean, new_text, plain_text, EarlierMessage, ReaderConfig};
use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

fn ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.0
}

#[tokio::test]
#[ignore = "reads a copy of a real mailbox named by MXR_MODES_REPORT_DIR"]
async fn messages_report() {
    let Ok(dir) = std::env::var("MXR_MODES_REPORT_DIR") else {
        eprintln!("set MXR_MODES_REPORT_DIR to a directory holding a copy of mxr.db");
        return;
    };
    let config_dir = std::env::temp_dir().join("mxr-messages-report-config");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::env::set_var("MXR_DATA_DIR", &dir);
    std::env::set_var("MXR_CONFIG_DIR", &config_dir);
    let state = AppState::new().await.unwrap();
    let at = Utc::now();

    // Warm the page cache once so timings are steady-state, as in use.
    let _ = messages::messages_at(&state, None, None, 50, at)
        .await
        .unwrap();
    let started = Instant::now();
    let data = messages::messages_at(&state, None, None, 50, at)
        .await
        .unwrap();
    println!("ListMessages: {:.0} ms", ms(started));
    println!(
        "  bands: your turn {}, pinned {}, recent {} (shown {}), quiet {} (shown {})",
        data.your_turn.len(),
        data.pinned.len(),
        data.recent_total,
        data.recent.len(),
        data.quiet_total,
        data.quiet.len()
    );
    println!(
        "  rows (people and groups) {}, threads behind them {}, merge suggestions {}",
        data.row_count, data.thread_count, data.merge_suggestion_count
    );
    let groups = data
        .your_turn
        .iter()
        .chain(&data.recent)
        .chain(&data.quiet)
        .filter(|row| row.kind == mxr_protocol::MessagesRowKindData::Group)
        .count();
    let asks = data
        .your_turn
        .iter()
        .filter(|row| {
            row.preview
                .as_ref()
                .is_some_and(|p| p.kind == mxr_protocol::MessagesPreviewKindData::Ask)
        })
        .count();
    let closeness: BTreeMap<&str, usize> =
        data.your_turn.iter().fold(BTreeMap::new(), |mut acc, row| {
            *acc.entry(row.closeness.label()).or_default() += 1;
            acc
        });
    println!(
        "  group rows (shown bands) {groups}; your-turn previews from a gist ask {asks}; your-turn closeness {closeness:?}"
    );

    let started = Instant::now();
    let mine = messages::messages_at(
        &state,
        None,
        Some(mxr_protocol::MessagesTurnData::Mine),
        50,
        at,
    )
    .await
    .unwrap();
    println!(
        "ListMessages --turn mine: {:.0} ms ({} rows)",
        ms(started),
        mine.your_turn.len()
    );

    if let Some(row) = data.your_turn.first().or(data.recent.first()) {
        let started = Instant::now();
        let page = messages::person_at(&state, Some(&row.account_id), &row.id, None, at)
            .await
            .unwrap();
        println!(
            "GetPerson (top row): {:.0} ms, {} topics, {} messages shown",
            ms(started),
            page.topics.len(),
            page.conversation.as_ref().map_or(0, |c| c.messages.len())
        );
        if let Some(conversation) = &page.conversation {
            let started = Instant::now();
            let result =
                crate::handler::messages_ack::ack(&state, &conversation.thread_id, true, None)
                    .await;
            println!(
                "AckMessage --dry-run: {:.0} ms ({})",
                ms(started),
                if result.is_ok() {
                    "ok"
                } else {
                    "no one to answer"
                }
            );
        }
    }

    // Thread shapes and quote stripping over every thread with mail in the
    // window, account by account.
    let accounts: Vec<AccountId> = state
        .store
        .list_accounts()
        .await
        .unwrap()
        .into_iter()
        .filter(|a| a.enabled)
        .map(|a| a.id)
        .collect();
    let mut shapes: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut before_clean = 0usize;
    let mut before_raw = 0usize;
    let mut after = 0usize;
    let mut measured = 0usize;
    let mut trimmed_quote = 0usize;
    let mut trimmed_sig = 0usize;
    let mut text_ms = 0f64;
    for account in &accounts {
        let messages = state
            .store
            .desk_thread_messages(account, at - Duration::days(30))
            .await
            .unwrap();
        let senders = Senders::load(&state, account, &messages).await.unwrap();
        let is_self = self_matcher(&state, account).await.unwrap();
        let dismissed = HashMap::new();
        let timers = DeskTimers::default();
        let mut people_threads = Vec::new();
        {
            let inputs = AccountInputs {
                account_id: account,
                messages: &messages,
                contacts: &senders.contacts,
                screener: &senders.screener,
                dismissed: &dismissed,
                timers: &timers,
                is_self: &is_self,
                shape: crate::handler::conversation_shape::shape_config(&state),
                now: at,
            };
            for thread in messages.chunk_by(|a, b| a.thread_id == b.thread_id) {
                let shape = inputs.shape(thread);
                let name = match shape {
                    Shape::OneToOne(_) => "one_to_one",
                    Shape::Group(_) => "group",
                    Shape::Copied => "copied",
                    Shape::NotConversation => "not_a_conversation",
                };
                *shapes.entry(name).or_default() += 1;
                if shape.in_messages() {
                    people_threads.push(thread.to_vec());
                }
            }
        }
        let started = Instant::now();
        for thread in &people_threads {
            let mut plains: Vec<String> = Vec::new();
            for message in thread {
                let Some(body) = state.store.get_body(&message.id).await.unwrap() else {
                    plains.push(String::new());
                    continue;
                };
                let plain = plain_text(body.text_plain.as_deref(), body.text_html.as_deref());
                let earlier: Vec<EarlierMessage<'_>> = plains
                    .iter()
                    .rev()
                    .take(8)
                    .map(|text| EarlierMessage {
                        text,
                        same_author: false,
                    })
                    .collect();
                let old = clean(
                    body.text_plain.as_deref(),
                    body.text_html.as_deref(),
                    &ReaderConfig::default(),
                );
                let new = new_text(
                    body.text_plain.as_deref(),
                    body.text_html.as_deref(),
                    &earlier,
                );
                before_raw += plain.chars().count();
                before_clean += old.content.chars().count();
                after += new.text.chars().count();
                measured += 1;
                trimmed_quote += usize::from(new.trimmed.quote);
                trimmed_sig += usize::from(new.trimmed.signature);
                plains.push(plain);
            }
        }
        text_ms += ms(started);
    }
    println!("Thread shapes (last 30 days): {shapes:?}");
    let pct = |a: usize, b: usize| {
        if b == 0 {
            0.0
        } else {
            100.0 * (1.0 - a as f64 / b as f64)
        }
    };
    println!(
        "New text over {measured} messages in person threads: {before_raw} chars as sent, {before_clean} after today's reader clean, {after} as new text ({:.1}% shorter than as sent, {:.1}% shorter than reader clean); trimmed quote {trimmed_quote}, sig {trimmed_sig}; {:.2} ms per message",
        pct(after, before_raw),
        pct(after, before_clean),
        if measured == 0 { 0.0 } else { text_ms / measured as f64 }
    );
}
