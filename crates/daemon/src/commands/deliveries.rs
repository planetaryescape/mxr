//! `mxr deliveries` — track packages and deliveries detected in your mail.

use crate::cli::{DeliveriesAction, OutputFormat};
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};
use mxr_core::id::DeliveryId;
use mxr_protocol::*;
use std::str::FromStr;

pub async fn run(
    action: Option<DeliveriesAction>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let action = action.unwrap_or_else(|| DeliveriesAction::List {
        filter: "active".to_string(),
    });
    let mut client = IpcClient::connect().await?;

    match action {
        DeliveriesAction::List { filter } => {
            let resp = client
                .request(Request::ListDeliveries {
                    filter: Some(filter),
                })
                .await?;
            match resp {
                Response::Ok {
                    data: ResponseData::Deliveries { deliveries },
                } => match resolve_format(format) {
                    OutputFormat::Json => {
                        println!("{}", serde_json::to_string_pretty(&deliveries)?);
                    }
                    OutputFormat::Jsonl => println!("{}", jsonl(&deliveries)?),
                    _ => print_table(&deliveries),
                },
                Response::Error { message, .. } => anyhow::bail!("{message}"),
                _ => anyhow::bail!("Unexpected response"),
            }
        }
        DeliveriesAction::Get { delivery_id } => {
            let resp = client
                .request(Request::GetDelivery {
                    delivery_id: parse_id(&delivery_id)?,
                })
                .await?;
            match resp {
                Response::Ok {
                    data: ResponseData::Delivery { delivery },
                } => println!("{}", serde_json::to_string_pretty(&delivery)?),
                Response::Error { message, .. } => anyhow::bail!("{message}"),
                _ => anyhow::bail!("Unexpected response"),
            }
        }
        DeliveriesAction::Resolve { delivery_id } => {
            let resp = client
                .request(Request::ResolveDelivery {
                    delivery_id: parse_id(&delivery_id)?,
                })
                .await?;
            match resp {
                Response::Ok {
                    data: ResponseData::Delivery { delivery },
                } => println!(
                    "Resolved {} ({})",
                    short(&delivery.id.to_string()),
                    delivery.status
                ),
                Response::Error { message, .. } => anyhow::bail!("{message}"),
                _ => anyhow::bail!("Unexpected response"),
            }
        }
        DeliveriesAction::Dismiss { delivery_id } => {
            let resp = client
                .request(Request::DismissDelivery {
                    delivery_id: parse_id(&delivery_id)?,
                })
                .await?;
            match resp {
                Response::Ok {
                    data: ResponseData::Ack,
                } => println!("Dismissed {delivery_id}"),
                Response::Error { message, .. } => anyhow::bail!("{message}"),
                _ => anyhow::bail!("Unexpected response"),
            }
        }
        DeliveriesAction::Scan {
            since_days,
            dry_run,
        } => {
            if !dry_run {
                eprintln!(
                    "Scanning mail for deliveries… large windows with LLM enrichment can take a few minutes."
                );
            }
            // A backfill over a wide window makes one LLM call per shortlisted
            // candidate, which easily exceeds the default 120s IPC timeout.
            // `request_with_events` waits without a deadline; the daemon runs
            // the scan to completion and replies with the summary.
            let resp = client
                .request_with_events(
                    Request::ScanDeliveries {
                        since_days,
                        dry_run,
                    },
                    |_| {},
                )
                .await?;
            match resp {
                Response::Ok {
                    data: ResponseData::DeliveryScan { summary },
                } => match resolve_format(format) {
                    OutputFormat::Json | OutputFormat::Jsonl => {
                        println!("{}", serde_json::to_string_pretty(&summary)?);
                    }
                    _ => println!(
                        "{}scanned {}, created {}, updated {}, shortlisted {}",
                        if summary.dry_run { "[dry-run] " } else { "" },
                        summary.scanned,
                        summary.created,
                        summary.updated,
                        summary.shortlisted,
                    ),
                },
                Response::Error { message, .. } => anyhow::bail!("{message}"),
                _ => anyhow::bail!("Unexpected response"),
            }
        }
    }

    Ok(())
}

fn parse_id(s: &str) -> anyhow::Result<DeliveryId> {
    DeliveryId::from_str(s).map_err(|_| anyhow::anyhow!("invalid delivery id: {s}"))
}

fn print_table(deliveries: &[DeliveryData]) {
    if deliveries.is_empty() {
        println!("No deliveries");
        return;
    }
    for d in deliveries {
        let merchant = d
            .merchant
            .as_deref()
            .or(d.carrier.as_deref())
            .unwrap_or("?");
        let eta = d
            .eta_until
            .map_or_else(|| "—".to_string(), |e| e.format("%Y-%m-%d").to_string());
        let tracking = d.tracking_number.as_deref().unwrap_or("—");
        println!(
            "  {:<16} {:<16} eta {:<10} {:<24} [{}]",
            trunc(merchant, 16),
            d.status,
            eta,
            trunc(tracking, 24),
            short(&d.id.to_string()),
        );
    }
}

fn trunc(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n.saturating_sub(1)).collect::<String>() + "…"
    }
}

fn short(id: &str) -> String {
    id.chars().take(8).collect()
}
