//! `mxr daemon dial-stdio` — pipe raw bytes between this process's
//! stdin/stdout and the local daemon's Unix socket.
//!
//! This is the Docker `connhelper` move (see
//! `docs/transport-adapters/05-tcp-stdio-adapters.md` §5c): a byte pump that
//! lets any transport which can exec a process and pipe stdio reach the daemon
//! — `ssh host mxr daemon dial-stdio`,
//! `docker exec -i <container> mxr daemon dial-stdio`, and any community bridge
//! that can spawn a process. No new daemon trust surface: the caller still
//! needs local Unix-socket access on the daemon's machine.
//!
//! ## stdout discipline
//!
//! Once piping begins, stdout carries only socket bytes. Nothing on this path
//! writes to stdout:
//! - daemon autostart status (`ensure_daemon_running`) prints only to stderr;
//! - tracing is file-only in this mode (`init_tracing(false)`, enforced by the
//!   dispatcher in `lib.rs`), so no log line can reach stdout;
//! - a connect failure returns an error that the process wrapper renders to
//!   stderr and turns into a non-zero exit.

use std::path::Path;

use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::UnixStream;

/// Entry point for `mxr daemon dial-stdio`.
///
/// Autostarts the local daemon per repo convention, then pipes this process's
/// stdin/stdout to the daemon socket until either side closes.
pub async fn run() -> anyhow::Result<()> {
    // Convention: CLI commands ensure the local daemon is up before talking to
    // it. Every progress message `ensure_daemon_running` emits goes to stderr,
    // so stdout stays byte-clean before the first piped socket byte.
    crate::server::ensure_daemon_running().await?;
    let socket_path = crate::state::AppState::socket_path();
    pipe_stdio(&socket_path, tokio::io::stdin(), tokio::io::stdout()).await
}

/// Connect to the daemon socket at `socket_path` and pump bytes bidirectionally
/// between `(reader, writer)` and the socket until either side closes.
///
/// Split out from [`run`] so tests can drive the real piping path over an
/// in-memory reader/writer without touching the process's stdin/stdout.
async fn pipe_stdio<R, W>(socket_path: &Path, reader: R, writer: W) -> anyhow::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut stream = UnixStream::connect(socket_path).await.map_err(|source| {
        anyhow::anyhow!(
            "Cannot connect to daemon socket at {}: {source}. Is the daemon running?",
            socket_path.display()
        )
    })?;
    // `join` presents (reader, writer) as one `AsyncRead + AsyncWrite`, so
    // `copy_bidirectional` pumps stdin -> socket and socket -> stdout at once.
    let mut local = tokio::io::join(reader, writer);
    tokio::io::copy_bidirectional(&mut local, &mut stream).await?;
    Ok(())
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
