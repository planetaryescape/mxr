//! Promises in mail you send: `mxr commitments detect` / `add`, and the
//! note `mxr send`, `compose --yes`, `reply --yes` and `forward --yes` print
//! after a send when the message promises something.

use crate::cli::OutputFormat;
use crate::commands::expect_response;
use crate::commands::selection::parse_message_id;
use crate::commands::time::resolve_time_arg;
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_core::MessageId;
use mxr_protocol::{
    CommitmentData, PromiseDetectionData, PromiseDetectionStatusData, PromiseSourceData, Request,
    Response, ResponseData,
};
use std::io::IsTerminal;

pub(crate) async fn detect(
    client: &mut IpcClient,
    message_id: &str,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let message_id = parse_message_id(message_id)?;
    let detection = request_detection(
        client,
        PromiseSourceData::SentMessage {
            message_id: message_id.clone(),
        },
    )
    .await?;
    match resolve_format(format) {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&detection)?),
        OutputFormat::Jsonl => println!("{}", serde_json::to_string(&detection)?),
        _ => {
            if detection.status != PromiseDetectionStatusData::Ready {
                println!(
                    "No promise check: {}",
                    detection
                        .message
                        .as_deref()
                        .unwrap_or("the model is unavailable")
                );
            } else if detection.promises.is_empty() {
                println!("No promises found.");
            } else {
                print!("{}", promise_lines(&detection, &message_id));
            }
        }
    }
    Ok(())
}

pub(crate) async fn add(
    client: &mut IpcClient,
    message_id: &str,
    what: String,
    due: &str,
    dry_run: bool,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let message_id = parse_message_id(message_id)?;
    let due = resolve_time_arg(due, chrono::Utc::now())?;
    let response = client
        .request(Request::RecordPromise {
            message_id,
            what,
            due_at: due.at,
            dry_run,
        })
        .await?;
    let (commitment, dry_run) = expect_response(response, |response| match response {
        Response::Ok {
            data:
                ResponseData::RecordedPromise {
                    commitment,
                    dry_run,
                },
        } => Some((commitment, dry_run)),
        _ => None,
    })?;
    match resolve_format(format) {
        format @ (OutputFormat::Json | OutputFormat::Jsonl) => {
            let payload = serde_json::json!({ "dry_run": dry_run, "commitment": commitment });
            if format == OutputFormat::Json {
                println!("{}", serde_json::to_string_pretty(&payload)?);
            } else {
                println!("{}", serde_json::to_string(&payload)?);
            }
        }
        OutputFormat::Ids => println!("{}", commitment.id),
        _ => println!("{}", added_line(&commitment, &due.description, dry_run)),
    }
    Ok(())
}

fn added_line(commitment: &CommitmentData, due: &str, dry_run: bool) -> String {
    if dry_run {
        format!(
            "Would remind you on {due}: {} (to {}). Nothing was stored.",
            commitment.what, commitment.email
        )
    } else {
        format!(
            "Reminder set for {due}: {} (to {}). Commitment {}",
            commitment.what, commitment.email, commitment.id
        )
    }
}

/// After a send: say what the message promised and how to keep each
/// promise. Never fails the command, since the mail already went out. The
/// note is for a person: it goes to stdout only for table output on a
/// terminal, and to stderr otherwise, so piped or machine-format stdout
/// stays exactly what scripts parse.
pub(crate) async fn report_after_send(
    client: &mut IpcClient,
    sent_message_id: &MessageId,
    format: Option<OutputFormat>,
) {
    let source = PromiseSourceData::SentMessage {
        message_id: sent_message_id.clone(),
    };
    // The daemon's own budget starts at the model call, after any wait for
    // a model slot; the command must not hang behind that. On time or not,
    // the mail already went out, so this only ever skips the note.
    let Some(detection) =
        within_deadline(AFTER_SEND_DEADLINE, request_detection(client, source)).await
    else {
        eprintln!("No promise check this time: the model didn't answer in time.");
        return;
    };
    let detection = match detection {
        Ok(detection) => detection,
        Err(error) => {
            tracing::debug!(%error, "promise check after send failed");
            return;
        }
    };
    if detection.status != PromiseDetectionStatusData::Ready || detection.promises.is_empty() {
        return;
    }
    let lines = promise_lines(&detection, sent_message_id);
    if note_on_stdout(format.as_ref(), std::io::stdout().is_terminal()) {
        print!("{lines}");
    } else {
        eprint!("{lines}");
    }
}

/// The whole promise check after a send, model slot wait included.
const AFTER_SEND_DEADLINE: std::time::Duration = std::time::Duration::from_secs(10);

