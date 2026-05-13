use crate::cli::OutputFormat;
use crate::commands::resolve_account;
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_protocol::*;

pub async fn run(
    query: String,
    account: Option<String>,
    limit: u32,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_account(&mut client, account.as_deref()).await?;
    let resp = client
        .request(Request::ExplainEntity {
            account_id,
            query,
            limit,
        })
        .await?;
    print(resp, resolve_format(format))
}

fn print(resp: Response, fmt: OutputFormat) -> anyhow::Result<()> {
    match resp {
        Response::Ok {
            data: ResponseData::EntityExplanation { entity },
        } => match fmt {
            OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&entity)?),
            OutputFormat::Jsonl => println!("{}", serde_json::to_string(&entity)?),
            _ => {
                println!("{} ({})", entity.canonical_name, entity.kind);
                println!("{}", entity.summary);
                if !entity.candidates.is_empty() {
                    println!("\nCandidates:");
                    for c in &entity.candidates {
                        println!(
                            "  - {} ({}, {} mentions)",
                            c.value, c.kind, c.mention_count
                        );
                    }
                }
                if !entity.citations.is_empty() {
                    println!("\nCitations:");
                    for c in &entity.citations {
                        println!("  - msg={} \"{}\"", c.msg_id, c.quote);
                    }
                }
            }
        },
        Response::Error { message, .. } => anyhow::bail!(message),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}
