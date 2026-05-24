use crate::cli::{OutputFormat, ThreadsSort};
use crate::commands::expect_response;
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_core::id::{AccountId, LabelId};
use mxr_protocol::*;

pub async fn run(
    account: Option<String>,
    label: Option<String>,
    limit: u32,
    offset: u32,
    sort: Option<ThreadsSort>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;

    let account_id = match account {
        Some(s) => Some(
            s.parse::<AccountId>()
                .map_err(|e| anyhow::anyhow!("invalid --account: {e}"))?,
        ),
        None => None,
    };

    // Resolve --label name → LabelId via the ListLabels endpoint so the
    // CLI accepts the human-readable label name (matching `mxr labels`).
    let label_id = match label {
        Some(name) => {
            let resp = client
                .request(Request::ListLabels {
                    account_id: account_id.clone(),
                })
                .await?;
            let labels = expect_response(resp, |r| match r {
                Response::Ok {
                    data: ResponseData::Labels { labels },
                } => Some(labels),
                _ => None,
            })?;
            Some(
                labels
                    .into_iter()
                    .find(|l| l.name == name)
                    .map(|l| l.id)
                    .ok_or_else(|| anyhow::anyhow!("no label named '{name}'"))?,
            )
        }
        None => None,
    };

    let threads = fetch_threads(
        &mut client,
        account_id,
        label_id,
        limit,
        offset,
        sort.map(mxr_core::types::SortOrder::from),
    )
    .await?;

    let fmt = resolve_format(format);
    match fmt {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&threads)?);
        }
        OutputFormat::Jsonl => {
            for thread in &threads {
                println!("{}", serde_json::to_string(thread)?);
            }
        }
        OutputFormat::Csv => {
            let mut writer = csv::Writer::from_writer(Vec::new());
            writer.write_record(["thread_id", "subject", "message_count", "latest_date"])?;
            for thread in &threads {
                writer.write_record(&[
                    thread.id.as_str().clone(),
                    thread.subject.clone(),
                    thread.message_count.to_string(),
                    thread.latest_date.to_rfc3339(),
                ])?;
            }
            println!("{}", String::from_utf8(writer.into_inner()?)?.trim_end());
        }
        OutputFormat::Ids => {
            for thread in &threads {
                println!("{}", thread.id);
            }
        }
        OutputFormat::Table => {
            if threads.is_empty() {
                println!("(no threads)");
            }
            for thread in &threads {
                println!(
                    "{}  {} msgs ({} unread)  {}",
                    thread.latest_date.format("%Y-%m-%d %H:%M"),
                    thread.message_count,
                    thread.unread_count,
                    thread.subject,
                );
            }
        }
    }
    Ok(())
}

async fn fetch_threads(
    client: &mut IpcClient,
    account_id: Option<AccountId>,
    label_id: Option<LabelId>,
    limit: u32,
    offset: u32,
    sort: Option<mxr_core::types::SortOrder>,
) -> anyhow::Result<Vec<mxr_core::Thread>> {
    let resp = client
        .request(Request::ListThreads {
            account_id,
            label_id,
            limit,
            offset,
            sort,
        })
        .await?;
    expect_response(resp, |r| match r {
        Response::Ok {
            data: ResponseData::Threads { threads },
        } => Some(threads),
        _ => None,
    })
}
