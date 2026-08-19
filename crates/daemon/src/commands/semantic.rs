use crate::cli::{OutputFormat, SemanticAction, SemanticProfileAction};
use crate::commands::progress::request_with_progress;
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_protocol::{Request, Response, ResponseData};

pub async fn run(
    action: Option<SemanticAction>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let action = action.unwrap_or(SemanticAction::Status);
    let mut client = IpcClient::connect().await?;
    let json_mode = matches!(
        resolve_format(format.clone()),
        OutputFormat::Json | OutputFormat::Jsonl
    );

    let response = match action {
        SemanticAction::Status => client.request(Request::GetSemanticStatus).await?,
        SemanticAction::Enable => {
            client
                .request(Request::EnableSemantic { enabled: true })
                .await?
        }
        SemanticAction::Disable => {
            client
                .request(Request::EnableSemantic { enabled: false })
                .await?
        }
        // Re-embeds every message before answering; minutes on a large
        // mailbox, so it must not run under the 120s request cap (#179).
        SemanticAction::Reindex { force } => {
            request_with_progress(
                &mut client,
                json_mode,
                "Embedding messages",
                Request::ReindexSemantic { force },
            )
            .await?
        }
        SemanticAction::Profile { action } => match action.unwrap_or(SemanticProfileAction::List) {
            SemanticProfileAction::List => client.request(Request::GetSemanticStatus).await?,
            // Downloads and verifies the model before answering.
            SemanticProfileAction::Install { profile } => {
                request_with_progress(
                    &mut client,
                    json_mode,
                    "Installing model",
                    Request::InstallSemanticProfile {
                        profile: profile.into(),
                    },
                )
                .await?
            }
            SemanticProfileAction::Use { profile } => {
                client
                    .request(Request::UseSemanticProfile {
                        profile: profile.into(),
                    })
                    .await?
            }
        },
    };

    let snapshot = crate::commands::expect_response(response, |r| match r {
        Response::Ok {
            data: ResponseData::SemanticStatus { snapshot },
        } => Some(snapshot),
        _ => None,
    })?;
    match resolve_format(format) {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&snapshot)?);
        }
        OutputFormat::Jsonl => {
            println!("{}", serde_json::to_string(&snapshot)?);
        }
        _ => {
            println!(
                "enabled={} active_profile={}",
                snapshot.enabled,
                snapshot.active_profile.as_str()
            );
            if snapshot.profiles.is_empty() {
                println!("no semantic profiles installed");
            } else {
                for profile in snapshot.profiles {
                    println!(
                        "{} status={:?} dims={} indexed_at={}",
                        profile.profile.as_str(),
                        profile.status,
                        profile.dimensions,
                        profile
                            .last_indexed_at
                            .map_or_else(|| "-".to_string(), |v| v.to_rfc3339())
                    );
                }
            }
        }
    }

    Ok(())
}
