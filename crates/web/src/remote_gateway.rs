//! Dedicated native-client gateway. No bootstrap, filesystem or administration routes.
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use futures::StreamExt;
use mxr_client::IpcConnection;
use mxr_core::RemoteSecret;
use mxr_protocol::{
    ClientKind, IpcCodec, IpcMessage, IpcPayload, RemoteRequest, Request, Response,
};
use std::path::PathBuf;
use tokio_util::{
    bytes::BytesMut,
    codec::{Decoder, Encoder},
};

const LIMIT: usize = 16 * 1024 * 1024 + 4;

pub fn router(socket: PathBuf) -> Router {
    Router::new()
        .route("/api/v1/remote/pairings", post(pairing))
        .route("/api/v1/remote/self/deregister", post(cleanup))
        .route("/api/v1/remote/ipc", get(upgrade))
        .layer(axum::extract::DefaultBodyLimit::max(16 * 1024))
        .with_state(socket)
}

async fn call(socket: &std::path::Path, request: Request) -> Result<Response, StatusCode> {
    let mut connection = IpcConnection::connect_supervised(socket, ClientKind::Web)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    connection
        .request_response(request, |_| {}, Some(std::time::Duration::from_secs(10)))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)
}

async fn pairing(
    State(socket): State<PathBuf>,
    Json(request): Json<RemoteRequest>,
) -> Result<Json<Response>, StatusCode> {
    if !matches!(
        request,
        RemoteRequest::StartPairing { .. } | RemoteRequest::ClaimPairing { .. }
    ) {
        return Err(StatusCode::FORBIDDEN);
    }
    public_response(call(&socket, Request::RemoteAuthority { request }).await?)
}
async fn cleanup(
    State(socket): State<PathBuf>,
    Json(request): Json<RemoteRequest>,
) -> Result<Json<Response>, StatusCode> {
    if !matches!(
        request,
        RemoteRequest::Cleanup { .. } | RemoteRequest::AbandonPairing { .. }
    ) {
        return Err(StatusCode::FORBIDDEN);
    }
    public_response(call(&socket, Request::RemoteAuthority { request }).await?)
}
fn public_response(response: Response) -> Result<Json<Response>, StatusCode> {
    match response {
        Response::Ok { .. } => Ok(Json(response)),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

async fn upgrade(
    State(socket): State<PathBuf>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, StatusCode> {
    // Native clients never send browser Origin. Browser grants are a later stage.
    if headers.contains_key("origin") {
        return Err(StatusCode::FORBIDDEN);
    }
    let proof = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let (id, secret) = proof.split_once(':').ok_or(StatusCode::UNAUTHORIZED)?;
    if id.is_empty()
        || secret.len() != 64
        || !secret
            .bytes()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let id = id.to_owned();
    let secret = RemoteSecret::new(secret.to_owned());
    let _ = public_response(
        call(
            &socket,
            Request::RemoteRead {
                client_id: id.clone(),
                mail_secret: secret.clone(),
                request: Box::new(Request::GetRemoteReady),
            },
        )
        .await?,
    )?;
    Ok(ws
        .max_message_size(LIMIT)
        .max_frame_size(LIMIT)
        .on_upgrade(move |ws| session(ws, socket, id, secret)))
}

async fn session(mut ws: WebSocket, socket: PathBuf, id: String, secret: RemoteSecret) {
    let Ok(mut connection) = IpcConnection::connect_supervised(&socket, ClientKind::Web).await
    else {
        return;
    };
    let mut bytes = BytesMut::new();
    let mut codec = IpcCodec::new();
    while let Some(Ok(message)) = ws.next().await {
        match message {
            Message::Binary(data) => {
                if bytes.len().saturating_add(data.len()) > LIMIT {
                    break;
                }
                bytes.extend_from_slice(&data);
                loop {
                    let frame = match codec.decode(&mut bytes) {
                        Ok(Some(frame)) => frame,
                        Ok(None) => break,
                        Err(_) => return,
                    };
                    let IpcPayload::Request(request) = frame.payload else {
                        return;
                    };
                    // The daemon validates proof and the precise operation at every admission.
                    let response = match connection
                        .request_response(
                            Request::RemoteRead {
                                client_id: id.clone(),
                                mail_secret: secret.clone(),
                                request: Box::new(request),
                            },
                            |_| {},
                            Some(std::time::Duration::from_secs(30)),
                        )
                        .await
                    {
                        Ok(response) => response,
                        Err(_) => return,
                    };
                    let mut output = BytesMut::new();
                    if codec
                        .encode(
                            IpcMessage {
                                expected_workspace: None,
                                id: frame.id,
                                source: ClientKind::Daemon,
                                payload: IpcPayload::Response(response),
                            },
                            &mut output,
                        )
                        .is_err()
                    {
                        return;
                    }
                    if ws.send(Message::Binary(output.freeze())).await.is_err() {
                        return;
                    }
                }
            }
            Message::Ping(data) => {
                if ws.send(Message::Pong(data)).await.is_err() {
                    break;
                }
            }
            Message::Pong(_) => (),
            _ => break,
        }
    }
}

pub async fn serve(socket: PathBuf, address: std::net::SocketAddr) -> Result<(), std::io::Error> {
    if !address.ip().is_loopback() {
        return Err(std::io::Error::other("Remote gateway must bind loopback"));
    }
    let listener = tokio::net::TcpListener::bind(address).await?;
    axum::serve(listener, router(socket)).await
}
