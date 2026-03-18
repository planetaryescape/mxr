use crate::handler::handle_request;
use crate::loops;
use crate::state::AppState;
use futures::{SinkExt, StreamExt};
use mxr_protocol::IpcCodec;
use std::sync::Arc;
use tokio::net::UnixListener;
use tokio_util::codec::Framed;

pub async fn run_daemon() -> anyhow::Result<()> {
    let state = Arc::new(AppState::new().await?);

    // Remove stale socket
    let sock_path = AppState::socket_path();
    if let Some(parent) = sock_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let _ = std::fs::remove_file(&sock_path);

    let listener = UnixListener::bind(&sock_path)?;
    tracing::info!("Daemon listening on {}", sock_path.display());

    // Initial sync with FakeProvider
    {
        let count = state
            .sync_engine
            .sync_account(state.provider.as_ref())
            .await?;
        tracing::info!("Initial sync complete: {} messages", count);
    }

    // Spawn background loops
    let sync_state = state.clone();
    tokio::spawn(async move {
        loops::sync_loop(sync_state).await;
    });

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
            let mut framed = Framed::new(stream, IpcCodec::new());

            loop {
                tokio::select! {
                    msg = framed.next() => {
                        match msg {
                            Some(Ok(ipc_msg)) => {
                                let response = handle_request(&state, &ipc_msg).await;
                                if framed.send(response).await.is_err() {
                                    break;
                                }
                            }
                            Some(Err(e)) => {
                                tracing::error!("IPC decode error: {}", e);
                                break;
                            }
                            None => break,
                        }
                    }
                    event = event_rx.recv() => {
                        if let Ok(event_msg) = event {
                            if framed.send(event_msg).await.is_err() {
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

    if tokio::net::UnixStream::connect(&sock_path).await.is_ok() {
        return Ok(());
    }

    let exe = std::env::current_exe()?;
    std::process::Command::new(exe)
        .arg("daemon")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    for i in 0..20 {
        tokio::time::sleep(std::time::Duration::from_millis(100 * (i + 1))).await;
        if tokio::net::UnixStream::connect(&sock_path).await.is_ok() {
            return Ok(());
        }
    }

    anyhow::bail!("Failed to start daemon")
}
