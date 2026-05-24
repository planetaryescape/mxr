use crate::cli::{OutputFormat, SavedAction};
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};
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
                    OutputFormat::Jsonl => {
                        println!("{}", jsonl(&searches)?);
                    }
                    OutputFormat::Csv => {
                        let mut writer = csv::Writer::from_writer(Vec::new());
                        writer.write_record(["name", "query", "mode"])?;
                        for search in &searches {
                            writer.write_record([
                                search.name.as_str(),
                                search.query.as_str(),
                                search.search_mode.as_str(),
                            ])?;
                        }
                        println!("{}", String::from_utf8(writer.into_inner()?)?.trim_end());
                    }
                    OutputFormat::Ids => {
                        for search in &searches {
                            println!("{}", search.name);
                        }
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
                Response::Error { message, .. } => anyhow::bail!("{message}"),
                _ => anyhow::bail!("Unexpected response"),
            }
        }
        SavedAction::Add { name, query, mode } => {
            let resp = client
                .request(Request::CreateSavedSearch {
                    name,
                    query,
                    search_mode: mode.map_or(mxr_core::SearchMode::Lexical, Into::into),
                })
                .await?;
            match resp {
                Response::Ok {
                    data: ResponseData::SavedSearchData { search },
                } => {
                    println!("Created saved search: {} -> {}", search.name, search.query);
                }
                Response::Error { message, .. } => anyhow::bail!("{message}"),
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
                    println!("Deleted saved search: {name}");
                }
                Response::Error { message, .. } => anyhow::bail!("{message}"),
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
                    OutputFormat::Jsonl => {
                        println!("{}", jsonl(&results)?);
                    }
                    OutputFormat::Csv => {
                        let mut writer = csv::Writer::from_writer(Vec::new());
                        writer.write_record([
                            "message_id",
                            "account_id",
                            "thread_id",
                            "score",
                            "mode",
                        ])?;
                        for result in &results {
                            writer.write_record(vec![
                                result.message_id.as_str(),
                                result.account_id.as_str(),
                                result.thread_id.as_str(),
                                result.score.to_string(),
                                result.mode.as_str().to_string(),
                            ])?;
                        }
                        println!("{}", String::from_utf8(writer.into_inner()?)?.trim_end());
                    }
                    OutputFormat::Ids => {
                        for result in &results {
                            println!("{}", result.message_id);
                        }
                    }
                    _ => {
                        println!("{} results", results.len());
                        for r in &results {
                            println!("  {} (score: {:.2})", r.message_id.as_str(), r.score);
                        }
                    }
                },
                Response::Error { message, .. } => anyhow::bail!("{message}"),
                _ => anyhow::bail!("Unexpected response"),
            }
        }
    }
    Ok(())
}
