//! Preview-first host setup. Service/proxy commands never use the selected remote.
use super::{
    proxy, runner,
    state::{self, Hosting},
    supervisor,
};
use mxr_protocol::{ArchiveWorkspaceData, ClientKind, Request, ResponseData};
use std::time::Duration;

pub(super) fn normalize_origin(origin: &str) -> anyhow::Result<String> {
    let url = url::Url::parse(origin)?;
    anyhow::ensure!(
        url.scheme() == "https"
            && url.username().is_empty()
            && url.password().is_none()
            && url.host_str().is_some()
            && url.path() == "/"
            && url.query().is_none()
            && url.fragment().is_none(),
        "Public origin must be an HTTPS origin without credentials, path, query or fragment"
    );
    Ok(url.origin().ascii_serialization())
}

pub(super) async fn local(request: Request) -> anyhow::Result<ResponseData> {
    let mut connection =
        mxr_client::IpcConnection::connect_supervised(&mxr_config::socket_path(), ClientKind::Cli)
            .await?;
    match connection
        .request_response(request, |_| {}, Some(Duration::from_secs(5)))
        .await?
    {
        mxr_protocol::Response::Ok { data } => Ok(data),
        mxr_protocol::Response::Error { message, .. } => anyhow::bail!("{message}"),
    }
}
pub(super) async fn workspace() -> anyhow::Result<ArchiveWorkspaceData> {
    let status = local(Request::GetStatus).await?;
    anyhow::ensure!(
        matches!(status,ResponseData::Status{protocol_version,..} if protocol_version==mxr_protocol::IPC_PROTOCOL_VERSION),
        "Local daemon must match this client's IPC version"
    );
    match local(Request::GetRecoveryStatus).await? {
        ResponseData::RecoveryStatus { recovery: status } => {
            anyhow::ensure!(
                !status.restore_pending,
                "Activate the restored workspace explicitly before hosting"
            );
            Ok(ArchiveWorkspaceData {
                workspace_id: status.workspace_id,
                recovery_generation: status.recovery_generation,
            })
        }
        _ => anyhow::bail!("Local daemon did not return workspace recovery identity"),
    }
}

pub(super) async fn prepare_foreground() -> anyhow::Result<()> {
    let identity = workspace().await?;
    if let Some(config) = state::read()? {
        anyhow::ensure!(config.daemon_enabled,"This hosted owner was stopped; explicitly start the local daemon or `serve start --confirm` before serving it");
        anyhow::ensure!(identity==config.workspace,"The local workspace changed; inspect recovery and update hosting setup before serving it");
    }
    local(Request::RemoteAuthority {
        request: mxr_protocol::RemoteRequest::ResumeHosting,
    })
    .await?;
    Ok(())
}

