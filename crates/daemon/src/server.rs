use crate::handler::handle_request;
use crate::loops;
use crate::state::AppState;
use futures::{SinkExt, StreamExt};
use mxr_protocol::{IpcCodec, IpcMessage};
use std::sync::Arc;
use tokio::net::UnixListener;
use tokio::sync::mpsc;
use tokio_util::codec::Framed;

pub async fn run_daemon() -> anyhow::Result<()> {
    let sock_path = AppState::socket_path();
    if let Some(parent) = sock_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match inspect_socket_state(&sock_path).await {
        SocketState::Reachable => {
            anyhow::bail!(
                "Daemon already running at {}. Try `mxr status`, `mxr logs --level error`, or `mxr daemon --foreground`.",
                sock_path.display()
            );
        }
        SocketState::Stale => {
            let _ = std::fs::remove_file(&sock_path);
        }
        SocketState::Missing => {}
    }

    let state = Arc::new(match AppState::new().await {
        Ok(state) => state,
        Err(error) if is_index_lock_error(&error.to_string()) => {
            anyhow::bail!(
                "Search index is locked by another process. Try `mxr status`, `mxr logs --level error`, or `mxr daemon --foreground`.\nOriginal error: {error}"
            );
        }
        Err(error) => return Err(error),
    });

    let listener = UnixListener::bind(&sock_path)?;
    tracing::info!("Daemon listening on {}", sock_path.display());

    // Reindex search if empty but DB has messages
    {
        let search = state.search.lock().await;
        let test_results = search.search("*", 1);
        let index_empty = test_results.map(|r| r.is_empty()).unwrap_or(true);
        drop(search);

        if index_empty {
            let accounts = state.store.list_accounts().await.unwrap_or_default();
            for account in &accounts {
                let envelopes = state
                    .store
                    .list_envelopes_by_account(&account.id, 10000, 0)
                    .await
                    .unwrap_or_default();
                if !envelopes.is_empty() {
                    tracing::info!(
                        "Reindexing {} existing messages for {}",
                        envelopes.len(),
                        account.email
                    );
                    let mut search = state.search.lock().await;
                    for env in &envelopes {
                        let _ = search.index_envelope(env);
                    }
                    let _ = search.commit();
                }
            }
        }
    }

    // All syncing happens in the background sync loops — no blocking initial sync.
    // The daemon starts accepting clients immediately. The sync loops detect
    // Initial/GmailBackfill cursors and handles them with no startup delay.

    // Spawn background loops
    loops::spawn_sync_loops(state.clone());

    let snooze_state = state.clone();
    tokio::spawn(async move {
        loops::snooze_loop(snooze_state).await;
    });

    // Accept connections
    loop {
        let (stream, _addr) = listener.accept().await?;
        let state = state.clone();
        let mut event_rx = state.event_tx.subscribe();

        tokio::spawn(async move {
            let (mut sink, mut stream) = Framed::new(stream, IpcCodec::new()).split();
            let (resp_tx, mut resp_rx) = mpsc::unbounded_channel::<IpcMessage>();

            loop {
                tokio::select! {
                    msg = stream.next() => {
                        match msg {
                            Some(Ok(ipc_msg)) => {
                                // Spawn handler as a task — requests run concurrently
                                let state = state.clone();
                                let resp_tx = resp_tx.clone();
                                tokio::spawn(async move {
                                    let response = handle_request(&state, &ipc_msg).await;
                                    let _ = resp_tx.send(response);
                                });
                            }
                            Some(Err(e)) => {
                                tracing::error!("IPC decode error: {}", e);
                                break;
                            }
                            None => break,
                        }
                    }
                    resp = resp_rx.recv() => {
                        if let Some(resp_msg) = resp {
                            if sink.send(resp_msg).await.is_err() {
                                break;
                            }
                        }
                    }
                    event = event_rx.recv() => {
                        if let Ok(event_msg) = event {
                            if sink.send(event_msg).await.is_err() {
                                break;
                            }
                        }
                    }
                }
            }

            tracing::debug!("Client disconnected");
        });
    }
}

pub async fn ensure_daemon_running() -> anyhow::Result<()> {
    let sock_path = AppState::socket_path();

    match inspect_socket_state(&sock_path).await {
        SocketState::Reachable => return Ok(()),
        SocketState::Stale => {
            let _ = std::fs::remove_file(&sock_path);
        }
        SocketState::Missing => {}
    }

    eprint!("Starting daemon...");

    let exe = std::env::current_exe()?;
    std::process::Command::new(exe)
        .arg("daemon")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    for i in 0..40 {
        tokio::time::sleep(std::time::Duration::from_millis(100 * (i + 1))).await;
        if tokio::net::UnixStream::connect(&sock_path).await.is_ok() {
            eprintln!(" ready.");
            return Ok(());
        }
    }

    eprintln!(" failed.");
    let log_path = AppState::data_dir().join("logs/mxr.log");
    if log_path.exists() {
        if let Ok(contents) = std::fs::read_to_string(&log_path) {
            let last_lines: Vec<&str> = contents.lines().rev().take(5).collect();
            eprintln!("Recent daemon logs:");
            for line in last_lines.into_iter().rev() {
                eprintln!("  {line}");
            }
        }
    }
    anyhow::bail!(
        "Failed to start daemon. Check logs at {}. Useful next steps: `mxr status`, `mxr logs --level error`, `mxr daemon --foreground`.",
        log_path.display()
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SocketState {
    Reachable,
    Stale,
    Missing,
}

async fn inspect_socket_state(path: &std::path::Path) -> SocketState {
    if !path.exists() {
        return SocketState::Missing;
    }

    if tokio::net::UnixStream::connect(path).await.is_ok() {
        SocketState::Reachable
    } else {
        SocketState::Stale
    }
}

fn is_index_lock_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("lockbusy")
        || lower.contains("lockfile")
        || lower.contains("failed to acquire index lock")
        || lower.contains("failed to acquire lockfile")
        || lower.contains("already an `indexwriter` working")
        || lower.contains("already an indexwriter working")
}

#[cfg(test)]
mod tests {
    use super::is_index_lock_error;

    #[test]
    fn detects_tantivy_lockbusy_message() {
        let msg = "Search error: Failed to acquire Lockfile: LockBusy. Some(\"Failed to acquire index lock. If you are using a regular directory, this means there is already an `IndexWriter` working on this `Directory`, in this process or in a different process.\")";
        assert!(is_index_lock_error(msg));
    }

    #[test]
    fn ignores_unrelated_search_error() {
        assert!(!is_index_lock_error("Search error: schema does not match"));
    }
}
