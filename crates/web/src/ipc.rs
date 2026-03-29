use super::*;

pub(crate) async fn ipc_request(
    socket_path: &Path,
    request: Request,
) -> Result<ResponseData, BridgeError> {
    let stream = UnixStream::connect(socket_path)
        .await
        .map_err(|error| BridgeError::Connect(error.to_string()))?;
    let mut framed = Framed::new(stream, IpcCodec::new());
    let message = IpcMessage {
        id: 1,
        payload: IpcPayload::Request(request),
    };
    framed
        .send(message)
        .await
        .map_err(|error| BridgeError::Ipc(error.to_string()))?;

    loop {
        match framed.next().await {
            Some(Ok(response)) => match response.payload {
                IpcPayload::Response(mxr_protocol::Response::Ok { data }) => {
                    return Ok(data)
                }
                IpcPayload::Response(mxr_protocol::Response::Error { message }) => {
                    return Err(BridgeError::Ipc(message));
                }
                IpcPayload::Event(_) => continue,
                _ => return Err(BridgeError::UnexpectedResponse),
            },
            Some(Err(error)) => return Err(BridgeError::Ipc(error.to_string())),
            None => return Err(BridgeError::Ipc("connection closed".into())),
        }
    }
}

pub(crate) async fn bridge_events(mut socket: WebSocket, socket_path: PathBuf) {
    let stream = match UnixStream::connect(&socket_path).await {
        Ok(stream) => stream,
        Err(error) => {
            let _ = socket
                .send(WebSocketMessage::Text(
                    serde_json::json!({ "error": error.to_string() })
                        .to_string()
                        .into(),
                ))
                .await;
            return;
        }
    };
    let mut framed = Framed::new(stream, IpcCodec::new());

    loop {
        match framed.next().await {
            Some(Ok(message)) => match message.payload {
                IpcPayload::Event(event) => {
                    let payload = match serde_json::to_string(&event) {
                        Ok(payload) => payload,
                        Err(_) => break,
                    };
                    if socket
                        .send(WebSocketMessage::Text(payload.into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                _ => continue,
            },
            Some(Err(_)) | None => break,
        }
    }
}

pub(crate) fn parse_thread_id(value: &str) -> Result<ThreadId, BridgeError> {
    Uuid::parse_str(value)
        .map(ThreadId::from_uuid)
        .map_err(|_| BridgeError::Ipc(format!("invalid thread id: {value}")))
}

pub(crate) fn parse_message_id(value: &str) -> Result<MessageId, BridgeError> {
    Uuid::parse_str(value)
        .map(MessageId::from_uuid)
        .map_err(|_| BridgeError::Ipc(format!("invalid message id: {value}")))
}

pub(crate) fn parse_attachment_id(
    value: &str,
) -> Result<mxr_core::AttachmentId, BridgeError> {
    Uuid::parse_str(value)
        .map(mxr_core::AttachmentId::from_uuid)
        .map_err(|_| BridgeError::Ipc(format!("invalid attachment id: {value}")))
}

pub(crate) fn parse_message_ids(values: &[String]) -> Result<Vec<MessageId>, BridgeError> {
    values
        .iter()
        .map(|value| parse_message_id(value))
        .collect::<Result<Vec<_>, _>>()
}

pub(crate) fn parse_account_id(value: &str) -> Result<AccountId, BridgeError> {
    Uuid::parse_str(value)
        .map(AccountId::from_uuid)
        .map_err(|_| BridgeError::Ipc(format!("invalid account id: {value}")))
}

pub(crate) fn parse_label_id(value: &str) -> Result<LabelId, BridgeError> {
    Uuid::parse_str(value)
        .map(LabelId::from_uuid)
        .map_err(|_| BridgeError::Ipc(format!("invalid label id: {value}")))
}
