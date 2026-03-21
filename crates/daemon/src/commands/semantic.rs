use crate::cli::{OutputFormat, SemanticAction, SemanticProfileAction};
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_protocol::{Request, Response, ResponseData};

pub async fn run(
    action: Option<SemanticAction>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let action = action.unwrap_or(SemanticAction::Status);
    let mut client = IpcClient::connect().await?;

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
        SemanticAction::Reindex => client.request(Request::ReindexSemantic).await?,
        SemanticAction::Profile { action } => match action.unwrap_or(SemanticProfileAction::List) {
            SemanticProfileAction::List => client.request(Request::GetSemanticStatus).await?,
            SemanticProfileAction::Install { profile } => {
                client
                    .request(Request::InstallSemanticProfile {
                        profile: profile.into(),
                    })
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

    match response {
        Response::Ok {
            data: ResponseData::SemanticStatus { snapshot },
        } => match resolve_format(format) {
            OutputFormat::Json => {
                println!("{}", serde_json::to_string_pretty(&snapshot)?);
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
                                .map(|v| v.to_rfc3339())
                                .unwrap_or_else(|| "-".to_string())
                        );
                    }
                }
            }
        },
        Response::Error { message } => anyhow::bail!("{}", message),
        _ => anyhow::bail!("Unexpected response"),
    }

    Ok(())
}
