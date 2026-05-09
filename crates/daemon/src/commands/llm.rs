use crate::cli::{LlmAction, OutputFormat};
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_protocol::{Request, Response, ResponseData};

pub async fn run(action: Option<LlmAction>, format: Option<OutputFormat>) -> anyhow::Result<()> {
    let action = action.unwrap_or(LlmAction::Status);
    let mut client = IpcClient::connect().await?;

    let response = match action {
        LlmAction::Status => client.request(Request::GetLlmStatus).await?,
    };
    let snapshot = crate::commands::expect_response(response, |r| match r {
        Response::Ok {
            data: ResponseData::LlmStatus { snapshot },
        } => Some(snapshot),
        _ => None,
    })?;

    match resolve_format(format) {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&snapshot)?),
        OutputFormat::Jsonl => println!("{}", serde_json::to_string(&snapshot)?),
        _ => {
            println!(
                "enabled={} provider={} model={}",
                snapshot.enabled, snapshot.provider, snapshot.model
            );
            if let Some(base_url) = snapshot.base_url {
                println!("base_url={base_url}");
            }
            if let Some(api_key_env) = snapshot.api_key_env {
                println!(
                    "api_key_env={} present={}",
                    api_key_env, snapshot.api_key_present
                );
            }
            println!(
                "context_window={} supports_streaming={} timeout_secs={}",
                snapshot.context_window, snapshot.supports_streaming, snapshot.request_timeout_secs
            );
        }
    }

    Ok(())
}
