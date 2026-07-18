//! `mxr daemon dial-stdio` — pipe raw bytes between this process's
//! stdin/stdout and the local daemon's Unix socket.
//!
//! This is the Docker `connhelper` move (see
//! `docs/transport-adapters/05-tcp-stdio-adapters.md` §5c): a byte pump that
//! lets any transport which can exec a process and pipe stdio reach the daemon
//! — `ssh -T host mxr daemon dial-stdio`,
//! `docker exec -i <container> mxr daemon dial-stdio`, and any community bridge
//! that can spawn a process. No new daemon trust surface: the caller still
//! needs local Unix-socket access on the daemon's machine.
//!
//! ## Byte-stream lifetime
//!
//! The two directions are pumped independently and the socket -> stdout
//! direction owns the exit (Docker `dial-stdio` semantics), because
//! `copy_bidirectional` — which only returns once *both* directions EOF —
//! deadlocks here: `tokio::io::stdin()` reads on a blocking thread that cannot
//! be cancelled, so if the daemon closes first the stdin -> socket direction
//! never finishes.
//!
//! - **stdin EOF first:** half-close the socket write side (SHUT_WR) so the
//!   daemon sees end-of-input, keep draining socket -> stdout until the daemon
//!   closes, then exit 0.
//! - **daemon closes first:** socket -> stdout hits EOF; flush stdout and exit
//!   promptly *without* waiting on the parked stdin read. [`run`] finishes with
//!   `std::process::exit`, because returning normally would hang the runtime's
//!   shutdown while it tries to join that uncancellable blocking read.
//!
//! Because that read cannot be cancelled, *every* post-pump exit — clean or
//! error — leaves via [`run`]'s `std::process::exit` (0 on clean close, 1 after
//! writing the error to stderr); there is no `?` once piping has begun.
//! Downstream (socket -> stdout) write failures propagate as errors; upstream
//! (stdin -> socket) failures are normalized to clean only when they are the
//! daemon's expected peer-close (`BrokenPipe`/`ConnectionReset`).
//!
//! ## stdout discipline
//!
//! Once piping begins, stdout carries only socket bytes. Nothing on this path
//! writes to stdout:
//! - daemon autostart status (`ensure_daemon_running`) prints only to stderr;
//! - tracing is file-only in this mode (`init_tracing(false)`, enforced by the
//!   dispatcher in `lib.rs`), so no log line can reach stdout;
//! - a connect (or pump) failure returns an error that [`run`] renders to
//!   stderr and turns into a non-zero exit — never to stdout.

use std::io::Write as _;
use std::path::Path;

use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::net::UnixStream;

/// Entry point for `mxr daemon dial-stdio`.
///
/// Autostarts the local daemon per repo convention, then pipes this process's
/// stdin/stdout to the daemon socket until the daemon closes its side.
pub async fn run() -> anyhow::Result<()> {
    // Convention: CLI commands ensure the local daemon is up before talking to
    // it. This runs before any byte is piped and before stdin is ever read, so a
    // normal `?` here is safe; its progress output goes to stderr, keeping
    // stdout byte-clean.
    crate::server::ensure_daemon_running().await?;
    let socket_path = crate::state::AppState::socket_path();

    // Once piping begins, stdin may be parked in tokio's uncancellable blocking
    // read, so EVERY exit path must terminate the process explicitly — a normal
    // return (even on error) would hang runtime shutdown trying to join that
    // read. No `?` past this point.
    match pipe_stdio(&socket_path, tokio::io::stdin(), tokio::io::stdout()).await {
        Ok(()) => std::process::exit(0),
        Err(error) => {
            let mut stderr = std::io::stderr();
            let _ = writeln!(stderr, "Error: {error:#}");
            let _ = stderr.flush();
            std::process::exit(1);
        }
    }
}

/// Connect to the daemon socket at `socket_path` and pump bytes between
/// `(reader, writer)` and the socket, driving the socket -> `writer` direction
/// as the lifetime anchor (see the module docs for the EOF contract).
///
/// Returns `Ok` on a clean close and `Err` on a genuine pump failure. Split out
/// from [`run`] so tests can drive the real piping path over an in-memory
/// reader/writer without touching the process's stdin/stdout — and without the
/// `std::process::exit` that `run` needs for real stdin.
async fn pipe_stdio<R, W>(socket_path: &Path, reader: R, writer: W) -> anyhow::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let stream = UnixStream::connect(socket_path).await.map_err(|source| {
        anyhow::anyhow!(
            "Cannot connect to daemon socket at {}: {source}. Is the daemon running?",
            socket_path.display()
        )
    })?;
    let (mut sock_read, mut sock_write) = stream.into_split();
    let mut reader = reader;
    let mut writer = writer;

    // Upstream: local input (stdin) -> socket. On input EOF, half-close the
    // socket write side (SHUT_WR) so the daemon sees end-of-input but can still
    // finish replying. A write or shutdown failure *after the daemon closed its
    // read side* surfaces as `BrokenPipe`/`ConnectionReset` — the normal end of
    // a half-duplex exchange, normalized to clean because reporting the close is
    // `downstream`'s job. Anything else is a genuine input or half-close failure
    // (a failed SHUT_WR can leave the daemon waiting for EOF forever), so it is
    // propagated.
    let upstream = async {
        if let Err(error) = tokio::io::copy(&mut reader, &mut sock_write).await {
            if !is_expected_peer_close(&error) {
                return Err(anyhow::Error::new(error).context("stdin -> socket pump failed"));
            }
        }
        if let Err(error) = sock_write.shutdown().await {
            if !is_expected_peer_close(&error) {
                return Err(
                    anyhow::Error::new(error).context("half-closing the socket write side failed")
                );
            }
        }
        anyhow::Ok(())
    };

    // Downstream: socket -> local output (stdout). The lifetime anchor: it
    // completes when the daemon closes its side. Unlike upstream its errors are
    // NOT normalized — a write failure here means the stdout consumer went away,
    // which fails the session and must surface as a non-zero exit.
    let downstream = async {
        tokio::io::copy(&mut sock_read, &mut writer).await?;
        writer.flush().await?;
        anyhow::Ok(())
    };

    tokio::pin!(upstream);
    tokio::pin!(downstream);

    let mut upstream_done = false;
    loop {
        tokio::select! {
            biased;
            // Daemon closed (or a stdout write failed): return the Result
            // verbatim WITHOUT waiting on `upstream`, whose real-stdin read is
            // parked in an uncancellable blocking read. This is the hang the
            // two-direction split exists to avoid.
            result = &mut downstream => return result,
            // stdin drained and the socket write half is shut down. A genuine
            // upstream failure aborts the proxy; a clean finish just means keep
            // draining `downstream` until the daemon closes.
            result = &mut upstream, if !upstream_done => {
                result?;
                upstream_done = true;
            }
        }
    }
}

