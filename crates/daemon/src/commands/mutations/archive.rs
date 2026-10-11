use super::{IpcClient, OutputFormat};
use crate::output::resolve_format;
use mxr_protocol::{ArchiveOperationData, Request, Response, ResponseData};

pub async fn archive_operation(
    operation_id: Option<String>,
    submit: bool,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    if matches!(resolve_format(format.clone()), OutputFormat::Csv) {
        anyhow::bail!("Archive receipts support table, json, jsonl and ids; select a supported format before submission");
    }
    if submit && operation_id.is_none() {
        anyhow::bail!("Archive submission requires one explicit retained operation ID");
    }
    let list_mode = operation_id.is_none();
    let mut outcomes = Vec::new();
    let journal = mxr_client::ArchiveJournal::new(&mxr_config::data_dir())?;
    let operations = if let Some(id) = operation_id {
        vec![journal.load(&id)?]
    } else {
        journal.list()?
    };
    let mut client = IpcClient::connect().await?;
    for retained in operations {
        let result = if submit {
            client.submit_archive(&retained, true).await
        } else {
            match client
                .request(Request::GetArchiveOperation {
                    identity: retained.identity.clone(),
                })
                .await?
            {
                Response::Ok {
                    data: ResponseData::ArchiveOperation { operation },
                } => Ok(operation),
                response @ Response::Error { .. } => {
                    println!("{}", serde_json::to_string(&response)?);
                    anyhow::bail!("Archive query refused; original identity retained");
                }
                _ => anyhow::bail!("Unexpected Archive response"),
            }
        };
        match result {
            Ok(operation) => {
                if list_mode && matches!(resolve_format(format.clone()), OutputFormat::Json) {
                    outcomes.push(operation);
                } else {
                    print_operation(&operation, format.clone())?;
                    if submit {
                        require_confirmed_completion(&operation)?;
                    }
                }
            }
            Err(error) => {
                print_recovery_error(&retained, &error, format.clone())?;
                return Err(error);
            }
        }
    }
    if list_mode && matches!(resolve_format(format), OutputFormat::Json) {
        println!("{}", serde_json::to_string_pretty(&outcomes)?);
    }
    Ok(())
}

pub(super) fn print_operation(
    operation: &ArchiveOperationData,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    match resolve_format(format) {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(operation)?),
        OutputFormat::Jsonl => println!("{}", serde_json::to_string(operation)?),
        OutputFormat::Ids => println!("{}", operation.identity.operation_id),
        OutputFormat::Csv => anyhow::bail!("Archive receipts support table, json, jsonl and ids"),
        OutputFormat::Table => {
            println!(
                "Archive operation {} ({})",
                operation.identity.operation_id,
                if operation.running {
                    "running"
                } else if operation.accepted {
                    "accepted"
                } else {
                    "reviewed; not submitted"
                }
            );
            for item in &operation.items {
                println!(
                    "{} {:?} {}",
                    item.message_id,
                    item.state,
                    crate::output::terminal_text(item.reason.as_deref().unwrap_or(""))
                );
            }
            println!(
                "Recover: mxr archive-operation {}",
                operation.identity.operation_id
            );
            if operation.undo_available {
                println!("Undo with: mxr undo {}", operation.mutation_id);
            }
            if !operation.accepted {
                println!(
                    "Submit reviewed selection: mxr archive-operation {} --submit",
                    operation.identity.operation_id
                );
            }
        }
    }
    Ok(())
}

pub(super) fn print_recovery_error(
    operation: &ArchiveOperationData,
    error: &anyhow::Error,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    if matches!(
        resolve_format(format),
        OutputFormat::Json | OutputFormat::Jsonl
    ) {
        println!(
            "{}",
            serde_json::to_string(
                &serde_json::json!({"identity": operation.identity, "state": "response_unavailable", "error": error.to_string(), "retry_guidance": "Query the retained identity; never create a new identity for an uncertain effect"})
            )?
        );
    }
    Ok(())
}

pub(super) fn require_confirmed_completion(operation: &ArchiveOperationData) -> anyhow::Result<()> {
    if !has_confirmed_completion(operation.items.iter().map(|item| item.state)) {
        anyhow::bail!("Archive has no confirmed completion; recover the same identity with mxr archive-operation {}", operation.identity.operation_id);
    }
    Ok(())
}
fn has_confirmed_completion(
    states: impl Iterator<Item = mxr_protocol::ArchiveItemStateData>,
) -> bool {
    states
        .into_iter()
        .any(|state| state == mxr_protocol::ArchiveItemStateData::Completed)
}
#[cfg(test)]
mod tests {
    use super::*;
    use mxr_protocol::ArchiveItemStateData::{Completed, NotExecuted, Unknown};
    #[test]
    fn archive_synchronous_exit_requires_at_least_one_confirmed_completion() {
        assert!(!has_confirmed_completion(
            [NotExecuted, NotExecuted].into_iter()
        ));
        assert!(!has_confirmed_completion([Unknown, Unknown].into_iter()));
        assert!(has_confirmed_completion([Completed, Unknown].into_iter()));
    }
}
