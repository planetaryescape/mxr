use crate::cli::OutputFormat;
use crate::commands::expect_response;
use crate::commands::selection::{resolve_thread_ids, SelectionLimit};
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_core::ThreadId;
use mxr_protocol::*;

pub async fn run(
    thread_id: Option<String>,
    search: Option<String>,
    first: bool,
    limit: Option<u32>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let ids = resolve_thread_ids(
        &mut client,
        thread_id.into_iter().collect(),
        search,
        SelectionLimit::from_flags(first, limit),
    )
    .await?;
    if ids.is_empty() {
        anyhow::bail!("No threads matched");
    }

    let fmt = resolve_format(format);
    match fmt {
        OutputFormat::Json => {
            let mut payloads = Vec::with_capacity(ids.len());
            for id in &ids {
                let (thread, messages) = fetch_thread(&mut client, id.clone()).await?;
                payloads.push(serde_json::json!({
                    "thread": thread,
                    "messages": messages,
                }));
            }
            if ids.len() == 1 {
                println!("{}", serde_json::to_string_pretty(&payloads[0])?);
            } else {
                println!("{}", serde_json::to_string_pretty(&payloads)?);
            }
        }
        OutputFormat::Jsonl => {
            for id in &ids {
                let (thread, messages) = fetch_thread(&mut client, id.clone()).await?;
                println!(
                    "{}",
                    serde_json::to_string(&serde_json::json!({
                        "thread": thread,
                        "messages": messages,
                    }))?
                );
            }
        }
        OutputFormat::Csv => {
            let mut writer = csv::Writer::from_writer(Vec::new());
            writer.write_record(["thread_id", "subject", "message_count"])?;
            for id in &ids {
                let (thread, _) = fetch_thread(&mut client, id.clone()).await?;
                writer.write_record(&[
                    id.as_str().clone(),
                    thread.subject.clone(),
                    thread.message_count.to_string(),
                ])?;
            }
            println!("{}", String::from_utf8(writer.into_inner()?)?.trim_end());
        }
        OutputFormat::Ids => {
            for id in &ids {
                println!("{id}");
            }
        }
        OutputFormat::Table => {
            for (index, id) in ids.iter().enumerate() {
                if ids.len() > 1 {
                    if index > 0 {
                        println!();
                    }
                    println!("--- {} ---", id.as_str());
                }
                let (thread, messages) = fetch_thread(&mut client, id.clone()).await?;
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
    }
    Ok(())
}

async fn fetch_thread(
    client: &mut IpcClient,
    id: ThreadId,
) -> anyhow::Result<(mxr_core::Thread, Vec<mxr_core::Envelope>)> {
    let resp = client.request(Request::GetThread { thread_id: id }).await?;
    expect_response(resp, |r| match r {
        Response::Ok {
            data: ResponseData::Thread {
                thread, messages, ..
            },
        } => Some((thread, messages)),
        _ => None,
    })
}
