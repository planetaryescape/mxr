//! `mxr time`: preview how a time phrase resolves, and the shared parser for
//! every CLI flag that takes a time (`--until`, `--at`, `--when`,
//! `--remind-after`).

use chrono::{DateTime, Local, Utc};
use mxr_core::natural_time::{resolve_time, TimeResolution, TimeResolveError};
use mxr_protocol::{Request, Response, ResponseData};

use crate::cli::OutputFormat;
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;

/// Resolve a time flag in local time with the user's snooze hours, the same
/// way the daemon's `ResolveTime` does. Ambiguous phrases take the default
/// choice that `mxr time` shows first.
pub(crate) fn parse_time_arg(input: &str, now: DateTime<Utc>) -> anyhow::Result<DateTime<Utc>> {
    resolve_time_arg(input, now).map(|resolution| resolution.at)
}

/// Like [`parse_time_arg`], keeping the resolution for callers that echo
/// the time back ("Reminder set for Friday 3 October, 15:00").
pub(crate) fn resolve_time_arg(input: &str, now: DateTime<Utc>) -> anyhow::Result<TimeResolution> {
    let prefs = mxr_config::load_config()
        .unwrap_or_default()
        .snooze
        .time_prefs();
    resolve_time(input, &now.with_timezone(&Local), &prefs).map_err(|error| {
        anyhow::anyhow!("Cannot parse '{input}': {error} Preview with `mxr time \"{input}\"`.")
    })
}

pub async fn run(
    input: String,
    now: Option<DateTime<Utc>>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let response = client
        .request(Request::ResolveTime {
            input,
            now,
            // The CLI runs next to the daemon, so the daemon's zone is ours.
            time_zone: None,
        })
        .await?;
    let (input, resolution, error) = match response {
        Response::Ok {
            data:
                ResponseData::ResolvedTime {
                    input,
                    resolution,
                    error,
                },
        } => (input, resolution, error),
        Response::Error { message, .. } => anyhow::bail!(message),
        _ => anyhow::bail!("Unexpected response"),
    };
    match resolve_format(format) {
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&payload(&input, &resolution, &error))?
        ),
        OutputFormat::Jsonl => println!(
            "{}",
            serde_json::to_string(&payload(&input, &resolution, &error))?
        ),
        _ => {
            if let Some(resolution) = &resolution {
                print!("{}", human(resolution));
            }
        }
    }
    match error {
        Some(error) => anyhow::bail!(error.message),
        None => Ok(()),
    }
}

fn payload(
    input: &str,
    resolution: &Option<TimeResolution>,
    error: &Option<TimeResolveError>,
) -> serde_json::Value {
    serde_json::json!({
        "input": input,
        "resolution": resolution,
        "error": error,
    })
}

fn human(resolution: &TimeResolution) -> String {
    let mut out = String::new();
    for (index, choice) in resolution.choices.iter().enumerate() {
        if index == 0 {
            out.push_str(&choice.summary());
            out.push('\n');
            let assumed = choice.assumed();
            if !assumed.is_empty() {
                out.push_str(&format!("  Assumed: {assumed}\n"));
            }
        } else {
            out.push_str(&format!("  Or: {}\n", choice.summary()));
        }
        if let Some(note) = &choice.note {
            out.push_str(&format!("  Note: {note}\n"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, TimeZone};
    use mxr_core::natural_time::TimePrefs;

    #[test]
    fn human_output_lists_the_default_then_the_other_choices() {
        let tz = FixedOffset::east_opt(3600).expect("valid offset");
        let now = tz
            .with_ymd_and_hms(2024, 5, 7, 14, 0, 0)
            .single()
            .expect("valid time");
        let resolution =
            resolve_time("fri 3", &now, &TimePrefs::default()).expect("fri 3 resolves");
        assert_eq!(
            human(&resolution),
            "Friday 10 May, 15:00 (in 3 days)\n  Assumed: am or pm\n  Or: Friday 10 May, 03:00 (in 3 days)\n"
        );
    }
}
