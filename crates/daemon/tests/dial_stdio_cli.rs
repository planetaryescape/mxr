//! Subprocess-level integration tests for `mxr daemon dial-stdio`.
//!
//! These drive the REAL `mxr` binary (`CARGO_BIN_EXE_mxr`) against a fake IPC
//! server on a temp Unix socket, covering the process-level contract the
//! in-crate `pipe_stdio` unit tests cannot: real stdin/stdout wiring, the
//! daemon-closes-first exit (the hang regression), and the failure exit code.
//!
//! The fake answers every request with a `Pong` echoing its id — enough for
//! `ensure_daemon_running`'s ping-based liveness probe, so autostart leaves the
//! fake running instead of trying to restart it.

#![expect(
    clippy::unwrap_used,
    reason = "integration tests assert directly on fixtures"
)]

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use bytes::BytesMut;
use futures::{SinkExt, StreamExt};
use mxr_protocol::{
    ClientKind, IpcCodec, IpcMessage, IpcPayload, Request, Response, ResponseData,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::process::{Child, Command};
use tokio::task::JoinHandle;
use tokio_util::codec::{Encoder, Framed};

const MXR_BIN: &str = env!("CARGO_BIN_EXE_mxr");
const BOUND: Duration = Duration::from_secs(20);

fn encode(message: &IpcMessage) -> Vec<u8> {
    let mut buf = BytesMut::new();
    IpcCodec::new().encode(message.clone(), &mut buf).unwrap();
    buf.to_vec()
}

fn ping_frame() -> Vec<u8> {
    encode(&IpcMessage {
        id: 1,
        source: ClientKind::Cli,
        payload: IpcPayload::Request(Request::Ping),
    })
}

fn expected_pong_frame() -> Vec<u8> {
    encode(&IpcMessage {
        id: 1,
        source: ClientKind::Daemon,
        payload: IpcPayload::Response(Response::Ok {
            data: ResponseData::Pong,
        }),
    })
}

/// Fake daemon that answers every request with a `Pong` echoing its id. When
/// `close_after_first`, it closes each connection right after the first reply —
/// the "daemon closes first" scenario that must not hang the proxy.
fn spawn_fake_daemon(sock: &Path, close_after_first: bool) -> JoinHandle<()> {
    let listener = UnixListener::bind(sock).unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut framed = Framed::new(stream, IpcCodec::new());
                while let Some(Ok(msg)) = framed.next().await {
                    let reply = IpcMessage {
                        id: msg.id,
                        source: ClientKind::Daemon,
                        payload: IpcPayload::Response(Response::Ok {
                            data: ResponseData::Pong,
                        }),
                    };
                    if framed.send(reply).await.is_err() || close_after_first {
                        return;
                    }
                }
            });
        }
    })
}

struct Isolated {
    _dir: tempfile::TempDir,
    sock: PathBuf,
    envs: Vec<(String, String)>,
}

/// A private instance (unique `MXR_INSTANCE` + temp dirs) so the test never
/// touches the user's real or `mxr-dev` daemon.
fn isolated(tag: &str) -> Isolated {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("mxr.sock");
    let envs = vec![
        (
            "MXR_INSTANCE".to_string(),
            format!("mxr-dialstdio-it-{tag}-{}", std::process::id()),
        ),
        (
            "MXR_CONFIG_DIR".to_string(),
            dir.path().join("config").display().to_string(),
        ),
        (
            "MXR_DATA_DIR".to_string(),
            dir.path().join("data").display().to_string(),
        ),
        ("MXR_SOCKET_PATH".to_string(), sock.display().to_string()),
        ("MXR_ACTIVITY".to_string(), "off".to_string()),
    ];
    Isolated {
        _dir: dir,
        sock,
        envs,
    }
}

fn spawn_dial_stdio(envs: &[(String, String)]) -> Child {
    Command::new(MXR_BIN)
        .args(["daemon", "dial-stdio"])
        .envs(envs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap()
}

/// stdin EOF path: the framed Ping comes back on the real process's stdout as a
/// byte-identical Pong frame with nothing else, and the process exits 0.
#[tokio::test]
async fn ping_pong_byte_identical_over_real_process() {
    let iso = isolated("pp");
    let _server = spawn_fake_daemon(&iso.sock, false);
    let mut child = spawn_dial_stdio(&iso.envs);
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();

    stdin.write_all(&ping_frame()).await.unwrap();
    stdin.flush().await.unwrap();
    // stdin EOF -> child half-closes the socket -> fake closes -> child exits 0.
    drop(stdin);

    let mut out = Vec::new();
    tokio::time::timeout(BOUND, stdout.read_to_end(&mut out))
        .await
        .expect("reading child stdout timed out")
        .unwrap();
    assert_eq!(
        out,
        expected_pong_frame(),
        "stdout must be exactly one Pong frame and nothing else"
    );

    let status = tokio::time::timeout(BOUND, child.wait())
        .await
        .expect("child did not exit")
        .unwrap();
    assert!(status.success(), "clean stdin EOF should exit 0, got {status:?}");
}

/// Daemon-closes-first path (the hang regression): with stdin still open, the
/// daemon closing its side must still end the proxy promptly with exit 0 — it
/// must not wait on the uncancellable blocking stdin read.
#[tokio::test]
async fn daemon_closing_first_exits_without_waiting_on_stdin() {
    let iso = isolated("df");
    let _server = spawn_fake_daemon(&iso.sock, true);
    let mut child = spawn_dial_stdio(&iso.envs);
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();

    stdin.write_all(&ping_frame()).await.unwrap();
    stdin.flush().await.unwrap();
    // Deliberately keep stdin OPEN. The daemon closes first; the proxy must exit
    // anyway. If it waited on the stdin direction, these bounded waits trip.

    let mut out = Vec::new();
    tokio::time::timeout(BOUND, stdout.read_to_end(&mut out))
        .await
        .expect("child stdout never reached EOF — daemon-first close hung")
        .unwrap();
    assert_eq!(out, expected_pong_frame());

    let status = tokio::time::timeout(BOUND, child.wait())
        .await
        .expect("child did not exit after daemon closed — HANG regression")
        .unwrap();
    assert!(
        status.success(),
        "daemon-first close should exit 0, got {status:?}"
    );

    drop(stdin);
}

/// Failure path: an unstartable instance (config/data/socket under a regular
/// file) makes the process exit nonzero, and stdout stays empty.
#[tokio::test]
async fn unstartable_instance_exits_nonzero_with_empty_stdout() {
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, b"x").unwrap(); // a FILE used as a dir root -> ENOTDIR

    let envs = [
        (
            "MXR_INSTANCE",
            format!("mxr-dialstdio-it-fail-{}", std::process::id()),
        ),
        (
            "MXR_CONFIG_DIR",
            blocker.join("config").display().to_string(),
        ),
        ("MXR_DATA_DIR", blocker.join("data").display().to_string()),
        ("MXR_SOCKET_PATH", blocker.join("mxr.sock").display().to_string()),
        ("MXR_ACTIVITY", "off".to_string()),
    ];

    let output = tokio::time::timeout(
        BOUND,
        Command::new(MXR_BIN)
            .args(["daemon", "dial-stdio"])
            .envs(envs.iter().map(|(k, v)| (*k, v.as_str())))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .expect("failure-path child did not exit")
    .unwrap();

    assert!(
        !output.status.success(),
        "unstartable instance must exit nonzero, got {:?}",
        output.status
    );
    assert!(
        output.stdout.is_empty(),
        "stdout must be empty on failure, got {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
}
