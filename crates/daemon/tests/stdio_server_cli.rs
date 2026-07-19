//! Subprocess-level integration tests for `mxr daemon --stdio` (phase 5b).
//!
//! These drive the REAL `mxr` binary (`CARGO_BIN_EXE_mxr`) as a one-connection
//! stdio server — the LSP/inetd model. Unlike `dial-stdio` (a client proxy that
//! needs a daemon on the socket), `--stdio` IS the daemon: it takes the
//! exclusive runtime state and serves exactly one connection over its own
//! stdin/stdout, so the test needs only an isolated instance with no other
//! daemon running.
//!
//! Covers the process-level contract the in-crate serve-core tests cannot:
//! real stdin/stdout wiring, absolute stdout purity (frames only — no log lines
//! leak), no auth handshake demanded (`LocalProcess` trust), and a clean exit on
//! stdin EOF.

#![expect(
    clippy::unwrap_used,
    reason = "integration tests assert directly on fixtures"
)]

use std::process::Stdio;
use std::time::Duration;

use bytes::BytesMut;
use mxr_protocol::{ClientKind, IpcCodec, IpcMessage, IpcPayload, Request, Response, ResponseData};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio_util::codec::Encoder;

const MXR_BIN: &str = env!("CARGO_BIN_EXE_mxr");
const BOUND: Duration = Duration::from_secs(30);

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

/// A private instance (unique `MXR_INSTANCE` + temp dirs) so the test never
/// touches the user's real or `mxr-dev` daemon and can take the exclusive lock.
fn isolated(tag: &str) -> (tempfile::TempDir, Vec<(String, String)>) {
    let dir = tempfile::tempdir().unwrap();
    let envs = vec![
        (
            "MXR_INSTANCE".to_string(),
            format!("mxr-stdio-it-{tag}-{}", std::process::id()),
        ),
        (
            "MXR_CONFIG_DIR".to_string(),
            dir.path().join("config").display().to_string(),
        ),
        (
            "MXR_DATA_DIR".to_string(),
            dir.path().join("data").display().to_string(),
        ),
        (
            "MXR_SOCKET_PATH".to_string(),
            dir.path().join("mxr.sock").display().to_string(),
        ),
        ("MXR_ACTIVITY".to_string(), "off".to_string()),
    ];
    (dir, envs)
}

/// The core contract: a framed `Ping` on stdin comes back as a byte-identical
/// `Pong` on stdout with NOTHING else (stdout purity), NO `Authenticate` was
/// demanded (LocalProcess trust), and the process exits 0 on stdin EOF.
#[tokio::test]
async fn stdio_server_ping_pong_pure_stdout_no_auth() {
    let (_dir, envs) = isolated("pp");
    let mut child = Command::new(MXR_BIN)
        .args(["daemon", "--stdio"])
        .envs(envs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();

    // Straight to a Ping — no Authenticate handshake first.
    stdin.write_all(&ping_frame()).await.unwrap();
    stdin.flush().await.unwrap();
    // stdin EOF -> the one connection closes -> the stdio server exits.
    drop(stdin);

    let mut out = Vec::new();
    tokio::time::timeout(BOUND, stdout.read_to_end(&mut out))
        .await
        .expect("reading stdio-server stdout timed out")
        .unwrap();

    // Purity + no-auth in one assertion: stdout is EXACTLY the Pong frame. Any
    // log line, ANSI colour, or an Auth-error frame instead of Pong would fail.
    assert_eq!(
        out,
        expected_pong_frame(),
        "stdout must be exactly one Pong frame — no logs, no auth demand, nothing else"
    );

    let status = tokio::time::timeout(BOUND, child.wait())
        .await
        .expect("stdio server did not exit after stdin EOF")
        .unwrap();
    assert!(
        status.success(),
        "clean stdin EOF should exit 0, got {status:?}"
    );
}