/// A socket write after the daemon has closed its read side surfaces as
/// `BrokenPipe`/`ConnectionReset` — the normal end of a half-duplex exchange,
/// not a failure. Every other error kind is genuine.
fn is_expected_peer_close(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset
    )
}

#[cfg(test)]
mod tests {
    #![expect(clippy::unwrap_used, reason = "tests assert directly on fixtures")]

    use super::*;
    use std::path::PathBuf;
    use std::time::Duration;

    use bytes::BytesMut;
    use futures::{SinkExt, StreamExt};
    use mxr_protocol::{
        ClientKind, IpcCodec, IpcMessage, IpcPayload, Request, Response, ResponseData,
    };
    use tokio::net::UnixListener;
    use tokio::task::JoinHandle;
    use tokio_util::codec::{Encoder, Framed};

    /// Encode one `IpcMessage` to its on-the-wire frame bytes using the real
    /// [`IpcCodec`] — the exact bytes that travel over the socket.
    fn encode_frame(message: &IpcMessage) -> Vec<u8> {
        let mut buf = BytesMut::new();
        IpcCodec::new().encode(message.clone(), &mut buf).unwrap();
        buf.to_vec()
    }

    /// Minimal daemon stand-in: accept one connection, speak the real
    /// [`IpcCodec`], and answer every `Ping` with a `Pong` echoing the request
    /// id. Closes when the client half-closes (EOF on read).
    fn spawn_ping_responder(socket_path: PathBuf) -> JoinHandle<()> {
        let listener = UnixListener::bind(&socket_path).unwrap();
        tokio::spawn(async move {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let mut framed = Framed::new(stream, IpcCodec::new());
            while let Some(Ok(message)) = framed.next().await {
                if let IpcPayload::Request(Request::Ping) = message.payload {
                    let reply = IpcMessage {
                        id: message.id,
                        source: ClientKind::Daemon,
                        payload: IpcPayload::Response(Response::Ok {
                            data: ResponseData::Pong,
                        }),
                    };
                    if framed.send(reply).await.is_err() {
                        return;
                    }
                }
            }
        })
    }

    /// The happy path: a framed `Ping` written to the pipe's "stdin" comes back
    /// on the pipe's "stdout" as a byte-identical framed `Pong`, and nothing
    /// else lands on stdout.
    #[tokio::test]
    async fn pipes_framed_ping_response_byte_identical() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("mxr.sock");
        let _server = spawn_ping_responder(sock.clone());

        let ping = IpcMessage {
            id: 1,
            source: ClientKind::Cli,
            payload: IpcPayload::Request(Request::Ping),
        };
        let expected_pong = encode_frame(&IpcMessage {
            id: 1,
            source: ClientKind::Daemon,
            payload: IpcPayload::Response(Response::Ok {
                data: ResponseData::Pong,
            }),
        });

        let ping_bytes = encode_frame(&ping);
        // `&[u8]` is a tokio `AsyncRead` that hits EOF once drained, which
        // half-closes the socket write side and lets the server complete.
        let reader: &[u8] = &ping_bytes;
        let mut stdout: Vec<u8> = Vec::new();

        tokio::time::timeout(
            Duration::from_secs(5),
            pipe_stdio(&sock, reader, &mut stdout),
        )
        .await
        .expect("dial-stdio piping should not hang")
        .expect("dial-stdio piping should succeed");

        assert_eq!(
            stdout, expected_pong,
            "stdout must contain exactly the pong frame and nothing else"
        );
    }

    /// The failure path: with no socket to connect to, the pipe returns an
    /// error (which the process wrapper turns into a non-zero exit + stderr
    /// message) and never writes a byte to stdout.
    #[tokio::test]
    async fn connect_failure_errors_with_empty_stdout() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("absent.sock");

        let reader: &[u8] = &[];
        let mut stdout: Vec<u8> = Vec::new();

        let error = pipe_stdio(&missing, reader, &mut stdout)
            .await
            .expect_err("connecting to a missing socket must fail");

        assert!(
            stdout.is_empty(),
            "no bytes may reach stdout when the connect fails"
        );
        assert!(
            error
                .to_string()
                .contains("Cannot connect to daemon socket"),
            "error should name the connect failure, got: {error}"
        );
    }
}
