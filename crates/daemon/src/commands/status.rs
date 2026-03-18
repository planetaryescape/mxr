use crate::cli::OutputFormat;
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_protocol::*;

pub async fn run(format: Option<OutputFormat>) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let resp = client.request(Request::GetStatus).await?;

    let fmt = resolve_format(format);
    match resp {
        Response::Ok {
            data:
                ResponseData::Status {
                    uptime_secs,
                    accounts,
                    total_messages,
                },
        } => match fmt {
            OutputFormat::Json => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "uptime_secs": uptime_secs,
                        "accounts": accounts,
                        "total_messages": total_messages,
                    }))?
                );
            }
            _ => {
                println!("Uptime: {}s", uptime_secs);
                println!("Accounts: {}", accounts.join(", "));
                println!("Total messages: {}", total_messages);
            }
        },
        Response::Error { message } => anyhow::bail!("{}", message),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}
