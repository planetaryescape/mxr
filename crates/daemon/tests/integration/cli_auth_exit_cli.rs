//! A daemon that rejects every request with `IpcErrorKind::Auth` makes the
//! CLI exit 4, whichever command module issued the request. The fake answers
//! the status probe with this binary's own identity so the CLI treats it as
//! compatible and reaches the command.

#![expect(
    clippy::unwrap_used,
    reason = "integration tests assert directly on fixtures"
)]

use std::path::Path;
use std::process::{Command, Output};

use futures::{SinkExt, StreamExt};
use mxr_protocol::{
    ClientKind, IpcCodec, IpcErrorKind, IpcMessage, IpcPayload, Request, Response, ResponseData,
    IPC_PROTOCOL_VERSION,
};
use tokio::net::UnixListener;
use tokio_util::codec::Framed;

const MXR_BIN: &str = env!("CARGO_BIN_EXE_mxr");

/// The build id the `mxr` subprocess reports, derived the same way as
/// `mxr_client::current_daemon_build_id` from its executable.
fn cli_build_id() -> String {
    let path = std::fs::canonicalize(MXR_BIN).unwrap();
    let meta = std::fs::metadata(&path).unwrap();
    let modified = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_secs());
    format!(
        "{}:{}:{}:{modified}",
        env!("CARGO_PKG_VERSION"),
        path.display(),
        meta.len()
    )
}

fn status() -> ResponseData {
    ResponseData::Status {
        workspace_owner_supported: false,
        draft_revision_supported: true,
        send_outcome_supported: true,
        archive_recovery_supported: true,
        attachment_transfer_supported: true,
        draft_attachment_upload_supported: true,
        recovery_supported: false,
        recovery: None,
        uptime_secs: 1,
        accounts: vec!["personal".into()],
        total_messages: 0,
        daemon_pid: Some(1),
        sync_statuses: vec![],
        protocol_version: IPC_PROTOCOL_VERSION,
        daemon_version: Some(env!("CARGO_PKG_VERSION").into()),
        daemon_build_id: Some(cli_build_id()),
        repair_required: false,
        semantic_runtime: None,
        feature_health: None,
        degraded: false,
    }
}

fn spawn_auth_rejecting_daemon(socket: &Path) -> tokio::task::JoinHandle<()> {
    let listener = UnixListener::bind(socket).unwrap();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut framed = Framed::new(stream, IpcCodec::new());
                while let Some(Ok(message)) = framed.next().await {
                    let response = match message.payload {
                        IpcPayload::Request(Request::GetStatus) => Response::Ok { data: status() },
                        _ => Response::error_kinded("Unauthorized", IpcErrorKind::Auth),
                    };
                    let reply = IpcMessage {
                        expected_workspace: None,
                        id: message.id,
                        source: ClientKind::Daemon,
                        payload: IpcPayload::Response(response),
                    };
                    if framed.send(reply).await.is_err() {
                        break;
                    }
                }
            });
        }
    })
}

async fn run_mxr(root: &Path, socket: &Path, args: &'static [&'static str]) -> Output {
    let local = root.join("client");
    let socket = socket.to_path_buf();
    let envs = [
        ("MXR_INSTANCE", "mxr-cli-auth-exit-it".to_string()),
        ("MXR_CONFIG_DIR", local.join("config").display().to_string()),
        ("MXR_DATA_DIR", local.join("data").display().to_string()),
        ("MXR_SOCKET_PATH", socket.display().to_string()),
        ("MXR_ACTIVITY", "off".to_string()),
        ("MXR_DAEMON_AUTOSTART", "off".to_string()),
        ("HOME", local.join("home").display().to_string()),
    ];
    tokio::task::spawn_blocking(move || {
        Command::new(MXR_BIN)
            .args(args)
            .env_remove("MXR_DAEMON_ADDR")
            .env_remove("MXR_DAEMON_TOKEN")
            .envs(envs)
            .output()
            .unwrap()
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn daemon_auth_rejections_exit_4_from_each_command_family() {
    let temp = tempfile::tempdir().unwrap();
    let socket = temp.path().join("mxr.sock");
    let server = spawn_auth_rejecting_daemon(&socket);

    for args in [&["snippets"][..], &["signatures"][..], &["rules"][..]] {
        let output = run_mxr(temp.path(), &socket, args).await;
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(4), "{args:?} stderr: {stderr}");
        assert!(stderr.contains("Unauthorized"), "{args:?} stderr: {stderr}");
    }

    server.abort();
}
