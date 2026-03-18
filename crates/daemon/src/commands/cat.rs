use crate::cli::OutputFormat;
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_core::MessageId;
use mxr_protocol::*;

pub async fn run(
    message_id: String,
    _raw: bool,
    html: bool,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mid = MessageId::from_uuid(uuid::Uuid::parse_str(&message_id)?);
    let mut client = IpcClient::connect().await?;
    let resp = client.request(Request::GetBody { message_id: mid }).await?;

    let fmt = resolve_format(format);
    match resp {
        Response::Ok {
            data: ResponseData::Body { body },
        } => match fmt {
            OutputFormat::Json => {
                println!("{}", serde_json::to_string_pretty(&body)?);
            }
            _ => {
                if html {
                    if let Some(ref h) = body.text_html {
                        println!("{}", h);
                    } else {
                        println!("(no HTML body)");
                    }
                } else if let Some(ref t) = body.text_plain {
                    println!("{}", t);
                } else {
                    println!("(no text body)");
                }
            }
        },
        Response::Error { message } => anyhow::bail!("{}", message),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}
