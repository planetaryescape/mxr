use axum::{
    extract::{Path as AxumPath, Query, State},
    http::HeaderMap,
    http::StatusCode,
    response::{IntoResponse, Response},
    extract::ws::{Message as WebSocketMessage, WebSocket, WebSocketUpgrade},
    routing::{get, post},
    Json, Router,
};
use futures::{SinkExt, StreamExt};
use mxr_core::{
    id::{MessageId, ThreadId},
    types::SearchMode,
};
use mxr_protocol::{IpcCodec, IpcMessage, IpcPayload, Request, ResponseData};
use serde::Deserialize;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use tokio::net::TcpListener;
use tokio::net::UnixStream;
use tokio_util::codec::Framed;
use tower_http::cors::CorsLayer;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct WebServerConfig {
    pub socket_path: PathBuf,
    pub auth_token: String,
}

impl WebServerConfig {
    pub fn new(socket_path: PathBuf, auth_token: String) -> Self {
        Self {
            socket_path,
            auth_token,
        }
    }
}

pub fn app(_config: WebServerConfig) -> Router {
    Router::new()
        .route("/status", get(status))
        .route("/mailbox", get(mailbox))
        .route("/search", get(search))
        .route("/thread/{thread_id}", get(thread))
        .route("/mutations/archive", post(archive))
        .route("/events", get(events))
        .with_state(AppState::new(_config))
        .layer(CorsLayer::permissive())
}

pub async fn serve(listener: TcpListener, config: WebServerConfig) -> std::io::Result<()> {
    axum::serve(listener, app(config)).await
}

pub async fn bind_and_serve(
    host: std::net::IpAddr,
    port: u16,
    config: WebServerConfig,
) -> std::io::Result<SocketAddr> {
    let listener = TcpListener::bind((host, port)).await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        let _ = serve(listener, config).await;
    });
    Ok(addr)
}

#[derive(Clone)]
struct AppState {
    config: WebServerConfig,
}

impl AppState {
    fn new(config: WebServerConfig) -> Self {
        Self { config }
    }
}

#[derive(Debug, thiserror::Error)]
enum BridgeError {
    #[error("failed to connect to mxr daemon at {0}")]
    Connect(String),
    #[error("ipc error: {0}")]
    Ipc(String),
    #[error("unauthorized")]
    Unauthorized,
    #[error("unexpected response from daemon")]
    UnexpectedResponse,
}

impl IntoResponse for BridgeError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            _ => StatusCode::BAD_GATEWAY,
        };
        (status, Json(serde_json::json!({ "error": self.to_string() }))).into_response()
    }
}

#[derive(Debug, Default, Deserialize)]
struct AuthQuery {
    #[serde(default)]
    token: Option<String>,
}

fn ensure_authorized(
    headers: &HeaderMap,
    query_token: Option<&str>,
    expected_token: &str,
) -> Result<(), BridgeError> {
    let header_token = headers
        .get("x-mxr-bridge-token")
        .and_then(|value| value.to_str().ok());
    let provided = header_token.or(query_token);
    if provided == Some(expected_token) {
        return Ok(());
    }
    Err(BridgeError::Unauthorized)
}

