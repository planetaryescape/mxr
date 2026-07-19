//! Unix domain socket transport — the default, and the only production
//! transport this phase.
//!
//! This absorbs the socket *lifecycle* that used to live inline in the daemon's
//! `server.rs`: bind, `chmod 0600`, stale-socket cleanup, and successor
//! detection (so a restart's exit cleanup never deletes a successor daemon's
//! freshly-bound socket). The daemon keeps the pieces that are *daemon*
//! lifecycle, not transport lifecycle — the pid file and the search-index
//! singleton lock. Bind is only ever called after the daemon has acquired that
//! lock, which is what makes the "any socket present now is genuinely stale,
//! clear it and bind ours" step below safe.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tokio::net::{UnixListener, UnixStream};

use crate::error::{Result, TransportError};
use crate::peer::{PeerAuth, PeerInfo};
use crate::{
    AuthCaps, BoxedIo, Connector, LifecycleCaps, LocalityCaps, ServerTransport,
    TransportCapabilities, TransportListener,
};

/// The Unix domain socket server transport. Binds a listener at a fixed path.
#[derive(Debug, Clone)]
pub struct UdsServerTransport {
    path: PathBuf,
}

impl UdsServerTransport {
    /// A UDS transport that binds at `path`.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

#[async_trait]
impl ServerTransport for UdsServerTransport {
    fn name(&self) -> &str {
        "uds"
    }

    fn capabilities(&self) -> TransportCapabilities {
        TransportCapabilities {
            locality: LocalityCaps { same_machine: true },
            auth: AuthCaps {
                implicit_peer_identity: true,
                token: false,
            },
            lifecycle: LifecycleCaps {
                client_autostart: true,
            },
        }
    }

    async fn bind(&self) -> Result<Box<dyn TransportListener>> {
        let endpoint = unix_endpoint(&self.path);
        // The daemon holds the exclusive index lock before calling bind, so any
        // socket file present now is genuinely stale (left by a fully-exited
        // daemon); clear it and bind ours.
        let _ = std::fs::remove_file(&self.path);
        let listener = UnixListener::bind(&self.path).map_err(|source| TransportError::Bind {
            endpoint: endpoint.clone(),
            source,
        })?;
        set_socket_permissions(&self.path).map_err(|source| TransportError::Bind {
            endpoint: endpoint.clone(),
            source,
        })?;
        // Remember which socket file is OURS. During an upgrade restart a
        // successor daemon can re-bind this path while we are still draining;
        // cleanup must not delete the successor's socket.
        let identity = socket_file_identity(&self.path);
        Ok(Box::new(UdsListener {
            path: self.path.clone(),
            listener,
            identity,
            cleaned: false,
        }))
    }
}

/// A bound UDS listener. Owns the socket file it must remove on cleanup.
struct UdsListener {
    path: PathBuf,
    listener: UnixListener,
    identity: Option<(u64, u64, i64, i64)>,
    cleaned: bool,
}

#[async_trait]
impl TransportListener for UdsListener {
    async fn accept(&mut self) -> Result<(BoxedIo, PeerInfo)> {
        let (stream, _addr) = self
            .listener
            .accept()
            .await
            .map_err(TransportError::Accept)?;
        let peer = peer_info(&stream);
        Ok((Box::new(stream), peer))
    }

    async fn cleanup(&mut self) -> Result<()> {
        if self.cleaned {
            return Ok(());
        }
        self.cleaned = true;
        remove_socket_if_owned(&self.path, self.identity);
        Ok(())
    }

