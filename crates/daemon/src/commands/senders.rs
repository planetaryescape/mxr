use crate::cli::OutputFormat;
use crate::commands::resolve_optional_account;
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};
use mxr_protocol::{Request, Response, ResponseData, SenderSummaryData};

fn display_name(sender: &SenderSummaryData) -> &str {
    sender
        .display_name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(sender.sender_email.as_str())
}

fn render_table(senders: &[SenderSummaryData]) {
    if senders.is_empty() {
        println!("No senders found.");
        return;
    }

    println!(
        "{:<32} {:<34} {:>6} {:>6} {:<32}",
        "SENDER", "EMAIL", "COUNT", "UNREAD", "LATEST SUBJECT"
    );
    println!("{}", "-".repeat(116));
    for sender in senders {
        let subject: String = sender.latest_subject.chars().take(32).collect();
        println!(
            "{:<32} {:<34} {:>6} {:>6} {:<32}",
            display_name(sender),
            sender.sender_email,
            sender.message_count,
            sender.unread_count,
            subject
        );
    }
}

pub async fn run(
    top: u32,
    account: Option<String>,
    since: Option<String>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    let since_unix = since
        .as_deref()
        .map(parse_since)
        .transpose()
        .map_err(|err| {
            anyhow::anyhow!(
                "invalid --since `{}`: {err}",
                since.as_deref().unwrap_or("")
            )
        })?
        .map(|dt| dt.timestamp());
    let resp = client
        .request(Request::ListSenders {
            account_id,
            limit: top,
            since_unix,
        })
        .await?;
    let senders = crate::commands::expect_response(resp, |r| match r {
        Response::Ok {
            data: ResponseData::Senders { senders },
        } => Some(senders),
        _ => None,
    })?;

    match resolve_format(format) {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&senders)?),
        OutputFormat::Jsonl => println!("{}", jsonl(&senders)?),
        _ => render_table(&senders),
    }

    Ok(())
}

/// Parse a `--since` argument. Accepts:
/// - shorthand durations: `7d`, `4w`, `12h`, `90d` — interpreted as
///   "now minus this duration"
/// - absolute RFC-3339 timestamps: `2026-02-01T00:00:00Z`
///
/// Returns the resolved cutoff timestamp. Negative / zero durations
/// are rejected — the user almost certainly means "the last N days",
/// never "in the future".
pub(crate) fn parse_since(input: &str) -> Result<chrono::DateTime<chrono::Utc>, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("empty --since value".into());
    }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(trimmed) {
        return Ok(dt.with_timezone(&chrono::Utc));
    }
    let last = trimmed.chars().last().ok_or("empty")?;
    let (unit_seconds, descriptor) = match last {
        'd' | 'D' => (60 * 60 * 24, "days"),
        'w' | 'W' => (60 * 60 * 24 * 7, "weeks"),
        'h' | 'H' => (60 * 60, "hours"),
        'm' | 'M' => (60, "minutes"),
        _ => {
            return Err(format!(
                "unrecognized suffix `{last}`; expected one of d/w/h/m or an RFC-3339 timestamp"
            ))
        }
    };
    let amount: i64 = trimmed[..trimmed.len() - last.len_utf8()]
        .trim()
        .parse()
        .map_err(|_| format!("could not parse the {descriptor} component of `{trimmed}`"))?;
    if amount <= 0 {
        return Err("--since must be a positive duration".into());
    }
    Ok(chrono::Utc::now() - chrono::Duration::seconds(amount * unit_seconds))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    /// Phase 2.6: shorthand `7d` is "last 7 days".
    #[test]
    fn parse_since_accepts_day_shorthand() {
        let before = Utc::now() - Duration::days(7);
        let parsed = parse_since("7d").unwrap();
        let after = Utc::now() - Duration::days(7);
        assert!(parsed >= before - Duration::seconds(1));
        assert!(parsed <= after + Duration::seconds(1));
    }

    #[test]
    fn parse_since_accepts_week_shorthand() {
        let approx = Utc::now() - Duration::weeks(4);
        let parsed = parse_since("4w").unwrap();
        let diff = (parsed - approx).num_seconds().abs();
        assert!(diff < 2, "4w should resolve to ~28 days ago");
    }

    /// Phase 2.6: absolute RFC-3339 is the unambiguous form for
    /// scripts and CI.
    #[test]
    fn parse_since_accepts_rfc3339_absolute_timestamp() {
        let parsed = parse_since("2026-02-01T00:00:00Z").unwrap();
        assert_eq!(parsed.to_rfc3339(), "2026-02-01T00:00:00+00:00");
    }

    /// Zero/negative durations are user error, not "all time"; reject
    /// so the user sees the mistake rather than silently getting
    /// nothing back.
    #[test]
    fn parse_since_rejects_zero_or_negative_durations() {
        assert!(parse_since("0d").is_err());
        assert!(parse_since("-5d").is_err());
    }

    /// Unknown suffix is rejected with a useful error rather than
    /// being silently treated as "no filter".
    #[test]
    fn parse_since_rejects_unknown_suffix() {
        let err = parse_since("90q").unwrap_err();
        assert!(err.contains("suffix"));
    }
}
