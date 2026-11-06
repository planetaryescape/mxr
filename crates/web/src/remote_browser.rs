//! Same-origin browser authentication. The native proof never reaches browser storage.
use crate::{auth::BridgeError, AppState, WebServerConfig};
use axum::{
    extract::{Form, Request, State},
    http::{header, HeaderValue, Method, StatusCode},
    middleware::Next,
    response::{Html, IntoResponse, Redirect, Response},
    routing::post,
    Router,
};
use mxr_client::{ClientError, IpcConnection};
use mxr_core::RemoteSecret;
use mxr_protocol::{
    ClientKind, RemoteRequest, RemoteResponse, Request as IpcRequest, ResponseData,
};
use serde::Deserialize;
use std::{path::Path, sync::Arc};

const COOKIE: &str = "__Host-mxr-session";
const LOGIN_HEAD: &str = r#"<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>Connect to mxr</title><body><main><h1>Connect to mxr</h1><p>Run <code>mxr web</code> on your paired computer, then enter its single-use code. It expires after two minutes.</p><form method="post" action="/api/v1/remote/browser-session"><label>Connection code <input name="code" type="password" required autocomplete="off" minlength="64" maxlength="64""#;
const LOGIN_TAIL: &str =
    r#"></label><button autofocus>Connect</button></form></main></body></html>"#;

#[derive(Clone)]
pub(super) struct Proof {
    pub id: String,
    pub secret: RemoteSecret,
    pub parent: String,
}
tokio::task_local! { static PROOF: Proof; }
pub(super) fn proof() -> Option<Proof> {
    PROOF.try_with(Clone::clone).ok()
}
pub(super) fn session_id() -> Option<String> {
    proof().map(|p| p.parent)
}

pub(super) async fn connect(path: &Path, source: ClientKind) -> Result<IpcConnection, ClientError> {
    let connection = IpcConnection::connect(path, source).await?;
    Ok(match proof() {
        Some(proof) => connection.with_remote_admission(proof.id, proof.secret),
        None => connection,
    })
}

#[derive(Clone)]
struct BrowserState {
    app: AppState,
    origin: String,
    authority: String,
}

pub(super) fn router(socket: std::path::PathBuf, origin: &str) -> Result<Router, std::io::Error> {
    let url = url::Url::parse(origin).map_err(std::io::Error::other)?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(std::io::Error::other("Browser gateway requires an HTTPS origin without a path, credentials, query or fragment"));
    }
    let origin = url.origin().ascii_serialization();
    let authority = origin.trim_start_matches("https://").to_string();
    // Only middleware can supply this private bridge credential. It is never returned.
    let token = format!(
        "{}{}",
        uuid::Uuid::now_v7().simple(),
        uuid::Uuid::now_v7().simple()
    );
    let config = WebServerConfig::new(socket, token)
        .with_auto_local_token(false)
        .with_host_allowlist(vec![url.host_str().unwrap_or_default().into()]);
    let app = AppState::new(config.clone());
    let state = Arc::new(BrowserState {
        app: app.clone(),
        origin,
        authority,
    });
    // Use one AppState for compose locks and sessions, shared across requests.
    Ok(crate::router::app_with_state(app)
        .route(
            "/api/v1/remote/browser-session",
            post(claim).with_state(state.clone()),
        )
        .layer(axum::middleware::from_fn_with_state(state, authorize))
        .layer(axum::middleware::from_fn(browser_security_headers)))
}

async fn browser_security_headers(request: Request, next: Next) -> Response {
    let mut response = crate::middleware::security_headers(request, next).await;
    // Fetch serializes a form POST's Origin as null under no-referrer. Keep
    // same-origin form login compatible with the exact Origin admission check.
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("same-origin"),
    );
    response
}

