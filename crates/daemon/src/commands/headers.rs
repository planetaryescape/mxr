use crate::ipc_client::IpcClient;
use crate::mxr_core::MessageId;
use crate::mxr_protocol::*;

pub async fn run(message_id: String) -> anyhow::Result<()> {
    let mid = MessageId::from_uuid(uuid::Uuid::parse_str(&message_id)?);
    let mut client = IpcClient::connect().await?;
    let resp = client
        .request(Request::GetHeaders { message_id: mid })
        .await?;

    match resp {
        Response::Ok {
            data: ResponseData::Headers { headers },
        } => {
            for (key, value) in &headers {
                println!("{}: {}", key, value);
            }
        }
        Response::Error { message } => anyhow::bail!("{}", message),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}
