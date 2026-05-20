use crate::cli::OutputFormat;
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};
use chrono::{Datelike, TimeZone, Utc};
use mxr_core::id::AccountId;
use mxr_core::types::{WrappedReplyExtreme, WrappedSummary};
use mxr_protocol::{Request, Response, ResponseData};
use std::str::FromStr;

pub async fn run(
    ytd: bool,
    year: Option<i32>,
    since_days: Option<u32>,
    account: Option<String>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let account_id = account
        .as_deref()
        .map(AccountId::from_str)
        .transpose()
        .map_err(|e| anyhow::anyhow!("invalid --account id: {e}"))?;

    let now = Utc::now();
    let (since_unix, until_unix, label) = match (year, since_days, ytd) {
        (Some(y), _, _) => {
            let start = Utc
                .with_ymd_and_hms(y, 1, 1, 0, 0, 0)
                .single()
                .ok_or_else(|| anyhow::anyhow!("invalid year: {y}"))?;
            let end = Utc
                .with_ymd_and_hms(y, 12, 31, 23, 59, 59)
                .single()
                .ok_or_else(|| anyhow::anyhow!("invalid year: {y}"))?;
            (start.timestamp(), end.timestamp(), format!("{y}"))
        }
        (_, Some(d), _) => {
            let start = now - chrono::Duration::days(d as i64);
            (start.timestamp(), now.timestamp(), format!("last {d} days"))
        }
        // Default and `--ytd`: Jan 1 of current year → now.
        _ => {
            let start = Utc
                .with_ymd_and_hms(now.year(), 1, 1, 0, 0, 0)
                .single()
                .ok_or_else(|| anyhow::anyhow!("invalid current year"))?;
            (
                start.timestamp(),
                now.timestamp(),
                format!("{} year-to-date", now.year()),
            )
        }
    };

    let mut client = IpcClient::connect().await?;
    let resp = client
        .request(Request::Wrapped {
            account_id,
            since_unix,
            until_unix,
            label: label.clone(),
        })
        .await?;
    let summary = crate::commands::expect_response(resp, |r| match r {
        Response::Ok {
            data: ResponseData::Wrapped { summary },
        } => Some(summary),
        _ => None,
    })?;

    match resolve_format(format) {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&summary)?),
        OutputFormat::Jsonl => println!("{}", jsonl(&[summary])?),
        OutputFormat::Csv | OutputFormat::Ids | OutputFormat::Table => {
            render_narrative(&summary);
        }
    }
    Ok(())
}

