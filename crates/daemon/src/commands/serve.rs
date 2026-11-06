mod hosting;
mod proxy;
mod runner;
mod state;
mod supervisor;

pub(crate) async fn prepare_foreground() -> anyhow::Result<()> {
    hosting::prepare_foreground().await
}
pub(crate) async fn stop_daemon(confirm: bool) -> anyhow::Result<()> {
    hosting::stop_daemon(confirm).await
}
pub(crate) fn daemon_desired_stopped() -> anyhow::Result<bool> {
    Ok(mxr_config::hosting_daemon_desired_stopped()?)
}
pub(crate) fn mark_daemon_started() -> anyhow::Result<()> {
    let Some(mut config) = state::read()? else {
        return Ok(());
    };
    if config.daemon_enabled {
        return Ok(());
    }
    let _lock = state::lock()?;
    config = state::read()?.ok_or_else(|| anyhow::anyhow!("Hosting state changed"))?;
    config.daemon_enabled = true;
    state::write(&config)
}
pub(crate) fn normalize_public_origin(origin: &str) -> anyhow::Result<String> {
    hosting::normalize_origin(origin)
}

use crate::commands::daemon_response_error;
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
        anyhow::bail!("Choose a `mxr serve` subcommand, or use `mxr serve --foreground` to run the loopback gateway.")
    };
    let request = match action {
        ServeAction::Setup {
            public_origin,
            listen,
            dry_run,
            confirm,
        } => return hosting::setup(public_origin, listen, confirm && !dry_run, format).await,
        ServeAction::Start { dry_run, confirm } => {
            return hosting::start(confirm && !dry_run, format).await
        }
        ServeAction::Stop {
            all,
            dry_run,
            confirm,
        } => return hosting::stop(all, confirm && !dry_run, format).await,
        ServeAction::Supervised => return runner::supervised().await,
        ServeAction::Status => return hosting::status(format).await,
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
        Response::Error { message, kind, .. } => {
            return Err(daemon_response_error(
                format!("Cannot inspect local daemon compatibility: {message}"),
                kind,
            ));
        }
        _ => anyhow::bail!(
            "Local daemon did not report its protocol; upgrade mxr and restart the local daemon"
        ),
    }
    match client.request(Request::RemoteAuthority { request }).await? {
        Response::Ok { data } => crate::output::print_json(&data, format)?,
        Response::Error {
            message,
            code,
            kind,
            ..
        } => {
            return Err(daemon_response_error(
                format!("Local pairing authority failed ({code}): {message}"),
                kind,
            ));
        }
    }
    Ok(())
}