fn cookie(request: &Request) -> Option<Proof> {
    let mut matches = request
        .headers()
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|h| h.to_str().ok())
        .flat_map(|h| h.split(';'))
        .filter_map(|part| part.trim().split_once('='))
        .filter(|(name, _)| *name == COOKIE);
    let (_, value) = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    let (id, secret) = value.rsplit_once(':')?;
    let uuid = id.strip_prefix("browser:")?;
    if uuid::Uuid::parse_str(uuid).is_err() || !valid_secret(secret) {
        return None;
    }
    Some(Proof {
        id: id.into(),
        secret: RemoteSecret::new(secret.into()),
        parent: String::new(),
    })
}
fn valid_secret(secret: &str) -> bool {
    secret.len() == 64
        && secret
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
async fn authority(socket: &Path, request: RemoteRequest) -> Result<ResponseData, StatusCode> {
    let mut connection = IpcConnection::connect_supervised(socket, ClientKind::Web)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    connection
        .request(IpcRequest::RemoteAuthority { request })
        .await
        .map_err(|_| StatusCode::UNAUTHORIZED)
}

async fn authorize(
    State(state): State<Arc<BrowserState>>,
    mut request: Request,
    next: Next,
) -> Response {
    // The proxy must preserve Host. Forwarded headers never establish browser trust.
    if request
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        != Some(state.authority.as_str())
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    let origin = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|h| h.to_str().ok());
    let websocket = request.headers().get(header::UPGRADE).is_some();
    if (request.method() != Method::GET && request.method() != Method::HEAD || websocket)
        && origin != Some(state.origin.as_str())
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    if origin.is_some_and(|origin| origin != state.origin) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if request.method() == Method::GET && request.uri().path() == "/api/v1/remote/browser-launch" {
        return login(launch_code(&request));
    }
    if request.uri().path() == "/api/v1/remote/browser-session" {
        return next.run(request).await;
    }
    if !request.uri().path().starts_with("/api/") {
        // Static assets are public, while the document is the login form until authenticated.
        if let Some(proof) = cookie(&request) {
            if authenticate(&state, &proof).await.is_ok() {
                return next.run(request).await;
            }
        }
        if request.uri().path().starts_with("/assets/") {
            return next.run(request).await;
        }
        return login(None);
    }
    let Some(mut proof) = cookie(&request) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    match authenticate(&state, &proof).await {
        Ok(parent) => proof.parent = parent,
        Err(status) => return status.into_response(),
    }
    let path = request.uri().path();
    let outcome_query = (request.method() == Method::GET
        && (path.starts_with("/api/v1/mail/send-outcomes/")
            || path.starts_with("/api/v1/mail/jobs/")))
        || (request.method() == Method::POST && path == "/api/v1/mail/archive-operations/query");
    if !touch_browser(&state.app.config.socket_path, &proof).await && !outcome_query {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    if request.uri().path() == "/api/v1/auth/local-token" {
        // This public marker only wakes the existing SPA auth state. The cookie is the credential.
        return axum::Json(
            serde_json::json!({"token":"browser-session","source":"cookie-session"}),
        )
        .into_response();
    }
    // Editor shell, filesystem helpers and host administration have no remote authority.
    let path = request.uri().path();
    if path.contains("/attachments/open")
        || path.contains("/attachments/download")
        || path.contains("/admin/diagnostics")
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    request.headers_mut().insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", state.app.config.auth_token))
            .unwrap_or_else(|_| HeaderValue::from_static("invalid")),
    );
    // URL token values are untrusted; handlers prefer the middleware's private header.
    if request.method() != Method::GET
        && request
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|h| h.to_str().ok())
            .is_some_and(is_json)
    {
        let (parts, body) = request.into_parts();
        let bytes = match axum::body::to_bytes(body, 16 * 1024 * 1024).await {
            Ok(bytes) => bytes,
            Err(_) => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
        };
        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
            if let Some(path) = value.get("draft_path").and_then(|p| p.as_str()) {
                if validate_compose_path(Path::new(path), &proof.parent)
                    .await
                    .is_err()
                {
                    return StatusCode::FORBIDDEN.into_response();
                }
            }
        }
        request = Request::from_parts(parts, axum::body::Body::from(bytes));
    }
    let mut response = PROOF.scope(proof, next.run(request)).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