fn render_narrative(s: &WrappedSummary) {
    println!();
    println!("    mxr wrapped — {}", s.label);
    println!("    {}", "═".repeat(40));
    println!();

    // Volume
    println!("📬  Volume");
    println!(
        "    You sent {}. You received {}.",
        s.volume.outbound_count, s.volume.inbound_count,
    );
    println!("    Across {} unique threads.", s.volume.thread_count);
    println!();

    // Time patterns
    if let Some(dow) = s.time_patterns.busiest_day_of_week.as_deref() {
        println!("⏰  When you do email");
        println!(
            "    Busiest weekday: {} ({} messages).",
            dow, s.time_patterns.busiest_day_of_week_count,
        );
        if let Some(h) = s.time_patterns.busiest_hour_utc {
            println!(
                "    Most active hour: {h:02}:00 UTC ({} messages).",
                s.time_patterns.busiest_hour_count,
            );
        }
        if let Some(d) = s.time_patterns.busiest_date {
            println!(
                "    Busiest single day: {} ({} messages — what happened?).",
                d.format("%a %b %-d, %Y"),
                s.time_patterns.busiest_date_count,
            );
        }
        println!();
    }

    // Top contacts
    if !s.top_contacts.most_emailed_to_me.is_empty()
        || !s.top_contacts.most_emailed_by_me.is_empty()
    {
        println!("👥  Top contacts");
        if let Some(top) = s.top_contacts.most_emailed_to_me.first() {
            println!(
                "    Emailed you most: {} ({} messages)",
                top.email, top.count
            );
        }
        if let Some(top) = s.top_contacts.most_emailed_by_me.first() {
            println!(
                "    You emailed most:  {} ({} messages)",
                top.email, top.count
            );
        }
        if let Some(top) = s.top_contacts.most_asymmetric.first() {
            println!(
                "    Most one-sided:    {} ({} in / {} out, {:.0}% gap)",
                top.email,
                top.total_inbound,
                top.total_outbound,
                top.asymmetry * 100.0,
            );
        }
        println!();
    }

    // Reply discipline
    if let Some(rd) = &s.reply_discipline {
        println!("⏱   Reply discipline");
        println!("    Sample size: {} replies you sent.", rd.sample_count);
        println!(
            "    Median reply time (clock):     {}",
            humanize_duration(rd.clock_p50_seconds)
        );
        println!(
            "    P90 (clock):                   {}",
            humanize_duration(rd.clock_p90_seconds)
        );
        if let Some(b50) = rd.business_hours_p50_seconds {
            println!(
                "    Median (business-hours only):  {}",
                humanize_duration(b50)
            );
        }
        if let Some(b90) = rd.business_hours_p90_seconds {
            println!(
                "    P90 (business-hours only):     {}",
                humanize_duration(b90)
            );
        }
        if let Some(f) = &rd.fastest {
            println!(
                "    Fastest reply:  {} → {} ⚡",
                humanize_duration(f.latency_seconds),
                f.counterparty_email,
            );
        }
        if let Some(s) = &rd.slowest {
            print_slowest(s);
        }
        println!();
    } else {
        println!("⏱   Reply discipline");
        println!("    No reply pairs in this window. Run `mxr doctor --rebuild-analytics`");
        println!("    to backfill reply pairs from existing messages.");
        println!();
    }

    // Storage
    println!("📎  Storage");
    println!(
        "    {} of mail in this window.",
        humanize_bytes(s.storage.total_bytes)
    );
    if let Some(mime) = &s.storage.top_mimetype {
        println!(
            "    Top file type: {} ({})",
            mime.key,
            humanize_bytes(mime.bytes)
        );
    }
    if let Some(h) = &s.storage.heaviest_message {
        let subject: String = h.subject.chars().take(40).collect();
        println!(
            "    Heaviest single message: {} from {} — \"{}\"",
            humanize_bytes(h.size_bytes),
            h.from_email,
            subject,
        );
    }
    println!();

    // Newsletters
    println!("📰  Newsletters");
    println!("    {} unique mailing lists.", s.newsletters.unique_lists);
    if let Some(top) = &s.newsletters.top_list {
        let open_pct = if top.message_count > 0 {
            (top.opened_count as f64 / top.message_count as f64) * 100.0
        } else {
            0.0
        };
        println!(
            "    Most-prolific list: {} ({} messages, {open_pct:.0}% opened)",
            top.list_id, top.message_count,
        );
    }
    println!(
        "    {:.0}% of inbound came from a mailing list.",
        s.newsletters.list_share_of_inbound_pct
    );
    println!();

    // Superlatives
    println!("🏆  Superlatives");
    if let Some(t) = &s.superlatives.longest_thread {
        let subject: String = t.subject.chars().take(50).collect();
        println!(
            "    Longest thread: {} messages — \"{}\"",
            t.message_count, subject
        );
    }
    if let Some(g) = &s.superlatives.most_ghosted {
        println!(
            "    Most-ghosted:   {} sent you {} messages, you replied 0 times 🫠",
            g.email, g.inbound_count,
        );
    }
    println!();
    println!(
        "    Window: {} → {}",
        s.window_start.format("%Y-%m-%d"),
        s.window_end.format("%Y-%m-%d"),
    );
    println!();
}

fn print_slowest(slowest: &WrappedReplyExtreme) {
    let suffix = if slowest.latency_seconds > 7 * 86_400 {
        " 🫠"
    } else {
        ""
    };
    println!(
        "    Slowest reply (capped at 30d): {} → {}{suffix}",
        humanize_duration(slowest.latency_seconds),
        slowest.counterparty_email,
    );
}

fn humanize_duration(seconds: u32) -> String {
    if seconds == 0 {
        return "0s".into();
    }
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let secs = seconds % 60;
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {secs}s")
    } else {
        format!("{secs}s")
    }
}

fn humanize_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut idx = 0;
    while value >= 1024.0 && idx + 1 < UNITS.len() {
        value /= 1024.0;
        idx += 1;
    }
    if idx == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[idx])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn humanize_duration_steps_through_units() {
        assert_eq!(humanize_duration(0), "0s");
        assert_eq!(humanize_duration(45), "45s");
        assert_eq!(humanize_duration(3 * 60 + 4), "3m 4s");
        assert_eq!(humanize_duration(2 * 3600 + 15 * 60), "2h 15m");
        assert_eq!(humanize_duration(3 * 86400 + 2 * 3600), "3d 2h");
    }

    #[test]
    fn humanize_bytes_steps_through_units() {
        assert_eq!(humanize_bytes(0), "0 B");
        assert_eq!(humanize_bytes(1023), "1023 B");
        assert_eq!(humanize_bytes(1024), "1.0 KB");
        assert_eq!(humanize_bytes(1024 * 1024 * 47), "47.0 MB");
    }
}