async fn within_deadline<T>(
    deadline: std::time::Duration,
    work: impl std::future::Future<Output = T>,
) -> Option<T> {
    tokio::time::timeout(deadline, work).await.ok()
}

fn note_on_stdout(format: Option<&OutputFormat>, stdout_is_terminal: bool) -> bool {
    stdout_is_terminal && matches!(format, None | Some(OutputFormat::Table))
}

async fn request_detection(
    client: &mut IpcClient,
    source: PromiseSourceData,
) -> anyhow::Result<PromiseDetectionData> {
    let response = client
        .request(Request::DetectPromises {
            source,
            now: None,
            // The CLI runs next to the daemon, so the daemon's zone is ours.
            time_zone: None,
        })
        .await?;
    expect_response(response, |response| match response {
        Response::Ok {
            data: ResponseData::Promises { detection },
        } => Some(detection),
        _ => None,
    })
}

/// "You promised: send the deck, due Friday 2 October, 09:00 ("by Friday")."
/// followed by the exact command that keeps it.
fn promise_lines(detection: &PromiseDetectionData, message_id: &MessageId) -> String {
    let mut out = String::new();
    for promise in &detection.promises {
        match (&promise.due, &promise.due_phrase) {
            (Some(due), Some(phrase)) => {
                out.push_str(&format!(
                    "You promised: {}, due {} (\"{phrase}\").\n  Remind me: mxr commitments add {message_id} --what {} --due {}\n",
                    promise.what,
                    due.description,
                    shell_quote(&promise.what),
                    due.at.to_rfc3339(),
                ));
            }
            _ => {
                out.push_str(&format!(
                    "You promised: {} (no date given).\n  Remind me: mxr commitments add {message_id} --what {} --due \"<when>\"\n",
                    promise.what,
                    shell_quote(&promise.what),
                ));
            }
        }
    }
    out
}

/// Single-quote for POSIX shells; the text is the model's wording of the
/// user's own mail, so it may hold anything.
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use mxr_core::natural_time::{resolve_time, TimePrefs};
    use mxr_protocol::DetectedPromiseData;

    #[tokio::test(start_paused = true)]
    async fn a_promise_check_that_never_answers_is_dropped_at_the_deadline() {
        let started = tokio::time::Instant::now();
        let answer = within_deadline(AFTER_SEND_DEADLINE, std::future::pending::<()>()).await;
        assert!(answer.is_none());
        assert_eq!(started.elapsed(), AFTER_SEND_DEADLINE);
        assert!(AFTER_SEND_DEADLINE <= std::time::Duration::from_secs(10));
        assert_eq!(
            within_deadline(AFTER_SEND_DEADLINE, async { 7 }).await,
            Some(7)
        );
    }

    #[test]
    fn the_note_never_lands_on_piped_or_machine_stdout() {
        // Piped stdout (not a terminal): stderr, whatever the format.
        for format in [
            None,
            Some(OutputFormat::Table),
            Some(OutputFormat::Json),
            Some(OutputFormat::Jsonl),
            Some(OutputFormat::Ids),
        ] {
            assert!(!note_on_stdout(format.as_ref(), false), "{format:?}");
        }
        // A person at a terminal reading table output sees it inline.
        assert!(note_on_stdout(None, true));
        assert!(note_on_stdout(Some(&OutputFormat::Table), true));
        assert!(!note_on_stdout(Some(&OutputFormat::Json), true));
    }

    #[test]
    fn promise_lines_give_the_exact_command_that_keeps_each_promise() {
        let now = chrono::Utc
            .with_ymd_and_hms(2026, 9, 29, 14, 0, 0)
            .unwrap()
            .fixed_offset();
        let due = resolve_time("friday", &now, &TimePrefs::default()).unwrap();
        let message_id = MessageId::new();
        let detection = PromiseDetectionData {
            status: PromiseDetectionStatusData::Ready,
            promises: vec![
                DetectedPromiseData {
                    what: "send Maya's deck".into(),
                    due_phrase: Some("by Friday".into()),
                    due: Some(due),
                },
                DetectedPromiseData {
                    what: "loop in Sam".into(),
                    due_phrase: None,
                    due: None,
                },
            ],
            provenance: None,
            message: None,
        };
        let lines = promise_lines(&detection, &message_id);
        assert_eq!(
            lines,
            format!(
                "You promised: send Maya's deck, due Friday 2 October, 09:00 (\"by Friday\").\n  \
                 Remind me: mxr commitments add {message_id} --what 'send Maya'\\''s deck' --due 2026-10-02T09:00:00+00:00\n\
                 You promised: loop in Sam (no date given).\n  \
                 Remind me: mxr commitments add {message_id} --what 'loop in Sam' --due \"<when>\"\n"
            )
        );
    }
}
