//! `mxr sender <addr>` — relationship aggregates for one contact.

use crate::cli::OutputFormat;
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_core::types::ResponseTimeBucket;
use mxr_protocol::*;

fn activity_sparkline(weeks: &[SenderWeeklyActivityData]) -> String {
    const STEPS: &[u8] = b" .:-=+*#";
    let totals: Vec<u32> = weeks
        .iter()
        .map(|week| week.inbound_count + week.outbound_count)
        .collect();
    let max = totals.iter().copied().max().unwrap_or(0);
    if max == 0 {
        return ".".repeat(weeks.len());
    }
    totals
        .into_iter()
        .map(|count| {
            let index = ((count as usize) * (STEPS.len() - 1)) / (max as usize);
            STEPS[index] as char
        })
        .collect()
}

fn histogram_label(upper_bound_seconds: u32) -> &'static str {
    match upper_bound_seconds {
        60 => "<1m",
        300 => "<5m",
        1_800 => "<30m",
        3_600 => "<1h",
        21_600 => "<6h",
        86_400 => "<1d",
        259_200 => "<3d",
        u32::MAX => ">=3d",
        _ => "?",
    }
}

fn histogram_summary(buckets: &[ResponseTimeBucket]) -> String {
    buckets
        .iter()
        .filter(|bucket| bucket.count > 0)
        .map(|bucket| {
            format!(
                "{}:{}",
                histogram_label(bucket.upper_bound_seconds),
                bucket.count
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub async fn run(
    email: String,
    account: Option<String>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;

    // Resolve the account id (CLI accepts the configured `key` or
    // falls back to the only/default account if not specified).
    let account_id = resolve_account(&mut client, account.as_deref()).await?;

    let resp = client
        .request(Request::GetSenderProfile {
            account_id,
            email: email.clone(),
        })
        .await?;

    let fmt = resolve_format(format);
    match resp {
        Response::Ok {
            data: ResponseData::SenderProfile { profile },
        } => match (fmt, profile) {
            (OutputFormat::Json, p) => {
                println!("{}", serde_json::to_string_pretty(&p)?);
            }
            (OutputFormat::Jsonl, p) => {
                println!("{}", serde_json::to_string(&p)?);
            }
            (_, None) => {
                println!("No history with {email}");
            }
            (_, Some(p)) => {
                let name = p.display_name.as_deref().unwrap_or(p.email.as_str());
                println!("{name} <{}>", p.email);
                if p.is_list_sender {
                    println!(
                        "  list-sender: {}",
                        p.list_id.as_deref().unwrap_or("(unidentified list)")
                    );
                }
                println!(
                    "  volume:   {} in, {} out",
                    p.total_inbound, p.total_outbound
                );
                println!("  replied:  {} times", p.replied_count);
                if let Some(cadence) = p.cadence_days_p50 {
                    println!("  cadence:  {:.1} days (p50)", cadence);
                }
                if let Some(last_in) = p.last_inbound_at {
                    println!(
                        "  last in:  {}",
                        last_in
                            .with_timezone(&chrono::Local)
                            .format("%a %b %e %H:%M")
                    );
                }
                if let Some(last_out) = p.last_outbound_at {
                    println!(
                        "  last out: {}",
                        last_out
                            .with_timezone(&chrono::Local)
                            .format("%a %b %e %H:%M")
                    );
                }
                if p.open_thread_count > 0 {
                    println!(
                        "  open:     {} thread(s) waiting on you",
                        p.open_thread_count
                    );
                }
                if let Some(question) = p.unanswered_question {
                    println!(
                        "  question: unanswered for {}d — {}",
                        question.days_waiting, question.subject
                    );
                }
                let histogram = histogram_summary(&p.response_histogram);
                if !histogram.is_empty() {
                    println!("  replies:  {histogram}");
                }
                if !p.weekly_activity.is_empty() {
                    println!("  weeks:    {}", activity_sparkline(&p.weekly_activity));
                }
            }
        },
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    }

    Ok(())
}

async fn resolve_account(
    client: &mut IpcClient,
    explicit: Option<&str>,
) -> anyhow::Result<mxr_core::AccountId> {
    let resp = client.request(Request::ListAccounts).await?;
    let accounts = match resp {
        Response::Ok {
            data: ResponseData::Accounts { accounts },
        } => accounts,
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    };

    if let Some(key) = explicit {
        return accounts
            .into_iter()
            .find(|a| a.key.as_deref() == Some(key) || a.email == key)
            .map(|a| a.account_id)
            .ok_or_else(|| anyhow::anyhow!("No account matching '{key}'"));
    }

    if let [account] = accounts.as_slice() {
        return Ok(account.account_id.clone());
    }
    if let Some(default) = accounts.iter().find(|a| a.is_default) {
        return Ok(default.account_id.clone());
    }
    anyhow::bail!("Multiple accounts configured; pass --account <key>")
}
