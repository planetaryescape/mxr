use crate::{cli::RecoveryAction, ipc_client::IpcClient};
use anyhow::{bail, Context};
use mxr_protocol::{Request, Response};
use std::str::FromStr;

pub async fn run(action: RecoveryAction) -> anyhow::Result<()> {
    let format = match &action {
        RecoveryAction::Status { format }
        | RecoveryAction::Activate { format, .. }
        | RecoveryAction::PrepareDraft { format, .. } => format.clone(),
    };
    let mut client = IpcClient::connect().await?;
    let response = match action {
        RecoveryAction::Status { .. } => client.request(Request::GetRecoveryStatus).await?,
        RecoveryAction::Activate {
            source_owner_stopped,
            expected_workspace,
            confirm,
            ..
        } => {
            if confirm && !source_owner_stopped {
                bail!("Activation requires --source-owner-stopped --confirm after stopping the prior owner");
            }
            let expected_workspace: mxr_protocol::ArchiveWorkspaceData =
                serde_json::from_str(&expected_workspace)
                    .context("--expected-workspace must be the owner JSON from recovery status")?;
            client.expect_workspace(expected_workspace.clone()).await?;
            client
                .request(Request::ActivateRecovery {
                    expected_workspace,
                    source_owner_stopped,
                    dry_run: !confirm,
                })
                .await?
        }
        RecoveryAction::PrepareDraft {
            draft_id,
            expected_revision,
            expected_workspace,
            confirm,
            ..
        } => {
            let draft_id =
                mxr_core::DraftId::from_str(&draft_id).context("invalid recovered draft id")?;
            let expected_workspace: mxr_protocol::ArchiveWorkspaceData =
                serde_json::from_str(&expected_workspace)
                    .context("--expected-workspace must be the owner JSON from recovery status")?;
            client.expect_workspace(expected_workspace.clone()).await?;
            client
                .request(Request::PrepareRecoveredDraft {
                    draft_id,
                    expected_revision,
                    expected_workspace,
                    dry_run: !confirm,
                })
                .await?
        }
    };
    match response {
        Response::Ok { data } => {
            crate::output::print_json(&data, format.unwrap_or(crate::cli::OutputFormat::Json))?
        }
        Response::Error { message, code, .. } => bail!("recovery failed ({code}): {message}"),
    }
    Ok(())
}
