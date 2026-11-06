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
use futures::{SinkExt, StreamExt};
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
        .route("/api/v1/health", get(health))
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
                expected_workspace: None,
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
    let session_id = uuid::Uuid::now_v7().to_string();
    let mut surface = "web";
    let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(20));
    let mut bytes = BytesMut::new();
    let mut codec = IpcCodec::new();
    'session: loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                if !touch(&socket, &id, &secret, &session_id, surface).await && !authenticated(&socket, &id, &secret).await { break; }
            }
            event = connection.next_event() => {
                let Ok(frame) = event else { break; };
                if let IpcPayload::Event(event) = frame.payload {
                    if !crate::remote_browser::event_visible(&id, &event) { continue; }
                    if !matches!(event, mxr_protocol::DaemonEvent::ServerStopping { .. }) && !authenticated(&socket, &id, &secret).await { break; }
                    if !send_frame(&mut ws, &mut codec, 0, IpcPayload::Event(crate::remote_browser::public_event(event))).await { break; }
                }
            }
            message = ws.next() => {
                match message {
                    Some(Ok(Message::Binary(data))) => {
                        let mut offset = 0;
                        while offset < data.len() {
                            let size = (LIMIT - bytes.len()).min(data.len() - offset);
                            if size == 0 { break 'session; }
                            bytes.extend_from_slice(&data[offset..offset + size]);
                            offset += size;
                            loop {
                                let frame = match codec.decode(&mut bytes) {
                                    Ok(Some(frame)) => frame,
                                    Ok(None) => break,
                                    Err(_) => break 'session,
                                };
                                let IpcPayload::Request(request) = frame.payload else { break 'session; };
                                surface = match frame.source {
                                    ClientKind::Cli => "cli", ClientKind::Tui => "tui", ClientKind::Mcp => "mcp",
                                    ClientKind::Agent => "agent", _ => "web",
                                };
                                if !touch(&socket, &id, &secret, &session_id, surface).await && !authenticated(&socket, &id, &secret).await { break 'session; }
                                // Authentication, workspace ownership and the exact operation are checked by the daemon.
                                let response = match request_with_events(&mut connection, &mut ws, &mut codec, &socket, &id, &secret,
                                    Request::RemoteRead { expected_workspace: frame.expected_workspace, client_id: id.clone(), mail_secret: secret.clone(), request: Box::new(request) }).await {
                                    Ok(response) => response, Err(()) => break 'session,
                                };
                                if !send_frame(&mut ws, &mut codec, frame.id, IpcPayload::Response(response)).await { break 'session; }
                            }
                        }
                    }
                    Some(Ok(Message::Ping(data))) => { if ws.send(Message::Pong(data)).await.is_err() { break; } }
                    Some(Ok(Message::Pong(_))) => (),
                    _ => break,
                }
            }
        }
    }
    let _ = call(
        &socket,
        Request::RemoteAuthority {
            request: RemoteRequest::CloseSession {
                client_id: id,
                mail_secret: secret,
                session_id,
            },
        },
    )
    .await;
    let _ = ws.close().await;
}

async fn touch(
    socket: &std::path::Path,
    id: &str,
    secret: &RemoteSecret,
    session: &str,
    surface: &str,
) -> bool {
    matches!(
        call(
            socket,
            Request::RemoteAuthority {
                request: RemoteRequest::TouchSession {
                    client_id: id.into(),
                    mail_secret: secret.clone(),
                    session_id: session.into(),
                    surface: surface.into(),
                }
            }
        )
        .await,
        Ok(Response::Ok { .. })
    )
}

async fn send_frame(
    ws: &mut WebSocket,
    codec: &mut IpcCodec,
    id: u64,
    payload: IpcPayload,
) -> bool {
    let mut bytes = BytesMut::new();
    if codec
        .encode(
            IpcMessage {
                expected_workspace: None,
                id,
                source: ClientKind::Daemon,
                payload,
            },
            &mut bytes,
        )
        .is_err()
    {
        return false;
    }
    ws.send(Message::Binary(bytes.freeze())).await.is_ok()
}

pub async fn serve(socket: PathBuf, address: std::net::SocketAddr) -> Result<(), std::io::Error> {
    if !address.ip().is_loopback() {
        return Err(std::io::Error::other("Remote gateway must bind loopback"));
    }
    let listener = tokio::net::TcpListener::bind(address).await?;
    axum::serve(listener, router(socket)).await
}

/// Correlate a listener with its explicit socket without exposing host filesystem paths.
pub fn profile_id(socket: &std::path::Path) -> String {
    uuid::Uuid::new_v5(
        &uuid::Uuid::NAMESPACE_URL,
        socket.as_os_str().as_encoded_bytes(),
    )
    .to_string()
}
async fn health(State(socket): State<PathBuf>) -> Json<serde_json::Value> {
    Json(
        serde_json::json!({"service":"mxr-remote-gateway","protocol_version":mxr_protocol::IPC_PROTOCOL_VERSION,"profile_id":profile_id(&socket)}),
    )
}
async fn browser_grant(
    State(socket): State<PathBuf>,
    headers: HeaderMap,
    Json(request): Json<RemoteRequest>,
) -> Result<Json<Response>, StatusCode> {
    if headers.contains_key("origin") || !matches!(request, RemoteRequest::IssueBrowserCode { .. })
    {
        return Err(StatusCode::FORBIDDEN);
    }
    public_response(call(&socket, Request::RemoteAuthority { request }).await?)
}

/// The advertised HTTPS origin is fixed by the host operator, never forwarded headers.
pub async fn serve_with_origin(
    socket: PathBuf,
    address: std::net::SocketAddr,
    origin: String,
) -> Result<(), std::io::Error> {
    if !address.ip().is_loopback() {
        return Err(std::io::Error::other("Remote gateway must bind loopback"));
    }
    let browser = crate::remote_browser::router(socket.clone(), &origin)?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    let native = router(socket.clone()).merge(
        Router::new()
            .route("/api/v1/remote/browser-grants", post(browser_grant))
            .with_state(socket),
    );
    axum::serve(listener, native.fallback_service(browser)).await
}

async fn authenticated(socket: &std::path::Path, id: &str, secret: &RemoteSecret) -> bool {
    matches!(
        call(
            socket,
            Request::RemoteAuthority {
                request: RemoteRequest::Authenticate {
                    client_id: id.into(),
                    mail_secret: secret.clone()
                }
            }
        )
        .await,
        Ok(Response::Ok { .. })
    )
}

/// Forward lifecycle events while a provider operation is still in flight. The
/// ordinary event slot and stopping slot each retain at most one IPC frame.
async fn request_with_events(
    connection: &mut IpcConnection,
    ws: &mut WebSocket,
    codec: &mut IpcCodec,
    socket: &std::path::Path,
    id: &str,
    secret: &RemoteSecret,
    request: Request,
) -> Result<Response, ()> {
    use mxr_protocol::DaemonEvent;
    use std::sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    };
    let (events, mut pending) = tokio::sync::mpsc::channel(1);
    let (stopping, mut stop_notice) = tokio::sync::watch::channel(None);
    let skipped = Arc::new(AtomicU64::new(0));
    let dropped = skipped.clone();
    let call = connection.request_response(
        request,
        move |event| {
            if matches!(event, DaemonEvent::ServerStopping { .. }) {
                stopping.send_replace(Some(event));
            } else if events.try_send(event).is_err() {
                dropped.fetch_add(1, Ordering::Relaxed);
            }
        },
        Some(std::time::Duration::from_secs(30)),
    );
    tokio::pin!(call);
    let mut events_open = true;
    let mut stopping_open = true;
    let mut notice_sent = false;
    loop {
        tokio::select! {
            biased;
            changed = stop_notice.changed(), if stopping_open => {
                if changed.is_err() { stopping_open = false; continue; }
                let notice = stop_notice.borrow_and_update().clone();
                if let Some(event) = notice {
                    if !send_frame(ws, codec, 0, IpcPayload::Event(event)).await { return Err(()); }
                    notice_sent = true;
                }
            }
            event = pending.recv(), if events_open => {
                let Some(event) = event else { events_open = false; continue; };
                if !crate::remote_browser::event_visible(id, &event) { continue; }
                if !authenticated(socket, id, secret).await { return Err(()); }
                if !send_frame(ws, codec, 0, IpcPayload::Event(crate::remote_browser::public_event(event))).await { return Err(()); }
            }
            response = &mut call => {
                // A ready response and event may arrive in the same poll. Do not
                // lose a stopping notice merely because the provider also finished.
                if !notice_sent {
                    let notice = stop_notice.borrow_and_update().clone();
                    if let Some(event) = notice {
                        if !send_frame(ws, codec, 0, IpcPayload::Event(event)).await { return Err(()); }
                    }
                }
                while let Ok(event) = pending.try_recv() {
                    if crate::remote_browser::event_visible(id, &event) && authenticated(socket, id, secret).await
                        && !send_frame(ws, codec, 0, IpcPayload::Event(crate::remote_browser::public_event(event))).await { return Err(()); }
                }
                let skipped = skipped.load(Ordering::Relaxed);
                if skipped > 0 && !send_frame(ws, codec, 0, IpcPayload::Event(DaemonEvent::EventsLagged { skipped })).await { return Err(()); }
                return response.map_err(|_| ());
            }
        }
    }
}