pub(super) async fn setup(
    origin: String,
    listen: std::net::SocketAddr,
    confirm: bool,
    format: crate::cli::OutputFormat,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        listen.ip() == std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST) && listen.port() != 0,
        "Tailscale hosting requires a fixed 127.0.0.1 loopback port"
    );
    let _lock = if confirm { Some(state::lock()?) } else { None };
    let previous = state::read()?;
    let mut config = Hosting {
        version: 1,
        executable: std::env::current_exe()?,
        config_dir: mxr_config::config_dir(),
        data_dir: mxr_config::data_dir(),
        socket: mxr_config::socket_path(),
        instance: mxr_config::app_instance_name(),
        token_dir: Some(mxr_config::token_dir()),
        secrets_file: Some(mxr_config::secrets_file_path()),
        public_origin: normalize_origin(&origin)?,
        listen,
        workspace: workspace().await?,
        enabled: true,
        daemon_enabled: true,
        mapping_installed: false,
        service_installed: false,
        service_digest: None,
        service_previous_digest: None,
    };
    anyhow::ensure!(
        [
            &config.executable,
            &config.config_dir,
            &config.data_dir,
            &config.socket
        ]
        .iter()
        .all(|path| path.is_absolute())
            && config.token_dir.as_ref().is_none_or(|p| p.is_absolute())
            && config.secrets_file.as_ref().is_none_or(|p| p.is_absolute()),
        "Hosting requires absolute profile, credential and socket paths"
    );
    if let Some(old) = &previous {
        anyhow::ensure!(old.public_origin==config.public_origin && old.listen==config.listen && old.workspace==config.workspace,"Existing hosting targets another origin, port or workspace; stop it and inspect hosting.json before replacing the installation");
        config.mapping_installed = old.mapping_installed;
        config.service_installed = old.service_installed;
        config.service_digest = old.service_digest.clone();
        config.service_previous_digest = old.service_previous_digest.clone();
    }
    proxy::prerequisites(&config).await?;
    let startup = supervisor::prerequisites().await?;
    let service_file = supervisor::path(&config)?;
    let rendering = supervisor::render(&config, cfg!(target_os = "macos"))?;
    supervisor::check_install(&config, previous.as_ref())?;
    if !confirm {
        return crate::output::print_json(
            &serde_json::json!({"dry_run":true,"hosting":config,"service_file":service_file,"service_configuration":rendering,"startup":startup,"effect":"install an owned user service and publish only the selected HTTPS port; local provider work remains on this host"}),
            format,
        );
    }
    // Persist both the old receipt and new intent before replacing a service file.
    // A crash on either side of the rename can then be reconciled without adopting
    // unrelated contents or losing ownership of the previous file.
    let new_digest = supervisor::digest(&rendering);
    let reload_service = previous.as_ref().is_some_and(|old| {
        old.service_installed
            && (old.service_digest.as_ref() != Some(&new_digest)
                || old.service_previous_digest.is_some())
    });
    if config.service_digest.as_ref() != Some(&new_digest) {
        config.service_previous_digest = config.service_digest.clone();
    }
    config.service_digest = Some(new_digest);
    config.service_installed = true;
    state::write(&config)?;
    if reload_service {
        if let Some(previous) = &previous {
            supervisor::stop(previous).await?;
        }
    }
    supervisor::install(&config, previous.as_ref())?;
    config.service_previous_digest = None;
    state::write(&config)?;
    supervisor::start(&config).await?;
    wait_local(&config).await?;
    config.mapping_installed = true;
    state::write(&config)?;
    proxy::publish(&config).await?;
    verify_public(&config).await?;
    crate::output::print_json(
        &serde_json::json!({"dry_run":false,"hosting":config,"startup":startup,"tls_health_verified":true,"anonymous_ipc_rejected":true}),
        format,
    )
}

pub(super) async fn status(format: crate::cli::OutputFormat) -> anyhow::Result<()> {
    let hosting = state::read()?;
    let authority = local(Request::RemoteAuthority {
        request: mxr_protocol::RemoteRequest::Status,
    })
    .await;
    if hosting.is_none() && authority.is_err() {
        return Err(authority
            .err()
            .unwrap_or_else(|| anyhow::anyhow!("Local host unavailable")));
    }
    let clients = local(Request::RemoteAuthority {
        request: mxr_protocol::RemoteRequest::Clients,
    })
    .await
    .ok();
    let sessions = local(Request::RemoteAuthority {
        request: mxr_protocol::RemoteRequest::Sessions,
    })
    .await
    .ok();
    crate::output::print_json(
        &serde_json::json!({"hosting":hosting,"local_host_available":authority.is_ok(),"authority":authority.ok(),"clients":clients,"presence":sessions}),
        format,
    )
}

pub(super) async fn start(confirm: bool, format: crate::cli::OutputFormat) -> anyhow::Result<()> {
    let _lock = if confirm { Some(state::lock()?) } else { None };
    let mut config = state::read()?.ok_or_else(|| anyhow::anyhow!("Run mxr serve setup first"))?;
    proxy::prerequisites(&config).await?;
    supervisor::prerequisites().await?;
    if !confirm {
        return crate::output::print_json(
            &serde_json::json!({"dry_run":true,"hosting":config,"effect":"enable hosting and local daemon recovery"}),
            format,
        );
    }
    config.enabled = true;
    config.daemon_enabled = true;
    state::write(&config)?;
    supervisor::start(&config).await?;
    wait_local(&config).await?;
    config.mapping_installed = true;
    state::write(&config)?;
    proxy::publish(&config).await?;
    verify_public(&config).await?;
    crate::output::print_json(
        &serde_json::json!({"dry_run":false,"hosting":config,"tls_health_verified":true}),
        format,
    )
}

