use crate::cli::{CadenceAction, OutputFormat};
use crate::commands::resolve_account;
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_protocol::*;

pub async fn run(action: CadenceAction) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    match action {
        CadenceAction::Watch {
            email,
            account,
            expected_days,
            every,
            note,
            allow_list_sender,
        } => {
            let expected_days = resolve_expected_days(expected_days, every.as_deref())?;
            let account_id = resolve_account(&mut client, account.as_deref()).await?;
            let resp = client
                .request(Request::WatchCadence {
                    account_id,
                    email,
                    expected_days,
                    note,
                    allow_list_sender,
                })
                .await?;
            ack(resp, "watching")
        }
        CadenceAction::Unwatch { email, account } => {
            let account_id = resolve_account(&mut client, account.as_deref()).await?;
            let resp = client
                .request(Request::UnwatchCadence { account_id, email })
                .await?;
            ack(resp, "unwatched")
        }
        CadenceAction::List { account, format } => {
            let account_id = resolve_account(&mut client, account.as_deref()).await?;
            let resp = client
                .request(Request::ListCadenceWatch { account_id })
                .await?;
            print_list(resp, resolve_format(format))
        }
        CadenceAction::Drift { account, format } => {
            let account_id = resolve_account(&mut client, account.as_deref()).await?;
            let resp = client
                .request(Request::ListCadenceDrift { account_id })
                .await?;
            print_drift(resp, resolve_format(format))
        }
    }
}

fn ack(resp: Response, label: &str) -> anyhow::Result<()> {
    match resp {
        Response::Ok { .. } => {
            println!("{label}");
            Ok(())
        }
        Response::Error { message, .. } => anyhow::bail!(message),
    }
}

fn print_list(resp: Response, fmt: OutputFormat) -> anyhow::Result<()> {
    match resp {
        Response::Ok {
            data: ResponseData::CadenceWatchList { entries },
        } => match fmt {
            OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&entries)?),
            OutputFormat::Jsonl => {
                for e in entries {
                    println!("{}", serde_json::to_string(&e)?);
                }
            }
            _ => {
                for e in entries {
                    let exp = e
                        .expected_days
                        .map(|d| format!("{d:.1}d"))
                        .unwrap_or_else(|| "auto".into());
                    println!(
                        "{:<32}  expected={}  added={}",
                        e.email,
                        exp,
                        e.added_at.format("%Y-%m-%d")
                    );
                }
            }
        },
        Response::Error { message, .. } => anyhow::bail!(message),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}

fn resolve_expected_days(
    expected_days: Option<f64>,
    every: Option<&str>,
) -> anyhow::Result<Option<f64>> {
    match (expected_days, every) {
        (Some(_), Some(_)) => anyhow::bail!("use either --expected-days or --every, not both"),
        (Some(days), None) => Ok(Some(days)),
        (None, Some(value)) => parse_every_days(value).map(Some),
        (None, None) => Ok(None),
    }
}

fn parse_every_days(value: &str) -> anyhow::Result<f64> {
    let trimmed = value.trim().to_ascii_lowercase();
    if trimmed.is_empty() {
        anyhow::bail!("--every cannot be empty");
    }
    let split_at = trimmed
        .char_indices()
        .find_map(|(idx, ch)| {
            if !(ch.is_ascii_digit() || ch == '.') {
                Some(idx)
            } else {
                None
            }
        })
        .unwrap_or(trimmed.len());
    let (number, unit) = trimmed.split_at(split_at);
    let count: f64 = number
        .trim()
        .parse()
        .map_err(|_| anyhow::anyhow!("--every must start with a number, e.g. 14d"))?;
    if count <= 0.0 {
        anyhow::bail!("--every must be positive");
    }
    let unit = unit.trim();
    match unit {
        "" | "d" | "day" | "days" => Ok(count),
        "w" | "week" | "weeks" => Ok(count * 7.0),
        _ => anyhow::bail!("unsupported --every unit `{unit}`; use days or weeks"),
    }
}

fn print_drift(resp: Response, fmt: OutputFormat) -> anyhow::Result<()> {
    match resp {
        Response::Ok {
            data: ResponseData::CadenceDriftList { rows },
        } => match fmt {
            OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&rows)?),
            OutputFormat::Jsonl => {
                for r in rows {
                    println!("{}", serde_json::to_string(&r)?);
                }
            }
            _ => {
                println!("{:<32}  {:>8}  {:>8}", "email", "drift", "expected");
                for r in rows {
                    println!(
                        "{:<32}  {:>6.1}d  {:>6.1}d",
                        r.email, r.drift_days, r.expected_days
                    );
                }
            }
        },
        Response::Error { message, .. } => anyhow::bail!(message),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_every_days_accepts_days_and_weeks() {
        assert_eq!(parse_every_days("14d").unwrap(), 14.0);
        assert_eq!(parse_every_days("2w").unwrap(), 14.0);
        assert_eq!(parse_every_days("30 days").unwrap(), 30.0);
    }
}
