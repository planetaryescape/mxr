use crate::cli::OutputFormat;
use crate::commands::expect_response;
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_core::ThreadId;
use mxr_protocol::*;

pub async fn run(thread_id: String, format: Option<OutputFormat>) -> anyhow::Result<()> {
    let tid = ThreadId::from_uuid(uuid::Uuid::parse_str(&thread_id)?);
    let mut client = IpcClient::connect().await?;
    let resp = client
        .request(Request::GetThread { thread_id: tid })
        .await?;

    let fmt = resolve_format(format);
    let (thread, messages) = expect_response(resp, |r| match r {
        Response::Ok {
            data: ResponseData::Thread { thread, messages },
        } => Some((thread, messages)),
        _ => None,
    })?;
    match fmt {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "thread": thread,
                    "messages": messages,
                }))?
            );
        }
        _ => {
            println!(
                "Thread: {} ({} messages)",
                thread.subject, thread.message_count
            );
            for env in &messages {
                println!(
                    "  {} {} - {}",
                    env.date.format("%Y-%m-%d %H:%M"),
                    env.from.email,
                    env.subject,
                );
            }
        }
    }
    Ok(())
}
