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
use futures::StreamExt;
use mxr_core::types::{SearchMode, SortOrder};
use mxr_protocol::{ClientKind, IpcCodec, IpcMessage, IpcPayload, Request, Response, ResponseData};
use mxr_test_support::daemon::write_fake_account_config;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio_util::codec::{Encoder, FramedRead};

const MXR_BIN: &str = env!("CARGO_BIN_EXE_mxr");
const BOUND: Duration = Duration::from_secs(30);

fn encode(message: &IpcMessage) -> Vec<u8> {
    let mut buf = BytesMut::new();
    IpcCodec::new().encode(message.clone(), &mut buf).unwrap();
    buf.to_vec()
}

fn ping_frame() -> Vec<u8> {
    encode(&IpcMessage {
        expected_workspace: None,
        id: 1,
        source: ClientKind::Cli,
        payload: IpcPayload::Request(Request::Ping),
    })
}

fn expected_pong_frame() -> Vec<u8> {
    encode(&IpcMessage {
        expected_workspace: None,
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

/// Declaration and Ping retain their correlated replies with no extra stdout,
/// require no transport token on LocalProcess, and exit cleanly on stdin EOF.
#[tokio::test]
async fn stdio_server_declaration_ping_pong_pure_stdout_no_auth() {
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

    stdin
        .write_all(&encode(&IpcMessage {
            expected_workspace: None,
            id: 0,
            source: ClientKind::Cli,
            payload: IpcPayload::Request(Request::DeclareClient {
                kind: ClientKind::Cli,
                protocol_version: mxr_protocol::IPC_PROTOCOL_VERSION,
            }),
        }))
        .await
        .unwrap();
    stdin.write_all(&ping_frame()).await.unwrap();
    stdin.flush().await.unwrap();

    let expected = [
        encode(&IpcMessage {
            expected_workspace: None,
            id: 0,
            source: ClientKind::Daemon,
            payload: IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }),
        }),
        expected_pong_frame(),
    ]
    .concat();
    let mut out = vec![0; expected.len()];
    tokio::time::timeout(BOUND, stdout.read_exact(&mut out))
        .await
        .expect("reading stdio-server stdout timed out")
        .unwrap();
    assert_eq!(
        out, expected,
        "stdout must contain declaration Ack then Pong, without logs or an auth demand"
    );

    drop(stdin);
    let mut rest = Vec::new();
    tokio::time::timeout(BOUND, stdout.read_to_end(&mut rest))
        .await
        .expect("reading stdio-server stdout timed out")
        .unwrap();
    assert!(
        rest.is_empty(),
        "nothing may follow the replies on stdout, got {rest:?}"
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

/// One `mxr daemon --stdio` process, for tests that send several requests and
/// must skip the events it pushes between replies.
struct StdioDaemon {
    child: Child,
    stdin: ChildStdin,
    frames: FramedRead<ChildStdout, IpcCodec>,
    next_id: u64,
}

impl StdioDaemon {
    async fn start(envs: &[(String, String)]) -> Self {
        let mut child = Command::new(MXR_BIN)
            .args(["daemon", "--stdio"])
            .envs(envs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let frames = FramedRead::new(child.stdout.take().unwrap(), IpcCodec::new());
        let mut daemon = Self {
            child,
            stdin,
            frames,
            next_id: 1,
        };
        let declared = daemon
            .request(Request::DeclareClient {
                kind: ClientKind::Cli,
                protocol_version: mxr_protocol::IPC_PROTOCOL_VERSION,
            })
            .await;
        assert!(
            matches!(
                declared,
                Response::Ok {
                    data: ResponseData::Ack
                }
            ),
            "declaration: {declared:?}"
        );
        daemon
    }

    async fn request(&mut self, request: Request) -> Response {
        let id = self.next_id;
        self.next_id += 1;
        self.stdin
            .write_all(&encode(&IpcMessage {
                expected_workspace: None,
                id,
                source: ClientKind::Cli,
                payload: IpcPayload::Request(request),
            }))
            .await
            .unwrap();
        self.stdin.flush().await.unwrap();
        loop {
            let frame = tokio::time::timeout(BOUND, self.frames.next())
                .await
                .expect("the stdio server did not reply in time")
                .expect("the stdio server closed stdout before replying")
                .unwrap();
            if let (true, IpcPayload::Response(response)) = (frame.id == id, frame.payload) {
                return response;
            }
        }
    }

    async fn stop(mut self) {
        drop(self.stdin);
        let status = tokio::time::timeout(BOUND, self.child.wait())
            .await
            .expect("stdio server did not exit after stdin EOF")
            .unwrap();
        assert!(
            status.success(),
            "clean stdin EOF should exit 0, got {status:?}"
        );
    }
}

/// `after:1970-01-01` matches every message, and the limited search that
/// answers while the index is rebuilding cannot run it.
fn search_every_message() -> Request {
    Request::Search {
        query: "after:1970-01-01".into(),
        limit: 1_000,
        offset: 0,
        account_id: None,
        mode: Some(SearchMode::Lexical),
        sort: Some(SortOrder::DateDesc),
        explain: false,
        limit_from_config: false,
    }
}

fn full_search_total(response: &Response) -> Option<u32> {
    match response {
        Response::Ok {
            data:
                ResponseData::SearchResults {
                    total,
                    degraded: None,
                    ..
                },
        } => Some(*total),
        _ => None,
    }
}

/// Regression: a store whose search index is gone, as `mxr doctor --reindex`
/// leaves it, gets full search back from the stdio server alone. It rebuilds
/// the index from SQLite, as a socket daemon's startup does, instead of
/// answering `search_index_unavailable` for as long as it runs.
#[tokio::test]
async fn stdio_server_rebuilds_a_missing_search_index() {
    let (dir, envs) = isolated("reindex");
    let config_dir = dir.path().join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    write_fake_account_config(&config_dir);

    let mut seeding = StdioDaemon::start(&envs).await;
    let synced = seeding
        .request(Request::SyncNow {
            account_id: None,
            background: false,
        })
        .await;
    assert!(matches!(synced, Response::Ok { .. }), "sync: {synced:?}");
    let seeded = seeding.request(search_every_message()).await;
    let stored = full_search_total(&seeded).expect("a fresh index answers in full");
    assert!(stored > 0, "the fake provider synced no mail: {seeded:?}");
    seeding.stop().await;

    std::fs::remove_dir_all(dir.path().join("data").join("search_index")).unwrap();

    let mut reopened = StdioDaemon::start(&envs).await;
    let deadline = tokio::time::Instant::now() + BOUND;
    let mut poll = tokio::time::interval(Duration::from_millis(200));
    let answered = loop {
        poll.tick().await;
        match reopened.request(search_every_message()).await {
            Response::Error { code, message, .. } if code == "search_index_unavailable" => {
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "search was still limited {BOUND:?} after the stdio server started: {message}"
                );
            }
            other => break other,
        }
    };
    assert_eq!(full_search_total(&answered), Some(stored), "{answered:?}");
    reopened.stop().await;
}
