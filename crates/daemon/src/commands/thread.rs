use crate::cli::OutputFormat;
use crate::ipc_client::IpcClient;
use crate::mxr_core::ThreadId;
use crate::mxr_protocol::*;
use crate::output::resolve_format;

pub async fn run(thread_id: String, format: Option<OutputFormat>) -> anyhow::Result<()> {
    let tid = ThreadId::from_uuid(uuid::Uuid::parse_str(&thread_id)?);
    let mut client = IpcClient::connect().await?;
    let resp = client
        .request(Request::GetThread { thread_id: tid })
        .await?;

    let fmt = resolve_format(format);
    match resp {
        Response::Ok {
            data: ResponseData::Thread { thread, messages },
        } => match fmt {
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
        },
        Response::Error { message } => anyhow::bail!("{}", message),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}
