use crate::cli::{OutputFormat, SavedAction};
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_protocol::*;

pub async fn run(action: Option<SavedAction>, format: Option<OutputFormat>) -> anyhow::Result<()> {
    let action = action.unwrap_or(SavedAction::List);
    let mut client = IpcClient::connect().await?;

    match action {
        SavedAction::List => {
            let resp = client.request(Request::ListSavedSearches).await?;
            let fmt = resolve_format(format);
            match resp {
                Response::Ok {
                    data: ResponseData::SavedSearches { searches },
                } => match fmt {
                    OutputFormat::Json => {
                        println!("{}", serde_json::to_string_pretty(&searches)?);
                    }
                    _ => {
                        if searches.is_empty() {
                            println!("No saved searches");
                        } else {
                            for s in &searches {
                                println!("  {} -> {}", s.name, s.query);
                            }
                        }
                    }
                },
                Response::Error { message } => anyhow::bail!("{}", message),
                _ => anyhow::bail!("Unexpected response"),
            }
        }
        SavedAction::Add { name, query, mode } => {
            let resp = client
                .request(Request::CreateSavedSearch {
                    name,
                    query,
                    search_mode: mode
                        .map(Into::into)
                        .unwrap_or(mxr_core::SearchMode::Lexical),
                })
                .await?;
            match resp {
                Response::Ok {
                    data: ResponseData::SavedSearchData { search },
                } => {
                    println!("Created saved search: {} -> {}", search.name, search.query);
                }
                Response::Error { message } => anyhow::bail!("{}", message),
                _ => anyhow::bail!("Unexpected response"),
            }
        }
        SavedAction::Delete { name } => {
            let resp = client
                .request(Request::DeleteSavedSearch { name: name.clone() })
                .await?;
            match resp {
                Response::Ok {
                    data: ResponseData::Ack,
                } => {
                    println!("Deleted saved search: {}", name);
                }
                Response::Error { message } => anyhow::bail!("{}", message),
                _ => anyhow::bail!("Unexpected response"),
            }
        }
        SavedAction::Run { name } => {
            let resp = client
                .request(Request::RunSavedSearch { name, limit: 50 })
                .await?;
            let fmt = resolve_format(format);
            match resp {
                Response::Ok {
                    data: ResponseData::SearchResults { results, .. },
                } => match fmt {
                    OutputFormat::Json => {
                        println!("{}", serde_json::to_string_pretty(&results)?);
                    }
                    _ => {
                        println!("{} results", results.len());
                        for r in &results {
                            println!("  {} (score: {:.2})", r.message_id.as_str(), r.score);
                        }
                    }
                },
                Response::Error { message } => anyhow::bail!("{}", message),
                _ => anyhow::bail!("Unexpected response"),
            }
        }
    }
    Ok(())
}
