//! `cmd://` connector (phase 5c, transport-adapter initiative).
//!
//! The Docker `connhelper` model as a client transport: spawn a command and
//! treat its stdio as the byte stream. `MXR_DAEMON_ADDR="cmd://ssh -T host mxr
//! daemon dial-stdio"` makes SSH remoting, `docker exec`, and any community
//! bridge that can exec-and-pipe a process work for every mxr client uniformly.
//! The entire "community transport plugin" system is an executable that speaks
//! frames on stdin/stdout.
//!
//! The child's stdout feeds our reader, our writes feed its stdin, and its
//! stderr passes through to our stderr (so `ssh` prompts / errors stay visible).
//! The child is signalled to die when the stream is dropped (`kill_on_drop`;
//! the runtime driver reaps the zombie best-effort), so a client that finishes
//! or errors tears the process down rather than leaking it. On the normal path
//! the child exits on its own when its stdin hits EOF.
//!
//! ## argv & quoting
//!
//! The command comes from [`crate::TransportAddr::Cmd`], which splits the
//! `cmd://` body on ASCII whitespace — no shell quoting. See
//! [`crate::TransportAddr::parse`] for the limitation and the wrap-in-a-script
//! escape hatch.

use std::pin::Pin;
use std::process::Stdio;
use std::task::{Context, Poll};

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncWrite, Join, ReadBuf};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

use crate::error::{Result, TransportError};
use crate::{BoxedIo, Connector};

/// A connector that dials the daemon by spawning a command and wrapping its
/// stdio as the byte stream.
#[derive(Debug, Clone)]
pub struct CmdConnector {
    argv: Vec<String>,
}

impl CmdConnector {
    /// A connector that spawns `argv` (program + arguments). `argv` must be
    /// non-empty; [`crate::TransportAddr::parse`] guarantees that for
    /// `cmd://`-derived commands.
    #[must_use]
    pub fn new(argv: Vec<String>) -> Self {
        Self { argv }
    }
}

#[async_trait]
impl Connector for CmdConnector {
    async fn connect(&self) -> Result<BoxedIo> {
        let Some((program, args)) = self.argv.split_first() else {
            return Err(TransportError::Connect {
                endpoint: self.describe(),
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "cmd:// connector has an empty command",
                ),
            });
        };

        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // stderr passes through to ours: ssh host-key prompts, connection
            // errors, and the remote's own diagnostics stay visible to the user.
            .stderr(Stdio::inherit())
            // Send a kill signal when the stream drops, so a finished or aborted
            // client tears the child down instead of leaking it. Reaping is
            // best-effort per tokio's docs (the runtime driver reaps the zombie
            // asynchronously once the signal lands); this is acceptable for a
            // short-lived per-request dialer, and normal completion — stdin EOF
            // → the child exits on its own — never relies on it.
            .kill_on_drop(true)
            .spawn()
            .map_err(|source| TransportError::Connect {
                endpoint: self.describe(),
                source,
            })?;

        // Take the piped handles out of the child; the `Child` stays owned by
        // the stream purely to keep the process alive (and reap it on drop).
        let stdout = child.stdout.take().ok_or_else(|| TransportError::Connect {
            endpoint: self.describe(),
            source: std::io::Error::other("cmd:// child produced no stdout pipe"),
        })?;
        let stdin = child.stdin.take().ok_or_else(|| TransportError::Connect {
            endpoint: self.describe(),
            source: std::io::Error::other("cmd:// child produced no stdin pipe"),
        })?;

        Ok(Box::new(CmdStream {
            io: tokio::io::join(stdout, stdin),
            _child: child,
        }))
    }

    fn describe(&self) -> String {
        format!("cmd://{}", self.argv.join(" "))
    }
}

/// The child process's stdio as one bidirectional byte stream. Reads come from
/// the child's stdout, writes go to its stdin; the `Child` is held so the
/// process lives exactly as long as the connection (and is killed+reaped on
/// drop). All the handles are `Unpin`, so the delegating impls need no pinning
/// gymnastics.
struct CmdStream {
    io: Join<ChildStdout, ChildStdin>,
    _child: Child,
}

impl AsyncRead for CmdStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().io).poll_read(cx, buf)
    }
}

impl AsyncWrite for CmdStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.get_mut().io).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().io).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().io).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        clippy::panic,
        reason = "tests assert directly on fixtures"
    )]

    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn describe_reconstructs_the_command() {
        let connector = CmdConnector::new(vec!["ssh".into(), "-T".into(), "host".into()]);
        assert_eq!(connector.describe(), "cmd://ssh -T host");
    }

    /// `cat` echoes stdin to stdout: a full round-trip proves the child's stdio
    /// is wired as a bidirectional byte stream.
    #[tokio::test]
    async fn spawns_and_pipes_stdio_round_trip() {
        let connector = CmdConnector::new(vec!["cat".into()]);
        let mut stream = connector.connect().await.unwrap();

        stream.write_all(b"hello over cmd://\n").await.unwrap();
        stream.flush().await.unwrap();

        let mut buf = [0u8; 18];
        stream.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"hello over cmd://\n");
    }

    #[tokio::test]
    async fn connect_fails_for_a_missing_program() {
        let connector = CmdConnector::new(vec!["mxr-no-such-binary-xyz".into()]);
        match connector.connect().await {
            Err(TransportError::Connect { .. }) => {}
            Ok(_) => panic!("spawning a missing program must fail"),
            Err(other) => panic!("expected a Connect error, got {other:?}"),
        }
    }
}
