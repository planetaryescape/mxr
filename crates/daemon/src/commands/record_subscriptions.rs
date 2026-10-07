//! `mxr records subscriptions`: what recurs, what it costs, and what
//! changed. The daemon works it out from the records; this prints it.

use crate::cli::OutputFormat;
use crate::output::{jsonl, print_json, terminal_block};
use mxr_protocol::{RecordSubscriptionData, RecordSubscriptionsData};
use std::fmt::Write as _;

pub fn print(data: &RecordSubscriptionsData, format: OutputFormat) -> anyhow::Result<()> {
    match format {
        OutputFormat::Json => print_json(data, format),
        OutputFormat::Jsonl => {
            println!("{}", jsonl(&data.subscriptions)?);
            Ok(())
        }
        OutputFormat::Ids => {
            data.subscriptions.iter().for_each(|s| println!("{}", s.id));
            Ok(())
        }
        _ => {
            print!("{}", terminal_block(&text(data)));
            Ok(())
        }
    }
}

fn day(at: chrono::DateTime<chrono::Utc>) -> String {
    at.format("%-d %b %Y").to_string()
}

fn row(subscription: &RecordSubscriptionData) -> String {
    let amount = subscription
        .amount
        .as_ref()
        .map(|a| {
            let unchecked = subscription
                .fields
                .iter()
                .any(|f| f.field == "amount" && !f.checked);
            format!("{}{}", a.display, if unchecked { " ?" } else { "" })
        })
        .unwrap_or_default();
    let when = match (subscription.status.as_str(), subscription.next_expected) {
        ("ended", _) | (_, None) => format!("last {}", day(subscription.last_charge)),
        ("overdue", Some(next)) => format!("overdue since {}", day(next)),
        (_, Some(next)) => format!("next {}", day(next)),
    };
    let mut line = format!(
        "  {:<32} {:>12}  {:<9} {:<24} {}\n",
        mxr_todo::text::clip(&subscription.title, 32),
        amount,
        subscription.cadence,
        when,
        subscription.id,
    );
    for change in &subscription.price_changes {
        let _ = writeln!(line, "      price {}", change.label);
    }
    if !subscription.confirmed {
        let _ = writeln!(line, "      seen twice; unconfirmed until the next charge");
    }
    line
}

fn text(data: &RecordSubscriptionsData) -> String {
    let mut out = format!("{}\n", data.header);
    if let Some(empty) = &data.empty_state {
        let _ = writeln!(out, "\n{empty}");
    }
    if !data.totals.is_empty() {
        let totals: Vec<String> = data
            .totals
            .iter()
            .map(|t| {
                format!(
                    "{} a month, {} a year",
                    t.per_month.display, t.per_year.display
                )
            })
            .collect();
        let _ = writeln!(out, "\n{} live: {}", data.live, totals.join(" + "));
    }
    for signal in &data.signals {
        let _ = writeln!(out, "  ! {}", signal.label);
    }
    for (label, live) in [("LIVE", true), ("ENDED", false)] {
        let rows: Vec<&RecordSubscriptionData> = data
            .subscriptions
            .iter()
            .filter(|s| (s.status != "ended") == live)
            .collect();
        if rows.is_empty() {
            continue;
        }
        let _ = writeln!(out, "\n{label}");
        for subscription in rows {
            out.push_str(&row(subscription));
        }
    }
    out
}