pub(super) async fn stop(
    all: bool,
    confirm: bool,
    format: crate::cli::OutputFormat,
) -> anyhow::Result<()> {
    let _lock = if confirm { Some(state::lock()?) } else { None };
    let Some(mut config) = state::read()? else {
        return crate::output::print_json(
            &serde_json::json!({"configured":false,"stopped":true}),
            format,
        );
    };
    let current = workspace().await;
    let original_pid = match local(Request::GetStatus).await {
        Ok(ResponseData::Status { daemon_pid, .. }) => daemon_pid,
        _ => None,
    };
    let original_started = original_pid.and_then(crate::process_probe::start_time);
    if let Ok(identity) = &current {
        anyhow::ensure!(
            identity == &config.workspace,
            "Local workspace changed; refusing to stop a different owner"
        );
    }
    if all && current.is_err() {
        runner::verify_offline_owner(&config).await?;
    }
    let registrations = local(Request::RemoteAuthority {
        request: mxr_protocol::RemoteRequest::Clients,
    })
    .await
    .ok();
    let impact = local(Request::RemoteAuthority {
        request: mxr_protocol::RemoteRequest::Drain {
            dry_run: true,
            deadline_seconds: 5,
        },
    })
    .await
    .ok();
    if !confirm {
        return crate::output::print_json(
            &serde_json::json!({"dry_run":true,"hosting":config,"scope":if all{"host_and_daemon"}else{"remote_hosting"},"registrations":registrations,"impact":impact,"deadline_seconds":5,"effect":"close remote admission, stop the owned user service and remove its exact proxy; accepted mail remains durable"}),
            format,
        );
    }
    config.enabled = false;
    if all {
        config.daemon_enabled = false;
    }
    state::write(&config)?;
    // Close admission atomically before closing the gateway. Existing accepted
    // operations get the same bounded drain window as a daemon shutdown.
    let drain = drain_remote().await;
    let service_stop = supervisor::stop(&config).await;
    let proxy_remove = proxy::remove(&config).await;
    if proxy_remove.is_ok() {
        config.mapping_installed = false;
        state::write(&config)?;
    }
    let daemon_stop = if all {
        match workspace().await {
            Ok(actual) if actual == config.workspace => local(Request::Shutdown).await.map(|_| ()),
            Ok(_) => Err(anyhow::anyhow!(
                "Workspace changed during stop; daemon left running"
            )),
            Err(_) => Ok(()),
        }
    } else {
        Ok(())
    };
    service_stop?;
    proxy_remove?;
    if all {
        // The tagged daemon may finish its own desired-state drain before this
        // command's next IPC call. Its original verified process exiting is a
        // successful stop, even when the final drain/Shutdown reply is gone.
        anyhow::ensure!(owner_stopped(&config,original_pid,original_started.as_deref()).await,"Remote hosting is stopped, but the original local daemon has not exited; inspect it before treating the provider owner as stopped");
    } else {
        daemon_stop?;
        if current.is_ok() {
            drain?;
        }
    }
    crate::output::print_json(
        &serde_json::json!({"dry_run":false,"hosting":config,"stopped":true,"daemon_stop_requested":all}),
        format,
    )
}