async fn authenticate(state: &BrowserState, proof: &Proof) -> Result<String, StatusCode> {
    match authority(
        &state.app.config.socket_path,
        RemoteRequest::Authenticate {
            client_id: proof.id.clone(),
            mail_secret: proof.secret.clone(),
        },
    )
    .await?
    {
        ResponseData::RemoteAuthority {
            response: RemoteResponse::Client { client, .. },
        } => Ok(client.client_id),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

#[derive(Deserialize)]
struct Claim {
    code: String,
}
async fn claim(
    State(state): State<Arc<BrowserState>>,
    headers: axum::http::HeaderMap,
    Form(form): Form<Claim>,
) -> Response {
    if !valid_secret(&form.code) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match authority(
        &state.app.config.socket_path,
        RemoteRequest::ClaimBrowserCode {
            code: RemoteSecret::new(form.code),
            replace_session: {
                let mut request = Request::new(axum::body::Body::empty());
                *request.headers_mut() = headers;
                cookie(&request).map(|proof| (proof.id, proof.secret))
            },
        },
    )
    .await
    {
        Ok(ResponseData::RemoteAuthority {
            response:
                RemoteResponse::BrowserSession {
                    session_id,
                    mail_secret,
                    expires_at,
                },
        }) => {
            let age = (expires_at - chrono::Utc::now().timestamp()).max(0);
            let value = format!("{COOKIE}={session_id}:{}; Path=/; Secure; HttpOnly; SameSite=Strict; Max-Age={age}", mail_secret.expose());
            let mut response = Redirect::to("/").into_response();
            if let Ok(value) = HeaderValue::from_str(&value) {
                response.headers_mut().insert(header::SET_COOKIE, value);
            }
            response
                .headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            response
        }
        Err(status) => status.into_response(),
        _ => StatusCode::UNAUTHORIZED.into_response(),
    }
}

pub(super) async fn status(socket: &Path) -> Result<ResponseData, BridgeError> {
    let mut connection = connect(socket, ClientKind::Web)
        .await
        .map_err(crate::map_bridge_error)?;
    let ready = connection
        .request(IpcRequest::GetRemoteReady)
        .await
        .map_err(crate::map_bridge_error)?;
    let ResponseData::RemoteReady {
        accepting_mail,
        protocol_version,
        draft_revision_supported,
        send_outcome_supported,
        attachment_transfer_supported,
        draft_attachment_upload_supported,
        capabilities,
        ..
    } = ready
    else {
        return Err(BridgeError::UnexpectedResponse);
    };
    if !accepting_mail {
        return Err(BridgeError::Connect(
            "Remote host is stopping; retain drafts and reconnect when it returns".into(),
        ));
    }
    let accounts = match connection
        .request(IpcRequest::ListAccounts)
        .await
        .map_err(crate::map_bridge_error)?
    {
        ResponseData::Accounts { accounts } => {
            accounts.iter().map(|a| a.name.clone()).collect::<Vec<_>>()
        }
        _ => return Err(BridgeError::UnexpectedResponse),
    };
    let total = match connection
        .request(IpcRequest::Count {
            query: String::new(),
            account_id: None,
            mode: None,
        })
        .await
        .map_err(crate::map_bridge_error)?
    {
        ResponseData::Count { count } => count,
        _ => return Err(BridgeError::UnexpectedResponse),
    };
    serde_json::from_value(serde_json::json!({"kind":"Status","uptime_secs":0,"accounts":accounts,"total_messages":total,
        "protocol_version":protocol_version,"workspace_owner_supported":true,"draft_revision_supported":draft_revision_supported,
        "send_outcome_supported":send_outcome_supported,"attachment_transfer_supported":attachment_transfer_supported,
        "draft_attachment_upload_supported":draft_attachment_upload_supported,"archive_recovery_supported":capabilities.contains(&mxr_core::RemoteCapability::MailTriage)}))
        .map_err(|_| BridgeError::UnexpectedResponse)
}

/// A browser can address only scratch files minted for its parent registration.
pub(super) async fn validate_compose_path(path: &Path, parent: &str) -> Result<(), ()> {
    let root = mxr_compose::private_tmp::private_scratch_dir().map_err(|_| ())?;
    let root = tokio::fs::canonicalize(root).await.map_err(|_| ())?;
    let actual = tokio::fs::canonicalize(path).await.map_err(|_| ())?;
    if !actual.starts_with(root) {
        return Err(());
    }
    let bytes = tokio::fs::read(crate::draft_session_metadata_path(&actual))
        .await
        .map_err(|_| ())?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| ())?;
    if value.get("compose_browser").and_then(|v| v.as_str()) != Some(parent) {
        return Err(());
    }
    Ok(())
}

pub(super) async fn validate_current(socket: &Path) -> bool {
    let Some(proof) = proof() else {
        return true;
    };
    let valid = authority(
        socket,
        RemoteRequest::Authenticate {
            client_id: proof.id.clone(),
            mail_secret: proof.secret.clone(),
        },
    )
    .await
    .is_ok();
    if valid {
        let _ = touch_browser(socket, &proof).await;
    }
    valid
}

pub(super) async fn events(
    socket: axum::extract::ws::WebSocket,
    path: std::path::PathBuf,
    proof: Option<Proof>,
) {
    match proof {
        Some(proof) => PROOF.scope(proof, crate::bridge_events(socket, path)).await,
        None => crate::bridge_events(socket, path).await,
    }
}

#[cfg(test)]
mod tests;

fn is_json(content_type: &str) -> bool {
    let mime = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    mime == "application/json" || (mime.starts_with("application/") && mime.ends_with("+json"))
}

/// Remote event messages describe outcomes without host diagnostics or filesystem paths.
pub(super) fn public_event(mut event: mxr_protocol::DaemonEvent) -> mxr_protocol::DaemonEvent {
    use mxr_protocol::DaemonEvent;
    match &mut event {
        DaemonEvent::SyncError { error, .. } | DaemonEvent::OperationFailed { error, .. } => {
            *error = "Remote operation failed; inspect host diagnostics".into();
        }
        DaemonEvent::OperationStarted { message, .. }
        | DaemonEvent::OperationProgress { message, .. }
        | DaemonEvent::OperationCompleted { message, .. }
        | DaemonEvent::OperationCancelled { message, .. } => {
            *message = "Remote operation state changed".into();
        }
        DaemonEvent::MutationReconciliationFailed { error_summary, .. } => {
            *error_summary =
                "Remote change could not be reconciled; refresh mail and inspect host diagnostics"
                    .into();
        }
        _ => (),
    }
    event
}

async fn touch_browser(socket: &Path, proof: &Proof) -> bool {
    let Some(session_id) = proof.id.strip_prefix("browser:") else {
        return false;
    };
    authority(
        socket,
        RemoteRequest::TouchSession {
            client_id: proof.id.clone(),
            mail_secret: proof.secret.clone(),
            session_id: session_id.into(),
            surface: "web".into(),
        },
    )
    .await
    .is_ok()
}

pub(super) fn event_visible(parent: &str, event: &mxr_protocol::DaemonEvent) -> bool {
    match event {
        mxr_protocol::DaemonEvent::AssistantUpdated {
            conversation_id, ..
        } => conversation_id.starts_with(&format!("asst_remote_{parent}_")),
        _ => true,
    }
}

fn login(prefill: Option<&str>) -> Response {
    // `mxr web` can hand the code over in the launch URL so nobody has to retype
    // 64 characters. The code stays single-use and two-minute bound, and the page
    // keeps its own CSP: no script, the field is only pre-filled.
    let value = prefill
        .filter(|code| {
            code.len() == 64 && code.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
        .map(|code| format!(" value=\"{code}\""))
        .unwrap_or_default();
    let body = format!("{LOGIN_HEAD}{value}{LOGIN_TAIL}");
    (
        [
            (
                header::CONTENT_SECURITY_POLICY,
                "default-src 'none'; form-action 'self'; frame-ancestors 'none'; base-uri 'none'",
            ),
            (header::CACHE_CONTROL, "no-store"),
        ],
        Html(body),
    )
        .into_response()
}

/// The `code` query parameter of a launch URL, if the request carries one.
fn launch_code(request: &Request) -> Option<&str> {
    request.uri().query()?.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        (name == "code").then_some(value)
    })
}
