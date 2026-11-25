//! `mxr relationships` — rebuild relationship profiles for many contacts.

use crate::cli::{OutputFormat, RelationshipsAction};
use crate::commands::daemon_response_error;
use crate::commands::resolve_optional_account;
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_protocol::{Request, Response, ResponseData};

pub async fn run(action: RelationshipsAction) -> anyhow::Result<()> {
    let RelationshipsAction::Rebuild {
        account,
        since_days,
        dry_run,
        format,
    } = action;
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    if !dry_run {
        eprintln!(
            "Rebuilding relationship profiles… each stale summary is one model call, so a wide window can take a while."
        );
    }
    let resp = client
        .request_with_events(
            Request::RebuildRelationshipProfiles {
                account_id,
                since_days,
                dry_run,
            },
            |_| {},
        )
        .await?;
    match resp {
        Response::Ok {
            data: ResponseData::RelationshipRebuild { summary },
        } => match resolve_format(format) {
            OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&summary)?),
            OutputFormat::Jsonl => println!("{}", serde_json::to_string(&summary)?),
            _ if summary.dry_run => println!(
                "[dry-run] {} contacts with mail in the last {since_days} days; {} model calls",
                summary.contacts, summary.model_calls
            ),
            _ => println!(
                "rebuilt {} contacts with mail in the last {since_days} days; {} model calls, {} errors",
                summary.contacts, summary.model_calls, summary.errors
            ),
        },
        Response::Error { message, kind, .. } => return Err(daemon_response_error(message, kind)),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}
