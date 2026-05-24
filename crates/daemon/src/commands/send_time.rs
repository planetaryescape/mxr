use crate::cli::OutputFormat;
use crate::commands::resolve_account;
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_protocol::*;

const WEEKDAY_LABELS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

pub async fn run(
    recipients: Vec<String>,
    account: Option<String>,
    at: Option<String>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_account(&mut client, account.as_deref()).await?;
    let proposed_at = at
        .as_deref()
        .map(|s| {
            mxr_core::parse_relative_time(s, chrono::Utc::now()).map_err(|e| {
                anyhow::anyhow!(
                    "Cannot parse --at value '{s}': {e}. Try: `fri 19:00`, `tomorrow 9am`, `in 2h`, or RFC3339."
                )
            })
        })
        .transpose()?;
    let resp = client
        .request(Request::SendTimeRecommendation {
            account_id,
            recipients: normalize_recipients(recipients)?,
            proposed_at,
        })
        .await?;
    print(resp, resolve_format(format))
}

fn normalize_recipients(values: Vec<String>) -> anyhow::Result<Vec<String>> {
    let recipients: Vec<String> = values
        .into_iter()
        .flat_map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect();
    if recipients.is_empty() {
        anyhow::bail!("at least one recipient is required");
    }
    Ok(recipients)
}

fn print(resp: Response, fmt: OutputFormat) -> anyhow::Result<()> {
    match resp {
        Response::Ok {
            data: ResponseData::SendTimeRecommendationResponse { recommendation },
        } => match fmt {
            OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&recommendation)?),
            OutputFormat::Jsonl => println!("{}", serde_json::to_string(&recommendation)?),
            _ => {
                println!("Confidence: {:?}", recommendation.confidence);
                if recommendation.proposed_at.is_some() {
                    let wd = recommendation
                        .proposed_weekday
                        .and_then(|w| WEEKDAY_LABELS.get(w as usize).copied())
                        .unwrap_or("?");
                    let hr = recommendation
                        .proposed_hour
                        .map_or_else(|| "??:??".into(), |h| format!("{h:02}:00"));
                    println!("Proposed slot:  {wd} {hr}");
                }
                println!();
                for row in &recommendation.recipient_rows {
                    println!("Recipient: {} (samples: {})", row.email, row.sample_count);
                    if let Some(best) = row.best_expected_reply_seconds {
                        if let Some(window) = row.best_windows.first() {
                            println!(
                                "  Fastest: {} {:02}:00-{:02}:00 (p50 = {})",
                                WEEKDAY_LABELS.get(window.weekday as usize).unwrap_or(&"?"),
                                window.hour_start,
                                window.hour_end,
                                humanize(best)
                            );
                        }
                    }
                    if recommendation.proposed_at.is_some() {
                        match row.proposed_expected_reply_seconds {
                            Some(p50) => println!("  Proposed p50: {}", humanize(p50)),
                            None => println!("  Proposed p50: no historical data"),
                        }
                    }
                }
            }
        },
        Response::Error { message, .. } => anyhow::bail!(message),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}

fn humanize(secs: i64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86_400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86_400)
    }
}
