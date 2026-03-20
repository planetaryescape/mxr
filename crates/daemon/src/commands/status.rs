use crate::cli::OutputFormat;
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_protocol::*;

fn render_status(
    uptime_secs: u64,
    accounts: &[String],
    total_messages: u32,
    format: OutputFormat,
) -> anyhow::Result<String> {
    Ok(match format {
        OutputFormat::Json => serde_json::to_string_pretty(&serde_json::json!({
            "uptime_secs": uptime_secs,
            "accounts": accounts,
            "total_messages": total_messages,
        }))?,
        _ => format!(
            "Uptime: {uptime_secs}s\nAccounts: {}\nTotal messages: {total_messages}",
            accounts.join(", ")
        ),
    })
}

pub async fn run(format: Option<OutputFormat>, watch: bool) -> anyhow::Result<()> {
    let fmt = resolve_format(format);

    loop {
        let mut client = IpcClient::connect().await?;
        let resp = client.request(Request::GetStatus).await?;

        match resp {
            Response::Ok {
                data:
                    ResponseData::Status {
                        uptime_secs,
                        accounts,
                        total_messages,
                    },
            } => {
                println!(
                    "{}",
                    render_status(uptime_secs, &accounts, total_messages, fmt.clone())?
                );
            }
            Response::Error { message } => anyhow::bail!("{}", message),
            _ => anyhow::bail!("Unexpected response"),
        }

        if !watch {
            break;
        }

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        if fmt != OutputFormat::Json {
            println!();
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_status_json_has_expected_fields() {
        let rendered = render_status(42, &["main".into()], 10, OutputFormat::Json).unwrap();
        assert!(rendered.contains("\"uptime_secs\": 42"));
        assert!(rendered.contains("\"total_messages\": 10"));
    }
}
