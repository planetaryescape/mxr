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
//! Downstream (socket -> stdout) failures always propagate. Upstream
//! distinguishes error sources: a stdin READ failure always propagates (even a
//! `ConnectionReset` from a dying transport — it is not the daemon's close),
//! while a socket WRITE or SHUT_WR failure is normalized to clean only when it
//! is the daemon's expected peer-close (`BrokenPipe`/`ConnectionReset`, plus
//! `NotConnected` for SHUT_WR on macOS AF_UNIX).
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

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
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

    // Upstream: local input (stdin) -> socket, then half-close (SHUT_WR) so the
    // daemon sees end-of-input. `pump_upstream` distinguishes error sources — a
    // stdin read failure is always genuine, only socket write/shutdown failures
    // after the daemon has gone are normalized to clean (reporting the close is
    // `downstream`'s job).
    let upstream = pump_upstream(&mut reader, &mut sock_write);

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

/// Pump `reader` (stdin) into `writer` (the socket write half), then half-close
/// it (SHUT_WR) on EOF.
///
/// Unlike `tokio::io::copy` — which reports reader and writer failures
/// indistinguishably — this distinguishes the error source, which the
/// normalization depends on:
/// - a **read** failure is always a genuine upstream (stdin) error and is
///   propagated, even a `ConnectionReset` from a dying transport, which must not
///   be mistaken for the daemon's peer-close;
/// - a **write** or **shutdown** failure is normalized to clean only when it is
///   the daemon's expected peer-close, so the daemon closing mid-exchange ends
///   the pump quietly and lets `downstream` report the close.
async fn pump_upstream<R, W>(reader: &mut R, writer: &mut W) -> anyhow::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        match reader.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => {
                if let Err(error) = writer.write_all(&buf[..n]).await {
                    if is_expected_write_peer_close(&error) {
                        return Ok(());
                    }
                    return Err(anyhow::Error::new(error).context("stdin -> socket write failed"));
                }
            }
            Err(error) => return Err(anyhow::Error::new(error).context("reading stdin failed")),
        }
    }
    if let Err(error) = writer.shutdown().await {
        if !is_expected_shutdown_peer_close(&error) {
            return Err(
                anyhow::Error::new(error).context("half-closing the socket write side failed")
            );
        }
    }
    Ok(())
}

/// A socket write after the daemon closed its read side surfaces as
/// `BrokenPipe`/`ConnectionReset` — the normal end of a half-duplex exchange,
/// not a failure. Every other kind is genuine.
fn is_expected_write_peer_close(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset
    )
}

/// `shutdown(SHUT_WR)` after the peer already closed additionally surfaces as
/// `NotConnected` (ENOTCONN) on macOS AF_UNIX sockets — also expected. (Writes
/// have not been observed to return ENOTCONN, so the write path keeps the
/// narrower set.)
fn is_expected_shutdown_peer_close(error: &std::io::Error) -> bool {
    is_expected_write_peer_close(error) || error.kind() == std::io::ErrorKind::NotConnected
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

    // --- pump_upstream error-source classification ---

    use std::pin::Pin;
    use std::task::{Context, Poll};
    use tokio::io::ReadBuf;

    /// An `AsyncRead` that fails its first read with a chosen error kind.
    struct FailingReader(std::io::ErrorKind);

    impl AsyncRead for FailingReader {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &mut ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            Poll::Ready(Err(std::io::Error::from(self.get_mut().0)))
        }
    }

    #[derive(Clone, Copy)]
    enum FailOn {
        Write,
        Shutdown,
    }

    /// An `AsyncWrite` that fails at a chosen point (a write, or the shutdown)
    /// with a chosen error kind; every other operation succeeds.
    struct FailingWriter {
        fail_on: FailOn,
        kind: std::io::ErrorKind,
    }

    impl AsyncWrite for FailingWriter {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<std::io::Result<usize>> {
            let this = self.get_mut();
            match this.fail_on {
                FailOn::Write => Poll::Ready(Err(std::io::Error::from(this.kind))),
                FailOn::Shutdown => Poll::Ready(Ok(buf.len())),
            }
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            let this = self.get_mut();
            match this.fail_on {
                FailOn::Shutdown => Poll::Ready(Err(std::io::Error::from(this.kind))),
                FailOn::Write => Poll::Ready(Ok(())),
            }
        }
    }

    /// A stdin read error is ALWAYS genuine — even a `ConnectionReset`, which on
    /// the write side would be normalized. It must never be mistaken for the
    /// daemon's peer-close.
    #[tokio::test]
    async fn pump_upstream_read_error_always_propagates() {
        let mut reader = FailingReader(std::io::ErrorKind::ConnectionReset);
        let mut sink = tokio::io::sink();
        let error = pump_upstream(&mut reader, &mut sink)
            .await
            .expect_err("a stdin read error must propagate");
        assert!(
            error.to_string().contains("reading stdin failed"),
            "got: {error}"
        );
    }

    /// A socket write failing with `BrokenPipe` after the daemon closed its read
    /// side is the expected end of a half-duplex exchange — normalized to clean.
    #[tokio::test]
    async fn pump_upstream_write_broken_pipe_is_normalized() {
        let mut reader: &[u8] = b"frame-bytes";
        let mut writer = FailingWriter {
            fail_on: FailOn::Write,
            kind: std::io::ErrorKind::BrokenPipe,
        };
        pump_upstream(&mut reader, &mut writer)
            .await
            .expect("a peer-close write error must be normalized to clean");
    }

    /// A genuine socket write error (not a peer-close) propagates.
    #[tokio::test]
    async fn pump_upstream_genuine_write_error_propagates() {
        let mut reader: &[u8] = b"frame-bytes";
        let mut writer = FailingWriter {
            fail_on: FailOn::Write,
            kind: std::io::ErrorKind::PermissionDenied,
        };
        let error = pump_upstream(&mut reader, &mut writer)
            .await
            .expect_err("a non-peer-close write error must propagate");
        assert!(
            error.to_string().contains("stdin -> socket write failed"),
            "got: {error}"
        );
    }

    /// On macOS AF_UNIX, `shutdown(SHUT_WR)` after the peer closed returns
    /// `NotConnected` — expected, normalized to clean.
    #[tokio::test]
    async fn pump_upstream_shutdown_not_connected_is_normalized() {
        let mut reader: &[u8] = &[];
        let mut writer = FailingWriter {
            fail_on: FailOn::Shutdown,
            kind: std::io::ErrorKind::NotConnected,
        };
        pump_upstream(&mut reader, &mut writer)
            .await
            .expect("ENOTCONN on SHUT_WR must be normalized to clean");
    }

    /// A genuine shutdown error (not a peer-close) propagates.
    #[tokio::test]
    async fn pump_upstream_genuine_shutdown_error_propagates() {
        let mut reader: &[u8] = &[];
        let mut writer = FailingWriter {
            fail_on: FailOn::Shutdown,
            kind: std::io::ErrorKind::PermissionDenied,
        };
        let error = pump_upstream(&mut reader, &mut writer)
            .await
            .expect_err("a non-peer-close shutdown error must propagate");
        assert!(
            error
                .to_string()
                .contains("half-closing the socket write side failed"),
            "got: {error}"
        );
    }
}
