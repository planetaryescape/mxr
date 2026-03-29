use crate::commands::expect_response;
use crate::ipc_client::IpcClient;
use mxr_core::MessageId;
use mxr_protocol::*;

pub async fn run(message_id: String) -> anyhow::Result<()> {
    let mid = MessageId::from_uuid(uuid::Uuid::parse_str(&message_id)?);
    let mut client = IpcClient::connect().await?;
    let resp = client
        .request(Request::GetHeaders { message_id: mid })
        .await?;

    let headers = expect_response(resp, |r| match r {
        Response::Ok {
            data: ResponseData::Headers { headers },
        } => Some(headers),
        _ => None,
    })?;
    for (key, value) in &headers {
        println!("{key}: {value}");
    }
    Ok(())
}
