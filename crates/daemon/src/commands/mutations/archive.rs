use super::{IpcClient, OutputFormat};
use crate::commands::selection::CappedSelection;
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
    if list_mode && !submit {
        return query_archive_operation_list(operations, &mut client, format).await;
    }
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

async fn query_archive_operation_list(
    operations: Vec<ArchiveOperationData>,
    client: &mut IpcClient,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut outcomes = Vec::with_capacity(operations.len());
    for retained in operations {
        let outcome = match client
            .request(Request::GetArchiveOperation {
                identity: retained.identity.clone(),
            })
            .await
        {
            Err(error) => Err(error),
            Ok(response) => match response {
                Response::Ok {
                    data: ResponseData::ArchiveOperation { operation },
                } => Ok(operation),
                Response::Error { message, .. } => Err(anyhow::anyhow!(
                    "Archive query refused: {message}; original identity retained"
                )),
                _ => Err(anyhow::anyhow!("Unexpected Archive response")),
            },
        };
        outcomes.push((retained, outcome));
    }

    let resolved_format = resolve_format(format.clone());
    let errors = match resolved_format {
        OutputFormat::Json => {
            let records = archive_list_records(&outcomes)?;
            let errors = outcomes
                .iter()
                .filter(|(_, outcome)| outcome.is_err())
                .count();
            println!("{}", serde_json::to_string_pretty(&records)?);
            errors
        }
        OutputFormat::Jsonl => {
            let mut errors = 0;
            for (retained, outcome) in &outcomes {
                match outcome {
                    Ok(operation) => print_operation(operation, format.clone())?,
                    Err(error) => {
                        errors += 1;
                        print_recovery_error(retained, error, format.clone())?;
                    }
                }
            }
            errors
        }
        OutputFormat::Table | OutputFormat::Ids | OutputFormat::Csv => {
            let mut errors = 0;
            for (_, outcome) in &outcomes {
                match outcome {
                    Ok(operation) => print_operation(operation, format.clone())?,
                    Err(_) => errors += 1,
                }
            }
            errors
        }
    };
    if errors > 0 {
        anyhow::bail!(
            "Failed to query {errors} retained Archive operation(s); retry each original identity"
        );
    }
    Ok(())
}

fn archive_list_records(
    outcomes: &[(ArchiveOperationData, anyhow::Result<ArchiveOperationData>)],
) -> anyhow::Result<Vec<serde_json::Value>> {
    outcomes
        .iter()
        .map(|(retained, outcome)| match outcome {
            Ok(operation) => serde_json::to_value(operation).map_err(Into::into),
            Err(error) => Ok(recovery_error_value(retained, error)),
        })
        .collect()
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

/// Prints an archive operation for a selection. A capped `--search` adds
/// `matched_total` and `capped` to the JSON object (the operation type is a
/// protocol type, so the fields are merged in here, not added to it). A
/// preview says so as well in table output, or on stderr for the formats that
/// cannot carry fields. A real run already said it before submission.
pub(super) fn print_selected_operation(
    operation: &ArchiveOperationData,
    format: Option<OutputFormat>,
    capped: Option<CappedSelection>,
    preview: bool,
) -> anyhow::Result<()> {
    let Some(capped) = capped else {
        return print_operation(operation, format);
    };
    let resolved = resolve_format(format.clone());
    match resolved {
        OutputFormat::Json | OutputFormat::Jsonl => {
            let mut value = serde_json::to_value(operation)?;
            capped.merge_into(&mut value);
            if matches!(resolved, OutputFormat::Json) {
                println!("{}", serde_json::to_string_pretty(&value)?);
            } else {
                println!("{}", serde_json::to_string(&value)?);
            }
            return Ok(());
        }
        OutputFormat::Table if preview => println!("{}", capped.notice()),
        OutputFormat::Ids | OutputFormat::Csv if preview => {
            capped.announce(&mut std::io::stderr())?;
        }
        _ => {}
    }
    print_operation(operation, format)
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
            serde_json::to_string(&recovery_error_value(operation, error))?
        );
    }
    Ok(())
}

fn recovery_error_value(
    operation: &ArchiveOperationData,
    error: &anyhow::Error,
) -> serde_json::Value {
    serde_json::json!({
        "identity": operation.identity,
        "state": "response_unavailable",
        "error": error.to_string(),
        "retry_guidance": "Query the retained identity; never create a new identity for an uncertain effect"
    })
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
    #![expect(clippy::unwrap_used, reason = "tests unwrap fixture setup")]
    use super::*;
    use mxr_protocol::ArchiveIdentityData;
    use mxr_protocol::ArchiveItemStateData::{Completed, NotExecuted, Unknown};

    fn operation(operation_id: &str) -> ArchiveOperationData {
        ArchiveOperationData {
            identity: ArchiveIdentityData {
                operation_id: operation_id.into(),
                caller_id: "caller".into(),
                workspace: mxr_protocol::ArchiveWorkspaceData {
                    workspace_id: "workspace".into(),
                    recovery_generation: "generation".into(),
                },
            },
            review_id: "review".into(),
            mutation_id: "mutation".into(),
            accepted: false,
            undo_available: false,
            running: false,
            started_at: 1,
            items: Vec::new(),
        }
    }

    #[test]
    fn archive_synchronous_exit_requires_at_least_one_confirmed_completion() {
        assert!(!has_confirmed_completion(
            [NotExecuted, NotExecuted].into_iter()
        ));
        assert!(!has_confirmed_completion([Unknown, Unknown].into_iter()));
        assert!(has_confirmed_completion([Completed, Unknown].into_iter()));
    }

    #[test]
    fn archive_list_records_preserve_successes_after_query_errors() {
        let unavailable = operation("unavailable");
        let valid = operation("valid");
        let records = archive_list_records(&[
            (
                unavailable.clone(),
                Err(anyhow::anyhow!("workspace generation changed")),
            ),
            (valid.clone(), Ok(valid.clone())),
        ])
        .unwrap();

        assert_eq!(records.len(), 2);
        assert_eq!(
            records[0]["identity"],
            serde_json::to_value(unavailable.identity).unwrap()
        );
        assert_eq!(records[0]["state"], "response_unavailable");
        assert_eq!(records[0]["error"], "workspace generation changed");
        assert_eq!(
            records[0]["retry_guidance"],
            "Query the retained identity; never create a new identity for an uncertain effect"
        );
        assert_eq!(records[1], serde_json::to_value(valid).unwrap());
    }

    #[test]
    fn empty_archive_list_is_one_empty_json_array() {
        let records = archive_list_records(&[]).unwrap();
        assert_eq!(serde_json::to_string(&records).unwrap(), "[]");
    }
}
