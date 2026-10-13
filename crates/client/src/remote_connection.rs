//! Shared bounded cleanup retries for CLI, TUI and MCP client use.
use crate::ClientError;
use futures::{Stream, StreamExt};
use mxr_config::connection_state::{self as persistence, ConnectionState};
use mxr_protocol::{RemoteRequest, Response, ResponseData};
use sha2::Digest;
use std::future::Future;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex,
};
const CLEANUP_BUDGET: std::time::Duration = std::time::Duration::from_secs(5);
const CLEANUP_CONCURRENCY: usize = 4;
static CLEANUP_TASK: Mutex<Option<tokio::task::JoinHandle<()>>> = Mutex::new(None);

enum CleanupIntent {
    Pairing(mxr_config::connection_state::RetiredPairing),
    Registration(mxr_config::connection_state::RetiredRegistration),
}
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
    let deadline = tokio::time::Instant::now() + CLEANUP_BUDGET;
    let snapshot = read().await?;
    let work = futures::stream::iter(
        snapshot
            .retired_pairings
            .into_iter()
            .map(CleanupIntent::Pairing)
            .chain(
                snapshot
                    .cleanup
                    .into_iter()
                    .map(CleanupIntent::Registration),
            ),
    )
    .map(|intent| async move {
        match intent {
            CleanupIntent::Pairing(old) => retire_pairing(old, deadline).await,
            CleanupIntent::Registration(old) => retire_registration(old, deadline).await,
        }
    });
    run_cleanup_tasks(work, deadline).await?;
    let state = read().await?;
    Ok(state.cleanup.len() + state.retired_pairings.len())
}

async fn run_cleanup_tasks<S, F>(work: S, deadline: tokio::time::Instant) -> Result<(), ClientError>
where
    S: Stream<Item = F>,
    F: Future<Output = Result<(), ClientError>>,
{
    let mut work = Box::pin(work.buffer_unordered(CLEANUP_CONCURRENCY));
    loop {
        match tokio::time::timeout_at(deadline, work.next()).await {
            Ok(Some(result)) => result?,
            Ok(None) | Err(_) => break,
        }
    }
    Ok(())
}

async fn retire_pairing(
    old: mxr_config::connection_state::RetiredPairing,
    deadline: tokio::time::Instant,
) -> Result<(), ClientError> {
    let retire = RemoteRequest::AbandonPairing {
        cleanup_secret: old.cleanup_secret.clone(),
    };
    let first = match tokio::time::timeout_at(
        deadline,
        authority(&old.origin, "self/deregister", retire.clone()),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => return Ok(()),
    };
    let retired = match first {
        Ok(_) => true,
        Err(_) => {
            // Reestablish only an unapproved proof record when start was lost or expired.
            let cleanup_verifier = mxr_core::RemoteSecret::new(base16ct::lower::encode_string(
                &sha2::Sha256::digest(old.cleanup_secret.expose().as_bytes()),
            ));
            let restarted = match tokio::time::timeout_at(
                deadline,
                authority(
                    &old.origin,
                    "pairings",
                    RemoteRequest::StartPairing {
                        name: old.name.clone(),
                        capabilities: vec![mxr_core::RemoteCapability::MailRead],
                        mail_verifier: old.mail_verifier.clone(),
                        cleanup_verifier,
                    },
                ),
            )
            .await
            {
                Ok(result) => result.is_ok(),
                Err(_) => return Ok(()),
            };
            if !restarted {
                false
            } else {
                match tokio::time::timeout_at(
                    deadline,
                    authority(&old.origin, "self/deregister", retire),
                )
                .await
                {
                    Ok(result) => result.is_ok(),
                    Err(_) => return Ok(()),
                }
            }
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
    Ok(())
}

async fn retire_registration(
    old: mxr_config::connection_state::RetiredRegistration,
    deadline: tokio::time::Instant,
) -> Result<(), ClientError> {
    let acknowledged = match tokio::time::timeout_at(
        deadline,
        authority(
            &old.origin,
            "self/deregister",
            RemoteRequest::Cleanup {
                client_id: old.client_id.clone(),
                cleanup_secret: old.cleanup_secret,
            },
        ),
    )
    .await
    {
        Ok(result) => result.is_ok(),
        Err(_) => false,
    };
    if acknowledged {
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
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;
    use std::sync::{atomic::AtomicBool, Arc};

    #[tokio::test]
    async fn unreachable_cleanup_does_not_starve_a_later_reachable_intent() {
        let reachable_ran = Arc::new(AtomicBool::new(false));
        let unreachable: futures::future::BoxFuture<'static, Result<(), ClientError>> =
            Box::pin(async { std::future::pending().await });
        let reachable_flag = reachable_ran.clone();
        let reachable: futures::future::BoxFuture<'static, Result<(), ClientError>> =
            Box::pin(async move {
                reachable_flag.store(true, Ordering::SeqCst);
                Ok(())
            });
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(50);

        run_cleanup_tasks(stream::iter([unreachable, reachable]), deadline)
            .await
            .unwrap();

        assert!(reachable_ran.load(Ordering::SeqCst));
    }
}
