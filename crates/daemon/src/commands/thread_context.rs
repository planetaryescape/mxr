//! `mxr briefing context <thread-id>`: the reader's context block on the
//! command line. Facts come first from `GetThreadContext`; the gist and the
//! ask come from `GetThreadGist` unless `--no-ai`.

use crate::cli::OutputFormat;
use crate::commands::daemon_response_error;
use crate::ipc_client::IpcClient;
use chrono::{DateTime, Utc};
use mxr_core::ThreadId;
use mxr_protocol::{Request, Response, ResponseData, ThreadContextData, ThreadGistData};
use mxr_tui::thread_context_rows::context_rows;

pub async fn run(
    client: &mut IpcClient,
    thread_id: ThreadId,
    with_ai: bool,
    refresh: bool,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let context = match client
        .request(Request::GetThreadContext {
            thread_id: thread_id.clone(),
        })
        .await?
    {
        Response::Ok {
            data: ResponseData::ThreadContext { context },
        } => context,
        Response::Error { message, kind, .. } => return Err(daemon_response_error(message, kind)),
        _ => anyhow::bail!("Unexpected response to GetThreadContext"),
    };
    let gist = if with_ai {
        match client
            .request(Request::GetThreadGist { thread_id, refresh })
            .await?
        {
            Response::Ok {
                data: ResponseData::ThreadGist { gist },
            } => Some(gist),
            Response::Error { message, kind, .. } => {
                return Err(daemon_response_error(message, kind))
            }
            _ => anyhow::bail!("Unexpected response to GetThreadGist"),
        }
    } else {
        None
    };

    let payload = serde_json::json!({ "context": context, "gist": gist });
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&payload)?),
        OutputFormat::Jsonl => println!("{}", serde_json::to_string(&payload)?),
        _ => print!("{}", render(&context, gist.as_ref(), Utc::now())),
    }
    Ok(())
}

pub(crate) fn render(
    context: &ThreadContextData,
    gist: Option<&ThreadGistData>,
    now: DateTime<Utc>,
) -> String {
    context_rows(Some(context), gist, now)
        .into_iter()
        .map(|row| format!("{:<10}{}\n", row.label, row.text))
        .collect()
}
