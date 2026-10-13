//! Shared bounded cleanup retries for CLI, TUI and MCP client use.
use crate::ClientError;
use mxr_config::connection_state::{self as persistence, ConnectionState};
use mxr_protocol::{RemoteRequest, Response, ResponseData};
use sha2::Digest;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex,
};
static CLEANUP_TASK: Mutex<Option<tokio::task::JoinHandle<()>>> = Mutex::new(None);
fn error(message: &str) -> ClientError {
    ClientError::Io(std::io::Error::other(message))
}
async fn read() -> Result<ConnectionState, ClientError> {
    tokio::task::spawn_blocking(|| persistence::read(&persistence::path()))
        .await
        .map_err(|_| error("Connection state read was interrupted"))?
        .map_err(Into::into)
}
async fn update(
    mutate: impl FnOnce(&mut ConnectionState) -> std::io::Result<()> + Send + 'static,
) -> Result<(), ClientError> {
    tokio::task::spawn_blocking(move || persistence::update(&persistence::path(), None, mutate))
        .await
        .map_err(|_| error("Connection cleanup commit was interrupted"))??;
    Ok(())
}
fn http() -> Result<reqwest::Client, ClientError> {
    Ok(reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|_| error("Cannot initialize HTTPS client"))?)
}
pub async fn authority(
    origin: &str,
    route: &str,
    request: RemoteRequest,
) -> Result<ResponseData, ClientError> {
    let origin = mxr_transport::https_origin(origin)?;
    let mut response = http()?
        .post(format!("{origin}/api/v1/remote/{route}"))
        .json(&request)
        .send()
        .await
        .map_err(|_| error("Remote HTTPS request failed; saved selection was preserved"))?;
    if !response.status().is_success() {
        return Err(error(
            "Remote pairing/cleanup was denied; inspect host approval and retry",
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| error("Remote response was interrupted"))?
    {
        if bytes.len().saturating_add(chunk.len()) > 16 * 1024 {
            return Err(error("Remote authority response exceeded its limit"));
        }
        bytes.extend_from_slice(&chunk);
    }
    match serde_json::from_slice::<Response>(&bytes)
        .map_err(|_| error("Invalid remote authority response"))?
    {
        Response::Ok { data } => Ok(data),
        _ => Err(error("Remote authority denied")),
    }
}
/// Cleanup never uses the mail credential or targets an inferred registration.
pub async fn retry_cleanup() -> Result<usize, ClientError> {
    let snapshot = read().await?;
    for old in snapshot.retired_pairings {
        let retire = RemoteRequest::AbandonPairing {
            cleanup_secret: old.cleanup_secret.clone(),
        };
        let retired = match authority(&old.origin, "self/deregister", retire.clone()).await {
            Ok(_) => true,
            Err(_) => {
                // Reestablish only an unapproved proof record when start was lost or expired.
                let cleanup_verifier = mxr_core::RemoteSecret::new(base16ct::lower::encode_string(
                    &sha2::Sha256::digest(old.cleanup_secret.expose().as_bytes()),
                ));
                authority(
                    &old.origin,
                    "pairings",
                    RemoteRequest::StartPairing {
                        name: old.name.clone(),
                        capabilities: vec![mxr_core::RemoteCapability::MailRead],
                        mail_verifier: old.mail_verifier.clone(),
                        cleanup_verifier,
                    },
                )
                .await
                .is_ok()
                    && authority(&old.origin, "self/deregister", retire)
                        .await
                        .is_ok()
            }
        };
        if retired {
            update(move |state| {
                state
                    .retired_pairings
                    .retain(|entry| entry.cleanup_secret.expose() != old.cleanup_secret.expose());
                Ok(())
            })
            .await?;
        }
    }
    for old in snapshot.cleanup {
        if authority(
            &old.origin,
            "self/deregister",
            RemoteRequest::Cleanup {
                client_id: old.client_id.clone(),
                cleanup_secret: old.cleanup_secret,
            },
        )
        .await
        .is_ok()
        {
            let origin = old.origin;
            let id = old.client_id;
            update(move |state| {
                state
                    .cleanup
                    .retain(|entry| entry.client_id != id || entry.origin != origin);
                Ok(())
            })
            .await?;
        }
    }
    let state = read().await?;
    Ok(state.cleanup.len() + state.retired_pairings.len())
}

/// Retired credentials are independent of current selection, including environment overrides.
pub(crate) fn schedule_cleanup() {
    static LAST: AtomicU64 = AtomicU64::new(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |value| value.as_secs());
    let previous = LAST.load(Ordering::Relaxed);
    if now.saturating_sub(previous) < 30
        || LAST
            .compare_exchange(previous, now, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
    {
        return;
    }
    let task = tokio::spawn(async {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(10), retry_cleanup()).await;
    });
    *CLEANUP_TASK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(task);
}

/// Short-lived clients finish their scheduled retry after rendering the mail result.
/// Deadline expiry retains every unacknowledged intent for the next client use.
pub async fn finish_cleanup() {
    let task = CLEANUP_TASK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take();
    if let Some(task) = task {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), task).await;
    }
}