    fn endpoint(&self) -> String {
        unix_endpoint(&self.path)
    }
}

/// Read the peer's Unix credentials off a just-accepted stream. tokio's
/// `peer_cred` is cross-platform (`SO_PEERCRED` on Linux, `getpeereid` on macOS
/// / the BSDs). A `peer_cred` failure on a connected AF_UNIX socket is a rare
/// I/O anomaly; it must not break serving (today no policy reads the creds), so
/// we log and fall back to an *unresolved* `UnixPeer` sentinel
/// (`uid`/`gid` == `u32::MAX`) rather than dropping the connection.
fn peer_info(stream: &UnixStream) -> PeerInfo {
    match stream.peer_cred() {
        Ok(cred) => PeerInfo {
            auth: PeerAuth::UnixPeer {
                uid: cred.uid(),
                gid: cred.gid(),
                pid: cred.pid(),
            },
        },
        Err(error) => {
            tracing::warn!(%error, "could not read UDS peer credentials; using unresolved sentinel");
            PeerInfo {
                auth: PeerAuth::UnixPeer {
                    uid: u32::MAX,
                    gid: u32::MAX,
                    pid: None,
                },
            }
        }
    }
}

/// The Unix domain socket client connector — dials a daemon socket at `path`.
#[derive(Debug, Clone)]
pub struct UnixConnector {
    path: PathBuf,
}

impl UnixConnector {
    /// A connector that dials the daemon socket at `path`.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

#[async_trait]
impl Connector for UnixConnector {
    async fn connect(&self) -> Result<BoxedIo> {
        let stream =
            UnixStream::connect(&self.path)
                .await
                .map_err(|source| TransportError::Connect {
                    endpoint: unix_endpoint(&self.path),
                    source,
                })?;
        Ok(Box::new(stream))
    }

    fn describe(&self) -> String {
        unix_endpoint(&self.path)
    }
}

fn unix_endpoint(path: &Path) -> String {
    format!("unix:{}", path.display())
}

#[cfg(unix)]
fn set_socket_permissions(sock_path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::set_permissions(sock_path, std::fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn set_socket_permissions(_sock_path: &Path) -> std::io::Result<()> {
    Ok(())
}

/// Identity of the file currently at `path`, used to detect whether the socket
/// we bound is still the one on disk. `(dev, ino)` alone is not enough: ext4
/// recycles inode numbers, so a successor daemon's freshly bound socket can land
/// on the inode we just freed. The bind timestamp disambiguates a recycled
/// inode. Caveat: Linux stamps files from the kernel's coarse clock (~ms ticks),
/// so two binds inside one tick could collide — irrelevant here because
/// successor binds are separated from ours by a full daemon startup.
#[cfg(unix)]
fn socket_file_identity(path: &Path) -> Option<(u64, u64, i64, i64)> {
    use std::os::unix::fs::MetadataExt;

    std::fs::metadata(path)
        .ok()
        .map(|m| (m.dev(), m.ino(), m.mtime(), m.mtime_nsec()))
}

#[cfg(not(unix))]
fn socket_file_identity(_path: &Path) -> Option<(u64, u64, i64, i64)> {
    None
}

/// Remove the socket file only if it is still the one this listener bound.
/// `owned` of `None` (identity capture failed at bind time) falls back to
/// unconditional removal, matching the previous behavior.
fn remove_socket_if_owned(path: &Path, owned: Option<(u64, u64, i64, i64)>) {
    match (owned, socket_file_identity(path)) {
        (Some(ours), Some(current)) if ours != current => {
            tracing::info!(
                "leaving IPC socket in place: the path was re-bound by a successor daemon"
            );
        }
        _ => {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    #![expect(clippy::unwrap_used, reason = "tests assert directly on fixtures")]

    use super::{remove_socket_if_owned, socket_file_identity};
    use std::time::Duration;

    #[test]
    fn socket_removal_skips_a_rebound_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mxr.sock");

        // Same inode -> removed (normal shutdown).
        std::fs::write(&path, b"ours").unwrap();
        let ours = socket_file_identity(&path);
        assert!(ours.is_some());
        remove_socket_if_owned(&path, ours);
        assert!(!path.exists(), "own socket should be removed");

        // Path re-created by a successor -> left alone. ext4 can recycle the
        // freed inode AND Linux stamps files from the kernel's coarse clock
        // (~ms ticks), so a same-tick re-create can be byte-identical to ours.
        // Sleep past the tick — in production the two binds are separated by a
        // full daemon startup.
        std::fs::write(&path, b"ours").unwrap();
        let ours = socket_file_identity(&path);
        std::fs::remove_file(&path).unwrap();
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(&path, b"successor").unwrap();
        remove_socket_if_owned(&path, ours);
        assert!(path.exists(), "successor's socket must survive our cleanup");

        // Unknown identity (capture failed at bind) -> fall back to removal.
        remove_socket_if_owned(&path, None);
        assert!(!path.exists(), "unknown ownership falls back to removal");
    }
}
