use crate::{
    cli::{ServeAction, ServeClientAction, ServePairAction},
    ipc_client::IpcClient,
};
use mxr_protocol::{RemoteRequest, Request, Response};

pub async fn run(
    action: Option<ServeAction>,
    format: Option<crate::cli::OutputFormat>,
) -> anyhow::Result<()> {
    let format = format.unwrap_or(crate::cli::OutputFormat::Json);
    if !matches!(
        format,
        crate::cli::OutputFormat::Json | crate::cli::OutputFormat::Jsonl
    ) {
        anyhow::bail!("serve supports --format json or jsonl");
    }
    let Some(action) = action else {
        anyhow::bail!("Remote hosting is not implemented in this foundation. Use mxr serve status to inspect local pairing authority.")
    };
    let request = match action {
        ServeAction::Status => RemoteRequest::Status,
        ServeAction::Pair {
            action: ServePairAction::List,
        } => RemoteRequest::Pairings,
        ServeAction::Pair {
            action:
                ServePairAction::Approve {
                    request_id,
                    dry_run,
                    confirm,
                },
        } => RemoteRequest::ApprovePairing {
            request_id,
            dry_run: dry_run || !confirm,
        },
        ServeAction::Clients { action: None } => RemoteRequest::Clients,
        ServeAction::Clients {
            action:
                Some(ServeClientAction::Revoke {
                    client_id,
                    dry_run,
                    confirm,
                }),
        } => RemoteRequest::RevokeClient {
            client_id,
            dry_run: dry_run || !confirm,
        },
    };
    // Deliberately bypass selection/environment transports. No autostart or foreign restart.
    let mut client = IpcClient::connect_to(&mxr_config::socket_path()).await?;
    match client.request(Request::GetStatus).await? {
        Response::Ok {
            data:
                mxr_protocol::ResponseData::Status {
                    protocol_version, ..
                },
        } if protocol_version == mxr_protocol::IPC_PROTOCOL_VERSION => {}
        Response::Ok {
            data:
                mxr_protocol::ResponseData::Status {
                    protocol_version, ..
                },
        } => {
            anyhow::bail!("Local pairing authority requires IPC protocol {}; local daemon uses {protocol_version}. Upgrade mxr and restart the local daemon.", mxr_protocol::IPC_PROTOCOL_VERSION);
        }
        Response::Error { message, .. } => {
            anyhow::bail!("Cannot inspect local daemon compatibility: {message}")
        }
        _ => anyhow::bail!(
            "Local daemon did not report its protocol; upgrade mxr and restart the local daemon"
        ),
    }
    match client.request(Request::RemoteAuthority { request }).await? {
        Response::Ok { data } => crate::output::print_json(&data, format)?,
        Response::Error { message, code, .. } => {
            anyhow::bail!("Local pairing authority failed ({code}): {message}")
        }
    }
    Ok(())
}