async fn status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(&state.config.socket_path, Request::GetStatus).await? {
        ResponseData::Status {
            uptime_secs,
            accounts,
            total_messages,
            daemon_pid,
            sync_statuses,
            protocol_version,
            daemon_version,
            daemon_build_id,
            repair_required,
        } => Ok(Json(serde_json::json!({
            "uptime_secs": uptime_secs,
            "accounts": accounts,
            "total_messages": total_messages,
            "daemon_pid": daemon_pid,
            "sync_statuses": sync_statuses,
            "protocol_version": protocol_version,
            "daemon_version": daemon_version,
            "daemon_build_id": daemon_build_id,
            "repair_required": repair_required,
        }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

#[derive(Debug, Default, Deserialize)]
struct MailboxQuery {
    #[serde(default = "default_limit")]
    limit: u32,
    #[serde(default)]
    offset: u32,
    #[serde(default)]
    token: Option<String>,
}

fn default_limit() -> u32 {
    50
}

#[derive(Debug, Deserialize)]
struct SearchQuery {
    q: String,
    #[serde(default = "default_limit")]
    limit: u32,
    #[serde(default)]
    mode: Option<SearchMode>,
    #[serde(default)]
    explain: bool,
    #[serde(default)]
    token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ArchiveRequest {
    message_ids: Vec<String>,
}

async fn mailbox(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<MailboxQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, query.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(
        &state.config.socket_path,
        Request::ListEnvelopes {
            label_id: None,
            account_id: None,
            limit: query.limit,
            offset: query.offset,
        },
    )
    .await?
    {
        ResponseData::Envelopes { envelopes } => Ok(Json(serde_json::json!({
            "envelopes": envelopes,
        }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn thread(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    AxumPath(thread_id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let thread_id = parse_thread_id(&thread_id)?;
    match ipc_request(&state.config.socket_path, Request::GetThread { thread_id }).await? {
        ResponseData::Thread { thread, messages } => {
            let bodies = match ipc_request(
                &state.config.socket_path,
                Request::ListBodies {
                    message_ids: messages
                        .iter()
                        .map(|message| message.id.clone())
                        .collect::<Vec<MessageId>>(),
                },
            )
            .await?
            {
                ResponseData::Bodies { bodies } => bodies,
                _ => return Err(BridgeError::UnexpectedResponse),
            };

            Ok(Json(serde_json::json!({
                "thread": thread,
                "messages": messages,
                "bodies": bodies,
            })))
        }
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn search(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<SearchQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, query.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(
        &state.config.socket_path,
        Request::Search {
            query: query.q,
            limit: query.limit,
            mode: query.mode,
            explain: query.explain,
        },
    )
    .await?
    {
        ResponseData::SearchResults { results, explain } => Ok(Json(serde_json::json!({
            "results": results,
            "explain": explain,
        }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn archive(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<ArchiveRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let message_ids = request
        .message_ids
        .iter()
        .map(|value| parse_message_id(value))
        .collect::<Result<Vec<_>, _>>()?;

    match ipc_request(
        &state.config.socket_path,
        Request::Mutation(mxr_protocol::MutationCommand::Archive { message_ids }),
    )
    .await?
    {
        ResponseData::Ack => Ok(Json(serde_json::json!({ "ok": true }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn events(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
) -> impl IntoResponse {
    if let Err(error) = ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)
    {
        return error.into_response();
    }
    ws.on_upgrade(move |socket| bridge_events(socket, state.config.socket_path))
}

async fn ipc_request(socket_path: &Path, request: Request) -> Result<ResponseData, BridgeError> {
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
                IpcPayload::Response(mxr_protocol::Response::Ok { data }) => return Ok(data),
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

async fn bridge_events(mut socket: WebSocket, socket_path: PathBuf) {
    let stream = match UnixStream::connect(&socket_path).await {
        Ok(stream) => stream,
        Err(error) => {
            let _ = socket
                .send(WebSocketMessage::Text(
                    serde_json::json!({ "error": error.to_string() }).to_string().into(),
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

fn parse_thread_id(value: &str) -> Result<ThreadId, BridgeError> {
    Uuid::parse_str(value)
        .map(ThreadId::from_uuid)
        .map_err(|_| BridgeError::Ipc(format!("invalid thread id: {value}")))
}

fn parse_message_id(value: &str) -> Result<MessageId, BridgeError> {
    Uuid::parse_str(value)
        .map(MessageId::from_uuid)
        .map_err(|_| BridgeError::Ipc(format!("invalid message id: {value}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::{SinkExt, StreamExt};
    use chrono::Utc;
    use mxr_core::{
        id::{AccountId, MessageId, ThreadId},
        types::{Address, Envelope, MessageBody, MessageFlags, MessageMetadata, Thread, UnsubscribeMethod},
    };
    use mxr_protocol::{
        DaemonEvent, IpcCodec, IpcMessage, IpcPayload, Request, Response, ResponseData,
        SearchResultItem,
        IPC_PROTOCOL_VERSION,
    };
    use tempfile::TempDir;
    use tokio::net::UnixListener;
    use tokio_tungstenite::tungstenite::Message;
    use tokio_util::codec::Framed;

    const TEST_AUTH_TOKEN: &str = "test-token";

    async fn spawn_fake_ipc_server<F>(
        socket_path: &std::path::Path,
        responder: F,
        event: Option<DaemonEvent>,
    ) -> tokio::task::JoinHandle<()>
    where
        F: Fn(Request) -> Option<Response> + Send + Sync + 'static,
    {
        let responder = std::sync::Arc::new(responder);
        let listener = UnixListener::bind(socket_path).unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let responder = responder.clone();
                let event = event.clone();
                tokio::spawn(async move {
                    let mut framed = Framed::new(stream, IpcCodec::new());
                    if let Some(event) = event {
                        let _ = framed
                            .send(IpcMessage {
                                id: 0,
                                payload: IpcPayload::Event(event),
                            })
                            .await;
                        return;
                    }
                    while let Some(Ok(message)) = framed.next().await {
                        if let IpcPayload::Request(request) = message.payload {
                            let Some(response) = responder(request) else {
                                continue;
                            };
                            let response = IpcMessage {
                                id: message.id,
                                payload: IpcPayload::Response(response),
                            };
                            let _ = framed.send(response).await;
                        }
                    }
                });
            }
        })
    }

    async fn spawn_fake_event_server(
        socket_path: &std::path::Path,
    ) -> tokio::task::JoinHandle<()> {
        spawn_fake_ipc_server(
            socket_path,
            |_| None,
            Some(DaemonEvent::SyncCompleted {
                account_id: AccountId::new(),
                messages_synced: 3,
            }),
        )
        .await
    }

    #[tokio::test]
    async fn status_endpoint_proxies_ipc_status() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let _ipc = spawn_fake_ipc_server(
            &socket_path,
            |request| match request {
                Request::GetStatus => Some(Response::Ok {
                    data: ResponseData::Status {
                        uptime_secs: 42,
                        accounts: vec!["personal".into()],
                        total_messages: 17,
                        daemon_pid: Some(999),
                        sync_statuses: Vec::new(),
                        protocol_version: IPC_PROTOCOL_VERSION,
                        daemon_version: Some("0.4.3".into()),
                        daemon_build_id: Some("build-123".into()),
                        repair_required: false,
                    },
                }),
                _ => None,
            },
            None,
        )
        .await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let response = reqwest::Client::new()
            .get(format!("http://{addr}/status"))
            .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);

        let json: serde_json::Value = response.json().await.unwrap();
        assert_eq!(json["protocol_version"], IPC_PROTOCOL_VERSION);
        assert_eq!(json["daemon_version"], "0.4.3");
        assert_eq!(json["total_messages"], 17);
    }

    #[tokio::test]
    async fn websocket_events_proxy_daemon_events() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let _ipc = spawn_fake_event_server(&socket_path).await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let (mut stream, _) =
            tokio_tungstenite::connect_async(format!("ws://{addr}/events?token={TEST_AUTH_TOKEN}"))
                .await
                .unwrap();
        let message = stream.next().await.unwrap().unwrap();
        let text = match message {
            Message::Text(text) => text.to_string(),
            other => panic!("expected text websocket frame, got {other:?}"),
        };

        assert!(text.contains("SyncCompleted"));
        assert!(text.contains("\"messages_synced\":3"));
    }

    #[tokio::test]
    async fn status_endpoint_rejects_missing_token() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let _ipc = spawn_fake_ipc_server(
            &socket_path,
            |_request| Some(Response::Ok {
                data: ResponseData::Ack,
            }),
            None,
        )
        .await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let response = reqwest::get(format!("http://{addr}/status")).await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
    }

    fn sample_envelope() -> Envelope {
        Envelope {
            id: MessageId::new(),
            account_id: AccountId::new(),
            provider_id: "provider-msg-1".into(),
            thread_id: ThreadId::new(),
            message_id_header: Some("<msg-1@example.com>".into()),
            in_reply_to: None,
            references: Vec::new(),
            from: Address {
                name: Some("Sender".into()),
                email: "sender@example.com".into(),
            },
            to: vec![Address {
                name: Some("User".into()),
                email: "user@example.com".into(),
            }],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: "Mailroom".into(),
            date: Utc::now(),
            flags: MessageFlags::empty(),
            snippet: "Preview".into(),
            has_attachments: false,
            size_bytes: 128,
            unsubscribe: UnsubscribeMethod::None,
            label_provider_ids: Vec::new(),
        }
    }

    fn sample_thread(envelope: &Envelope) -> Thread {
        Thread {
            id: envelope.thread_id.clone(),
            account_id: envelope.account_id.clone(),
            subject: envelope.subject.clone(),
            participants: vec![envelope.from.clone()],
            message_count: 1,
            unread_count: 1,
            latest_date: envelope.date,
            snippet: envelope.snippet.clone(),
        }
    }

    fn sample_body(envelope: &Envelope) -> MessageBody {
        MessageBody {
            message_id: envelope.id.clone(),
            text_plain: Some("plain text".into()),
            text_html: Some("<p>rich html</p>".into()),
            attachments: Vec::new(),
            fetched_at: Utc::now(),
            metadata: MessageMetadata::default(),
        }
    }

    #[tokio::test]
    async fn mailbox_endpoint_lists_envelopes() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let expected = sample_envelope();
        let expected_id = expected.id.to_string();
        let _ipc = spawn_fake_ipc_server(
            &socket_path,
            move |request| match request {
                Request::ListEnvelopes {
                    limit: 50,
                    offset: 0,
                    label_id: None,
                    account_id: None,
                } => Some(Response::Ok {
                    data: ResponseData::Envelopes {
                        envelopes: vec![expected.clone()],
                    },
                }),
                _ => None,
            },
            None,
        )
        .await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let response = reqwest::Client::new()
            .get(format!("http://{addr}/mailbox"))
            .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);

        let json: serde_json::Value = response.json().await.unwrap();
        assert_eq!(json["envelopes"][0]["id"], expected_id);
        assert_eq!(json["envelopes"][0]["subject"], "Mailroom");
    }

    #[tokio::test]
    async fn thread_endpoint_returns_messages_and_bodies() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let envelope = sample_envelope();
        let thread = sample_thread(&envelope);
        let body = sample_body(&envelope);
        let thread_id = thread.id.to_string();
        let message_id = envelope.id.to_string();
        let _ipc = spawn_fake_ipc_server(
            &socket_path,
            move |request| match request {
                Request::GetThread { thread_id: requested } if requested == thread.id => {
                    Some(Response::Ok {
                        data: ResponseData::Thread {
                            thread: thread.clone(),
                            messages: vec![envelope.clone()],
                        },
                    })
                }
                Request::ListBodies { message_ids } if message_ids == vec![body.message_id.clone()] => {
                    Some(Response::Ok {
                        data: ResponseData::Bodies {
                            bodies: vec![body.clone()],
                        },
                    })
                }
                _ => None,
            },
            None,
        )
        .await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let response = reqwest::Client::new()
            .get(format!("http://{addr}/thread/{thread_id}"))
            .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);

        let json: serde_json::Value = response.json().await.unwrap();
        assert_eq!(json["thread"]["id"], thread_id);
        assert_eq!(json["messages"][0]["id"], message_id);
        assert_eq!(json["bodies"][0]["text_html"], "<p>rich html</p>");
    }

    #[tokio::test]
    async fn search_endpoint_proxies_results() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let envelope = sample_envelope();
        let result = SearchResultItem {
            message_id: envelope.id.clone(),
            account_id: envelope.account_id.clone(),
            thread_id: envelope.thread_id.clone(),
            score: 9.5,
            mode: mxr_core::types::SearchMode::Lexical,
        };
        let message_id = result.message_id.to_string();
        let _ipc = spawn_fake_ipc_server(
            &socket_path,
            move |request| match request {
                Request::Search {
                    query,
                    limit: 50,
                    mode: None,
                    explain: false,
                } if query == "buildkite" => Some(Response::Ok {
                    data: ResponseData::SearchResults {
                        results: vec![result.clone()],
                        explain: None,
                    },
                }),
                _ => None,
            },
            None,
        )
        .await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let response = reqwest::Client::new()
            .get(format!("http://{addr}/search?q=buildkite"))
            .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);

        let json: serde_json::Value = response.json().await.unwrap();
        assert_eq!(json["results"][0]["message_id"], message_id);
        assert_eq!(json["results"][0]["score"], 9.5);
        assert!(json["explain"].is_null());
    }

    #[tokio::test]
    async fn archive_mutation_endpoint_proxies_message_ids() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let expected = sample_envelope();
        let expected_id = expected.id.to_string();
        let captured = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let captured_ids = captured.clone();
        let _ipc = spawn_fake_ipc_server(
            &socket_path,
            move |request| match request {
                Request::Mutation(mxr_protocol::MutationCommand::Archive { message_ids }) => {
                    *captured_ids.lock().unwrap() =
                        message_ids.iter().map(ToString::to_string).collect();
                    Some(Response::Ok {
                        data: ResponseData::Ack,
                    })
                }
                _ => None,
            },
            None,
        )
        .await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let client = reqwest::Client::new();
        let response = client
            .post(format!("http://{addr}/mutations/archive"))
            .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
            .json(&serde_json::json!({ "message_ids": [expected_id] }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);

        let json: serde_json::Value = response.json().await.unwrap();
        assert_eq!(json["ok"], true);
        assert_eq!(*captured.lock().unwrap(), vec![expected.id.to_string()]);
    }
}
