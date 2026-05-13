use crate::cli::OutputFormat;
use crate::commands::resolve_account;
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_protocol::*;

const WEEKDAY_LABELS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

pub async fn run(
    recipient: String,
    account: Option<String>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_account(&mut client, account.as_deref()).await?;
    let resp = client
        .request(Request::SendTimeRecommendation {
            account_id,
            recipient,
        })
        .await?;
    print(resp, resolve_format(format))
}

fn print(resp: Response, fmt: OutputFormat) -> anyhow::Result<()> {
    match resp {
        Response::Ok {
            data: ResponseData::SendTimeRecommendationResponse { recommendation },
        } => match fmt {
            OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&recommendation)?),
            OutputFormat::Jsonl => println!("{}", serde_json::to_string(&recommendation)?),
            _ => {
                println!("Recipient: {}", recommendation.recipient);
                println!(
                    "Confidence: {:?} (samples: {})",
                    recommendation.confidence, recommendation.sample_count
                );
                if let (Some(wd), Some(hr), Some(p50)) = (
                    recommendation.best_weekday,
                    recommendation.best_hour,
                    recommendation.best_p50_seconds,
                ) {
                    println!(
                        "Fastest bucket: {} {:02}:00 (p50 = {})",
                        WEEKDAY_LABELS.get(wd as usize).unwrap_or(&"?"),
                        hr,
                        humanize(p50)
                    );
                }
                if !recommendation.buckets.is_empty() {
                    println!();
                    println!("Buckets:");
                    for b in &recommendation.buckets {
                        println!(
                            "  {} {:02}:00  p50={}  n={}",
                            WEEKDAY_LABELS.get(b.weekday as usize).unwrap_or(&"?"),
                            b.hour,
                            humanize(b.p50_seconds),
                            b.sample_count,
                        );
                    }
                }
            }
        },
        Response::Error { message, .. } => anyhow::bail!(message),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}

fn humanize(secs: i64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86_400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86_400)
    }
}