pub(super) async fn drain_remote() -> anyhow::Result<()> {
    let response = local(Request::RemoteAuthority {
        request: mxr_protocol::RemoteRequest::Drain {
            dry_run: false,
            deadline_seconds: 5,
        },
    })
    .await?;
    let ResponseData::RemoteAuthority {
        response: mxr_protocol::RemoteResponse::Drain { status, .. },
    } = response
    else {
        anyhow::bail!("Unexpected remote drain response")
    };
    // Concurrent CLI and supervisor drains share the first deadline. An empty
    // in-flight set does not remove idle clients' opportunity to read the notice.
    let remaining =
        drain_notice_remaining(status.deadline_at, chrono::Utc::now().timestamp_millis());
    tokio::time::sleep(remaining).await;
    Ok(())
}
fn drain_notice_remaining(deadline_at: Option<i64>, now_millis: i64) -> Duration {
    Duration::from_millis(
        deadline_at
            .map(|at| at.saturating_mul(1000).saturating_sub(now_millis))
            .unwrap_or(5000)
            .clamp(0, 5000) as u64,
    )
}
async fn owner_stopped(config: &Hosting, pid: Option<u32>, started: Option<&str>) -> bool {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(12);
    loop {
        let original_gone = match pid {
            Some(pid) => {
                !crate::server::process_is_alive(pid)
                    || started.is_some_and(|before| {
                        crate::process_probe::start_time(pid)
                            .as_deref()
                            .is_some_and(|current| current != before)
                    })
            }
            None => match std::fs::read_to_string(config.data_dir.join("daemon.pid")) {
                Ok(value) => value
                    .trim()
                    .parse::<u32>()
                    .ok()
                    .is_some_and(|pid| !crate::server::process_is_alive(pid)),
                Err(error) => error.kind() == std::io::ErrorKind::NotFound,
            },
        };
        if original_gone
            && !matches!(
                crate::server::inspect_socket_state(&config.socket).await,
                crate::server::SocketState::Reachable
            )
        {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
pub(super) async fn stop_daemon(confirm: bool) -> anyhow::Result<()> {
    let format = crate::cli::OutputFormat::Json;
    if state::read()?.is_some() {
        return stop(true, confirm, format).await;
    }
    let impact = local(Request::RemoteAuthority {
        request: mxr_protocol::RemoteRequest::Drain {
            dry_run: !confirm,
            deadline_seconds: 5,
        },
    })
    .await?;
    if !confirm {
        return crate::output::print_json(
            &serde_json::json!({"dry_run":true,"scope":"daemon","impact":impact}),
            format,
        );
    }
    drain_remote().await?;
    local(Request::Shutdown).await?;
    crate::output::print_json(
        &serde_json::json!({"dry_run":false,"daemon_stop_requested":true}),
        format,
    )
}

fn health_matches(body: &serde_json::Value, config: &Hosting) -> bool {
    body["service"] == "mxr-remote-gateway"
        && body["protocol_version"] == mxr_protocol::IPC_PROTOCOL_VERSION
        && body["profile_id"] == mxr_web::remote_gateway::profile_id(&config.socket)
}
async fn wait_local(config: &Hosting) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    for _ in 0..50 {
        if let Ok(response) = client
            .get(format!("http://{}/api/v1/health", config.listen))
            .send()
            .await
        {
            if response.status().is_success() {
                if let Ok(body) = response.json::<serde_json::Value>().await {
                    if health_matches(&body, config) {
                        return Ok(());
                    }
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    anyhow::bail!(
        "Owned gateway did not become ready; inspect the user service before retrying setup"
    )
}
async fn verify_public(config: &Hosting) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let health = client
        .get(format!("{}/api/v1/health", config.public_origin))
        .send()
        .await?;
    anyhow::ensure!(
        health.status().is_success(),
        "Public TLS gateway health failed"
    );
    let body = health.json::<serde_json::Value>().await?;
    anyhow::ensure!(
        health_matches(&body, config),
        "Public gateway identity/protocol did not match the installed build"
    );
    let response = client
        .get(format!("{}/api/v1/remote/ipc", config.public_origin))
        .header("connection", "Upgrade")
        .header("upgrade", "websocket")
        .header("sec-websocket-version", "13")
        .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
        .send()
        .await?;
    anyhow::ensure!(
        response.status() == reqwest::StatusCode::UNAUTHORIZED,
        "Public gateway did not reject anonymous IPC"
    );
    Ok(())
}

#[cfg(test)]
mod tests;
