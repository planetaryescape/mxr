use crate::cli::{HumanizeAction, OutputFormat};
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_protocol::*;

pub async fn run(action: HumanizeAction, format: Option<OutputFormat>) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let resp = match action {
        HumanizeAction::Score { text } => client.request(Request::HumanizerScore { text }).await?,
        HumanizeAction::Rewrite {
            text,
            max_iterations,
        } => {
            client
                .request(Request::HumanizerRewrite {
                    text,
                    max_iterations,
                })
                .await?
        }
    };
    print_response(resp, resolve_format(format))
}

fn print_response(resp: Response, fmt: OutputFormat) -> anyhow::Result<()> {
    match resp {
        Response::Ok {
            data: ResponseData::HumanizerReport { report },
        } => match fmt {
            OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&report)?),
            OutputFormat::Jsonl => println!("{}", serde_json::to_string(&report)?),
            _ => {
                println!("humanizer: {}/100", report.score);
                for hit in report.hits {
                    println!("  {}: {}", hit.category, hit.matched);
                }
            }
        },
        Response::Ok {
            data:
                ResponseData::HumanizedText {
                    text,
                    report,
                    iterations,
                },
        } => match fmt {
            OutputFormat::Json => println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "text": text,
                    "report": report,
                    "iterations": iterations,
                }))?
            ),
            OutputFormat::Jsonl => println!(
                "{}",
                serde_json::to_string(&serde_json::json!({
                    "text": text,
                    "report": report,
                    "iterations": iterations,
                }))?
            ),
            _ => {
                println!("{text}");
                eprintln!("humanizer: {}/100, rewritten {}x", report.score, iterations);
            }
        },
        Response::Error { message, .. } => anyhow::bail!(message),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}
