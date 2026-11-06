//! A user supervisor recovers the local owner only within its recorded workspace identity.
use super::{
    hosting::{local, workspace},
    state::{self, Hosting},
};
use mxr_protocol::Request;
use std::time::Duration;

pub(super) async fn supervised() -> anyhow::Result<()> {
    let config = state::read()?.ok_or_else(|| anyhow::anyhow!("No hosting setup"))?;
    if !config.enabled || !config.daemon_enabled {
        return Ok(());
    }
    anyhow::ensure!(
        config.config_dir == mxr_config::config_dir()
            && config.data_dir == mxr_config::data_dir()
            && config.socket == mxr_config::socket_path(),
        "Supervisor profile does not match its recorded installation"
    );
    let mut child = None;
    let mut ready = false;
    for _ in 0..30 {
        if ensure_daemon(&config, &mut child).await? {
            ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    anyhow::ensure!(
        ready,
        "Local daemon did not become ready; no gateway was opened"
    );
    local(Request::RemoteAuthority {
        request: mxr_protocol::RemoteRequest::ResumeHosting,
    })
    .await?;
    let monitor = monitor(config.clone(), child);
    tokio::pin!(monitor);
    let gateway = mxr_web::remote_gateway::serve_with_origin(
        config.socket.clone(),
        config.listen,
        config.public_origin.clone(),
    );
    tokio::pin!(gateway);
    #[cfg(unix)]
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    tokio::select! {
        result=&mut gateway=>{result?;Ok(())},
        result=&mut monitor=>result,
        _=tokio::signal::ctrl_c()=>Ok(()),
        _=terminate.recv()=>Ok(()),
    }
}
async fn monitor(config: Hosting, mut child: Option<tokio::process::Child>) -> anyhow::Result<()> {
    loop {
        let desired = state::read()?.ok_or_else(|| anyhow::anyhow!("Hosting state removed"))?;
        if !desired.enabled || !desired.daemon_enabled {
            // Keep the gateway polled while the same boot's stopping notice and
            // bounded outcome-recovery window are still live.
            if let Err(error) = super::hosting::drain_remote().await {
                tracing::debug!(%error,"host already unavailable during desired stop");
            }
            return Ok(());
        }
        anyhow::ensure!(
            desired.workspace == config.workspace
                && desired.public_origin == config.public_origin
                && desired.listen == config.listen,
            "Hosting target changed; restart the service explicitly"
        );
        ensure_daemon(&config, &mut child).await?;
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}

/// Inspect durable ownership before starting provider work, including after a restore.
pub(super) async fn verify_offline_owner(config: &Hosting) -> anyhow::Result<()> {
    use sqlx::Connection;
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(config.data_dir.join("mxr.db"))
        .read_only(true)
        .create_if_missing(false);
    let mut connection = sqlx::SqliteConnection::connect_with(&options).await?;
    let identity: (String, String) = sqlx::query_as(
        "SELECT workspace_id,recovery_generation FROM archive_workspace WHERE singleton=1",
    )
    .fetch_one(&mut connection)
    .await?;
    let paused: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM restore_recovery WHERE pending=1)")
            .fetch_one(&mut connection)
            .await?;
    anyhow::ensure!(!paused && identity.0==config.workspace.workspace_id && identity.1==config.workspace.recovery_generation,"Restored/replaced workspace needs explicit recovery and hosting setup; no daemon was started");
    Ok(())
}
async fn ensure_daemon(
    config: &Hosting,
    child: &mut Option<tokio::process::Child>,
) -> anyhow::Result<bool> {
    if let Some(process) = child.as_mut() {
        if process.try_wait()?.is_some() {
            *child = None;
        }
    }
    if let Ok(actual) = workspace().await {
        anyhow::ensure!(
            actual == config.workspace,
            "Workspace changed; remote admission has stopped"
        );
        return Ok(true);
    }
    if child.is_none() {
        verify_offline_owner(config).await?;
        // The daemon's socket/index lock remains the sole ownership arbiter.
        *child = Some(
            tokio::process::Command::new(&config.executable)
                .args(["daemon", "--foreground", "--no-bridge"])
                // launchd kills children sharing a stopped hosting job's group.
                // A hosting-only stop must leave local provider work running.
                .process_group(0)
                .env(
                    "MXR_DAEMON_ADDR",
                    format!("unix://{}", config.socket.display()),
                )
                .env("MXR_CONFIG_DIR", &config.config_dir)
                .env("MXR_DATA_DIR", &config.data_dir)
                .env("MXR_SOCKET_PATH", &config.socket)
                .env("MXR_INSTANCE", &config.instance)
                .env("MXR_DAEMON_CLIENT_AUTOSTART", "1")
                .env(
                    "MXR_TOKEN_DIR",
                    config
                        .token_dir
                        .clone()
                        .unwrap_or_else(|| config.data_dir.join("tokens")),
                )
                .env(
                    "MXR_SECRETS_PATH",
                    config
                        .secrets_file
                        .clone()
                        .unwrap_or_else(|| config.config_dir.join("secrets.toml")),
                )
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()?,
        );
    }
    Ok(false)
}
