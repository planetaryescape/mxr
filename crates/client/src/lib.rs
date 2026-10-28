//! Shared IPC client for the mxr daemon.
//!
//! Every mxr client — the CLI/daemon-internal `IpcClient`, the TUI worker, the
//! web bridge, and the MCP server — needs the same three things: open the
//! daemon's Unix socket, send a length-delimited [`Request`] frame, and read
//! the correlated [`Response`] while forwarding any interleaved
//! [`DaemonEvent`]s. This crate is the single implementation of that
//! mechanism. Reconnect loops, autostart, retry classification, and per-client
//! error shaping stay *policy* owned by each consumer; this crate only owns the
//! connection.
//!
//! The byte stream is abstracted behind [`mxr_transport::Connector`]
//! (phase 4): [`IpcConnection::connect_with`] takes any connector, while
//! [`IpcConnection::connect`] keeps the path-based Unix-socket entry point every
//! consumer already calls (it builds a [`mxr_transport::UnixConnector`]
//! internally). The connection holds a [`mxr_transport::BoxedIo`], so the same
//! client speaks to the daemon over any transport.
//!
//! ```no_run
//! # async fn demo() -> Result<(), mxr_client::ClientError> {
//! use mxr_client::IpcConnection;
//! use mxr_protocol::{ClientKind, Request};
//! use std::path::Path;
//!
//! let mut conn = IpcConnection::connect(Path::new("/tmp/mxr.sock"), ClientKind::Cli).await?;
//! let data = conn.request(Request::Ping).await?;
//! # let _ = data;
//! # Ok(())
//! # }
//! ```

pub mod remote_connection;
pub mod selected_daemon;
pub use selected_daemon::SelectedDaemonTarget;
mod archive;
pub use archive::ArchiveJournal;
mod attachment_transfer;
mod draft_attachment_upload;
pub use attachment_transfer::{download_attachment_to, DownloadedAttachment};

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use mxr_protocol::{
    ClientKind, DaemonEvent, IpcCodec, IpcErrorKind, IpcMessage, IpcPayload, Request, Response,
    ResponseData,
};
use mxr_transport::{BoxedIo, Connector, TransportError, UnixConnector};
use tokio_util::codec::Framed;

// Re-exported so consumers can select a daemon without taking a direct
// mxr-transport dependency. The web bridge retains its Unix-only resolver.
pub use mxr_transport::TransportAddr;

/// Whether a client may start a local daemon when its selected Unix socket is
/// unavailable. Supervised installations set this to `off` so only their
/// service manager owns daemon creation and replacement.
pub fn daemon_autostart_enabled() -> bool {
    daemon_autostart_enabled_for(std::env::var("MXR_DAEMON_AUTOSTART").ok().as_deref())
}

fn daemon_autostart_enabled_for(value: Option<&str>) -> bool {
    !value.is_some_and(|value| value.eq_ignore_ascii_case("off"))
}

/// Remediation shared by clients that cannot reach a daemon while autostart
/// is disabled.
pub fn daemon_autostart_disabled_message() -> &'static str {
    "MXR_DAEMON_AUTOSTART=off and no compatible daemon is available; check the service manager and its logs, then restart the supervised service"
}

/// Build identity used by the daemon and clients that must leave a supervised
/// daemon untouched. The executable path and file metadata distinguish local
/// builds that share a package version.
pub fn current_daemon_build_id() -> String {
    let version = env!("CARGO_PKG_VERSION");
    let Ok(exe) = std::env::current_exe() else {
        return format!("{version}:unknown");
    };
    let path = std::fs::canonicalize(&exe).unwrap_or(exe);
    let Ok(meta) = std::fs::metadata(&path) else {
        return format!("{version}:{}", path.display());
    };
    let modified = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_secs());
    format!("{version}:{}:{}:{modified}", path.display(), meta.len())
}

/// Whether the reported daemon identity exactly matches this running binary.
pub fn daemon_matches_current_binary(
    protocol_version: u32,
    daemon_version: Option<&str>,
    daemon_build_id: Option<&str>,
) -> bool {
    protocol_version == mxr_protocol::IPC_PROTOCOL_VERSION
        && daemon_version == Some(env!("CARGO_PKG_VERSION"))
        && daemon_build_id == Some(current_daemon_build_id().as_str())
}

#[cfg(test)]
mod daemon_autostart_policy_tests {
    use super::daemon_autostart_enabled_for;

    #[test]
    fn autostart_stays_compatible_unless_explicitly_disabled() {
        assert!(daemon_autostart_enabled_for(None));
        assert!(daemon_autostart_enabled_for(Some("on")));
        assert!(daemon_autostart_enabled_for(Some("false")));
        assert!(!daemon_autostart_enabled_for(Some("off")));
        assert!(!daemon_autostart_enabled_for(Some("OFF")));
    }
}

/// Resolve `MXR_DAEMON_ADDR` to a Unix socket path for unix-only clients. Thin
/// wrapper over [`TransportAddr::resolve_unix_socket`] so callers depend only on
/// `mxr-client`.
pub fn resolve_unix_socket(
    default_socket: std::path::PathBuf,
) -> Result<std::path::PathBuf, TransportError> {
    TransportAddr::resolve_unix_socket(default_socket)
}

/// One vocabulary for every way a daemon exchange can fail.
///
/// Consumers map these onto their own error surfaces (the TUI's retry
/// classifier, the bridge's HTTP status mapping, the CLI's `anyhow` messages)
/// but the failure *classes* are shared: a connect failure, a socket I/O or
/// framing error, a kinded daemon-side error, a timeout, or a clean close.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    /// Establishing the connection to the daemon socket failed.
    #[error("cannot connect to daemon at {path}: {source}")]
    Connect {
        /// The socket path that could not be reached.
        path: PathBuf,
        /// The underlying connect error (its [`std::io::ErrorKind`] drives
        /// autostart decisions in policy code).
        #[source]
        source: std::io::Error,
    },

    /// An I/O or framing error occurred sending or receiving a frame. The
    /// [`IpcCodec`] surfaces a malformed/undecodable frame as
    /// [`std::io::ErrorKind::InvalidData`], so protocol-version mismatches
    /// arrive here too.
    #[error("{0}")]
    Io(#[from] std::io::Error),

    /// The daemon answered with a [`Response::Error`]. Only produced by the
    /// [`IpcConnection::request`] / [`IpcConnection::request_with_events`]
    /// helpers that unwrap to [`ResponseData`]; callers wanting the error
    /// response verbatim use [`IpcConnection::request_response`].
    #[error("{message}")]
    Daemon {
        /// Human-readable daemon error message.
        message: String,
        /// Machine-readable failure class.
        kind: IpcErrorKind,
        /// Whether the daemon flagged the failure as retryable.
        retryable: bool,
        code: String,
        details: Option<serde_json::Value>,
    },

    /// A frame arrived that does not correlate to the in-flight request: a
    /// `Response` carrying a different id, or a non-response frame. This is
    /// fail-fast — at this point the framed stream is out of step with the
    /// request/response sequence, so the connection is unusable. Consumers that
    /// historically skipped such frames now surface this (a documented, and in
    /// practice unreachable, deviation); each maps it back onto its own error
    /// surface via the `frame_id`/`expected_id`/`is_response` fields.
    #[error("unexpected IPC frame (id {frame_id}) while awaiting response {expected_id}")]
    UnexpectedFrame {
        /// Id carried by the rogue frame.
        frame_id: u64,
        /// Id of the request still awaiting its response.
        expected_id: u64,
        /// True when the rogue frame was itself a `Response` (wrong id); false
        /// for a non-response frame.
        is_response: bool,
    },

    /// The request exceeded its configured timeout without a response.
    #[error("IPC request timed out after {} seconds", .0.as_secs())]
    Timeout(Duration),

    /// The connection closed before the response arrived.
    #[error("connection closed")]
    Closed,

    /// A [`Connector`] failed to establish the connection. Produced by
    /// [`IpcConnection::connect_with`]; the path-based [`IpcConnection::connect`]
    /// re-maps a connect failure onto the historical [`ClientError::Connect`]
    /// shape so its consumers keep matching on `{ path, source }`.
    #[error("{0}")]
    Transport(#[from] TransportError),
}

/// A single framed connection to the daemon over its Unix socket.
///
/// Requests sent on one connection are correlated to their responses by an
/// internal monotonic id; interleaved [`DaemonEvent`] frames (id `0`) are
/// surfaced to the caller's callback or via [`Self::next_event`]. A connection
/// carries exactly one in-flight request at a time — the model every mxr client
/// already uses.
pub struct IpcConnection {
    framed: Framed<BoxedIo, IpcCodec>,
    next_id: AtomicU64,
    source: ClientKind,
    default_timeout: Option<Duration>,
    attachment_transfer_supported: Option<bool>,
    // TCP/command targets restrict local provisioning without paired authority.
    selected_remote: bool,
    preview_attachment_declarations:
        std::collections::HashMap<mxr_core::DraftId, Vec<mxr_core::DraftAttachment>>,
    remote_workspace: Option<mxr_protocol::ArchiveWorkspaceData>,
    remote_registration_identity: Option<(String, String)>,
    selection_generation: Option<u64>,
}

impl IpcConnection {
    pub fn is_paired_remote(&self) -> bool {
        self.remote_registration_identity.is_some()
    }

    /// Carry byte reviews only across reconnection of the same selected session.
    /// A refused replacement leaves the original review intact for another attempt.
    pub fn retain_draft_attachment_previews_from(
        &mut self,
        previous: &mut Self,
    ) -> Result<(), ClientError> {
        if self.selected_remote != previous.selected_remote
            || self.remote_workspace != previous.remote_workspace
            || self.remote_registration_identity != previous.remote_registration_identity
            || self.selection_generation != previous.selection_generation
        {
            return Err(workspace_error(
                "workspace_conflict",
                "Connection changed; reopen and review the draft on its original workspace",
            ));
        }
        for (draft, declarations) in std::mem::take(&mut previous.preview_attachment_declarations) {
            self.preview_attachment_declarations
                .entry(draft)
                .or_insert(declarations);
        }
        Ok(())
    }

    pub(crate) fn with_selection_generation(mut self, generation: u64) -> Self {
        self.selection_generation = Some(generation);
        self
    }

    /// Connect to the daemon at `path`, tagging every request with `source`.
    ///
    /// The `source` tag rides on each [`IpcMessage`] so the daemon's activity
    /// recorder attributes traffic to the right surface — making the class of
    /// bug where a client mislabels itself structurally impossible.
    ///
    /// Builds a [`UnixConnector`] internally and delegates to
    /// [`Self::connect_with`], re-mapping a connect failure onto the historical
    /// [`ClientError::Connect`] shape (its `source.kind()` still drives
    /// autostart decisions in policy code).
    pub async fn connect(path: &Path, source: ClientKind) -> Result<Self, ClientError> {
        Self::connect_with(&UnixConnector::new(path.to_path_buf()), source)
            .await
            .map_err(|error| match error {
                ClientError::Transport(TransportError::Connect {
                    source: io_error, ..
                }) => ClientError::Connect {
                    path: path.to_path_buf(),
                    source: io_error,
                },
                other => other,
            })
    }

    /// Connect to a daemon owned by a service manager and verify its exact
    /// identity before returning a connection that can issue normal requests.
    /// A status failure is actionable and never starts, stops, or replaces the
    /// supervised process.
    pub async fn connect_supervised(path: &Path, source: ClientKind) -> Result<Self, ClientError> {
        let mut connection = Self::connect(path, source).await?;
        let status = tokio::time::timeout(
            Duration::from_secs(5),
            connection.request(Request::GetStatus),
        )
        .await
        .map_err(|_| supervised_connection_error(path, "daemon status request timed out"))?
        .map_err(|error| {
            supervised_connection_error(path, &format!("daemon status request failed: {error}"))
        })?;

        match status {
            ResponseData::Status {
                protocol_version,
                daemon_version,
                daemon_build_id,
                ..
            } if daemon_matches_current_binary(
                protocol_version,
                daemon_version.as_deref(),
                daemon_build_id.as_deref(),
            ) =>
            {
                Ok(connection)
            }
            ResponseData::Status { .. } => Err(supervised_connection_error(
                path,
                "the running daemon is incompatible with this mxr binary",
            )),
            _ => Err(supervised_connection_error(
                path,
                "the daemon returned an unexpected response to its status request",
            )),
        }
    }

    /// Connect to the daemon through any [`Connector`], tagging every request
    /// with `source`. The transport-agnostic entry point: the connection holds a
    /// [`BoxedIo`], so the same framing/correlation machinery serves whatever
    /// byte stream the connector produced.
    pub async fn connect_with(
        connector: &dyn Connector,
        source: ClientKind,
    ) -> Result<Self, ClientError> {
        let io = connector.connect().await?;
        let mut conn = Self {
            framed: Framed::new(io, IpcCodec::new()),
            next_id: AtomicU64::new(1),
            source,
            default_timeout: None,
            attachment_transfer_supported: None,
            selected_remote: false,
            preview_attachment_declarations: std::collections::HashMap::new(),
            remote_workspace: None,
            remote_registration_identity: None,
            selection_generation: None,
        };
        // Token-bearing transports (the TCP-loopback adapter) authenticate
        // in-band as the FIRST request on the connection. The byte-stream seam
        // stays protocol-free — the connector only advertises its token; the
        // framed handshake lives here. Implicit-trust transports (UDS, memory,
        // stdio, cmd) advertise no token and skip this entirely.
        if let Some(token) = connector.auth_token() {
            conn.authenticate(token.to_string()).await?;
        }
        Ok(conn)
    }

    /// Perform the in-band token handshake: send `Authenticate` and require an
    /// `Authenticated` reply. Any daemon-side rejection surfaces as
    /// [`ClientError::Daemon`] with [`IpcErrorKind::Auth`]; an unexpected
    /// success payload is a protocol error.
    async fn authenticate(&mut self, token: String) -> Result<(), ClientError> {
        match self.request(Request::Authenticate { token }).await? {
            ResponseData::Authenticated => Ok(()),
            other => Err(ClientError::Daemon {
                message: format!("unexpected response to Authenticate: {other:?}"),
                kind: IpcErrorKind::Auth,
                retryable: false,
                code: "auth".into(),
                details: None,
            }),
        }
    }

    /// Set the timeout applied by [`Self::request`] (per-connection default).
    /// `None` — the default — means no timeout is applied by the connection
    /// itself.
    #[must_use]
    pub fn with_default_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.default_timeout = timeout;
        self
    }

    /// Seed the correlation-id counter. Connection-per-request callers that
    /// carry an externally-chosen request id (e.g. the web bridge's log/wire
    /// correlation id) use this so the id on the wire matches.
    #[must_use]
    pub fn with_start_id(mut self, start: u64) -> Self {
        self.next_id = AtomicU64::new(start);
        self
    }

    fn take_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    fn envelope(&self, id: u64, request: Request) -> IpcMessage {
        IpcMessage {
            expected_workspace: None,
            id,
            source: self.source,
            payload: IpcPayload::Request(request),
        }
    }

    /// Send `request` and return the daemon's correlated [`Response`] verbatim
    /// (both `Ok` and `Error` variants), forwarding any interleaved
    /// [`DaemonEvent`] frames to `on_event`. Applies `request_timeout` when
    /// `Some`.
    ///
    /// `on_event` runs synchronously between frame reads — keep it fast (write
    /// to a stream or push into a channel; never block).
    ///
    /// This is the low-level primitive: connection, timeout, and framing
    /// failures map to [`ClientError`], but a `Response::Error` from the daemon
    /// is returned as `Ok(Response::Error { .. })` so callers can inspect its
    /// full envelope. Use [`Self::request`] for the [`ResponseData`]-or-
    /// [`ClientError::Daemon`] shape.
    pub async fn request_response<F>(
        &mut self,
        request: Request,
        on_event: F,
        request_timeout: Option<Duration>,
    ) -> Result<Response, ClientError>
    where
        F: FnMut(DaemonEvent),
    {
        self.request_response_owned(request, None, on_event, request_timeout)
            .await
    }

    /// Ownership belongs to this command, never to mutable connection state.
    pub async fn request_response_owned<F>(
        &mut self,
        request: Request,
        expected_workspace: Option<&mxr_protocol::ArchiveWorkspaceData>,
        mut on_event: F,
        request_timeout: Option<Duration>,
    ) -> Result<Response, ClientError>
    where
        F: FnMut(DaemonEvent),
    {
        remote_connection::schedule_cleanup();
        self.verify_selection().await?;
        if self.is_paired_remote() && matches!(request, Request::GetArchiveWorkspace) {
            let workspace = self.capture_workspace_with_events(&mut on_event).await?;
            return Ok(Response::Ok {
                data: ResponseData::ArchiveWorkspace { workspace },
            });
        }
        if self.selected_remote && selected_daemon::requires_local_account_setup(&request) {
            return Err(ClientError::Io(std::io::Error::other(
                "Account provisioning and credential/config maintenance are unsupported through a selected remote daemon; run account setup on the daemon host",
            )));
        }
        if request.requires_workspace_owner() && expected_workspace.is_none() {
            return Err(ClientError::Daemon {
                message: "This private command has no original workspace owner. Keep the work and explicitly reopen/review it before continuing.".into(),
                kind: IpcErrorKind::InvalidRequest,
                code: "workspace_owner_required".into(),
                retryable: false,
                details: None,
            });
        }
        if let Some(workspace) = expected_workspace {
            self.verify_workspace_with_events(workspace, &mut on_event)
                .await?;
        }
        let safety_preview =
            self.is_paired_remote() && matches!(request, Request::CheckDraftSafety { .. });
        let request = match self
            .prepare_draft_attachment_request(request, expected_workspace)
            .await
        {
            Ok(request) => request,
            Err(ClientError::Daemon {
                message,
                kind,
                code,
                retryable,
                details,
            }) => {
                return Ok(Response::Error {
                    message,
                    kind,
                    code,
                    retryable,
                    details,
                })
            }
            Err(error) => return Err(error),
        };
        if matches!(
            request,
            Request::PreviewDraftAttachment { .. }
                | Request::BeginDraftAttachment { .. }
                | Request::WriteDraftAttachmentChunk { .. }
                | Request::FinalizeDraftAttachment { .. }
        ) {
            self.require_draft_attachment_upload().await?;
        }
        if matches!(
            request,
            Request::RuleCategories { .. } | Request::ExplainCategoryMembership { .. }
        ) {
            let status = self
                .request_response_unchecked(
                    if self.is_paired_remote() {
                        Request::GetRemoteReady
                    } else {
                        Request::GetStatus
                    },
                    &mut on_event,
                    request_timeout,
                )
                .await?;
            match status {
                Response::Ok { data: ResponseData::RemoteReady { .. } } => return Ok(Response::error_kinded("Category rules are unavailable on paired remote hosts", IpcErrorKind::Unsupported)),
                Response::Ok {data:ResponseData::Status {protocol_version,..}} if protocol_version >= mxr_protocol::IPC_PROTOCOL_VERSION => {},
                Response::Ok {data:ResponseData::Status {protocol_version,..}} => return Ok(Response::error_kinded(format!("Category rules require daemon IPC protocol {}; running daemon uses {protocol_version}. Update or restart the daemon before retrying.",mxr_protocol::IPC_PROTOCOL_VERSION),IpcErrorKind::Unsupported)),
                error @ Response::Error {..} => return Ok(error),
                _ => return Err(ClientError::Io(std::io::Error::other("Unexpected GetStatus response while checking category-rule support"))),
            }
        }
        if matches!(request, Request::SystemMailboxCounts { .. }) {
            // A daemon that predates the variant cannot decode it and closes
            // the connection, so ask its version first and answer
            // "unsupported" without sending. Callers fall back to label counts.
            let status = self
                .request_response_unchecked(
                    if self.is_paired_remote() {
                        Request::GetRemoteReady
                    } else {
                        Request::GetStatus
                    },
                    &mut on_event,
                    request_timeout,
                )
                .await?;
            match status {
                error @ Response::Error { .. } => return Ok(error),
                Response::Ok {
                    data:
                        ResponseData::Status {
                            protocol_version, ..
                        }
                        | ResponseData::RemoteReady {
                            protocol_version, ..
                        },
                } if protocol_version >= mxr_protocol::SYSTEM_MAILBOX_COUNTS_PROTOCOL => {}
                Response::Ok {
                    data:
                        ResponseData::Status {
                            protocol_version, ..
                        }
                        | ResponseData::RemoteReady {
                            protocol_version, ..
                        },
                } => {
                    return Ok(Response::error_kinded(
                        format!(
                            "System mailbox counts need daemon IPC protocol {}; running daemon uses {protocol_version}",
                            mxr_protocol::SYSTEM_MAILBOX_COUNTS_PROTOCOL
                        ),
                        IpcErrorKind::Unsupported,
                    ));
                }
                _ => {
                    return Err(ClientError::Io(std::io::Error::other(
                        "Unexpected GetStatus response while checking system mailbox count support",
                    )));
                }
            }
        }
        if matches!(
            request,
            Request::GetAttachmentTransferMetadata { .. } | Request::ReadAttachmentChunk { .. }
        ) && self.attachment_transfer_supported != Some(true)
        {
            let status = self
                .request_response_unchecked(
                    if self.is_paired_remote() {
                        Request::GetRemoteReady
                    } else {
                        Request::GetStatus
                    },
                    &mut on_event,
                    request_timeout,
                )
                .await?;
            match status {
                error @ Response::Error { .. } => return Ok(error),
                Response::Ok {
                    data:
                        ResponseData::Status {
                            attachment_transfer_supported: true,
                            ..
                        }
                        | ResponseData::RemoteReady {
                            attachment_transfer_supported: true,
                            ..
                        },
                } => self.attachment_transfer_supported = Some(true),
                Response::Ok {
                    data:
                        ResponseData::Status {
                            attachment_transfer_supported: false,
                            ..
                        }
                        | ResponseData::RemoteReady {
                            attachment_transfer_supported: false,
                            ..
                        },
                } => {
                    return Ok(Response::error_kinded(
                        "Daemon does not support path-free attachment transfers; upgrade the daemon before downloading",
                        IpcErrorKind::Unsupported,
                    ));
                }
                _ => {
                    return Err(ClientError::Io(std::io::Error::other(
                        "Unexpected GetStatus response while checking attachment transfer support",
                    )));
                }
            }
        }
        if matches!(
            request,
            Request::GetArchiveWorkspace
                | Request::PreviewArchive { .. }
                | Request::SubmitArchive { .. }
                | Request::GetArchiveOperation { .. }
        ) {
            let status = self
                .request_response_unchecked(
                    if self.is_paired_remote() {
                        Request::GetRemoteReady
                    } else {
                        Request::GetStatus
                    },
                    &mut on_event,
                    request_timeout,
                )
                .await?;
            match status {
                error @ Response::Error { .. } => return Ok(error),
                Response::Ok {
                    data: ResponseData::RemoteReady { .. },
                } => {
                    return Ok(Response::error_kinded(
                        "Recoverable Archive is unavailable on paired remote hosts",
                        IpcErrorKind::Unsupported,
                    ));
                }
                Response::Ok {
                    data:
                        ResponseData::Status {
                            archive_recovery_supported: true,
                            ..
                        },
                } => {}
                Response::Ok {
                    data:
                        ResponseData::Status {
                            archive_recovery_supported: false,
                            ..
                        },
                } => {
                    return Ok(Response::error_kinded(
                        "Daemon does not support recoverable Archive; upgrade before submitting",
                        IpcErrorKind::Unsupported,
                    ));
                }
                _ => {
                    return Err(ClientError::Io(std::io::Error::other(
                        "Unexpected GetStatus response while checking Archive support",
                    )));
                }
            }
        }
        if matches!(
            request,
            Request::SendDraft { .. }
                | Request::SendStoredDraft { .. }
                | Request::ScheduleSend { .. }
                | Request::GetSendOutcome { .. }
        ) {
            let status = self
                .request_response_unchecked(
                    if self.is_paired_remote() {
                        Request::GetRemoteReady
                    } else {
                        Request::GetStatus
                    },
                    &mut on_event,
                    request_timeout,
                )
                .await?;
            if !matches!(
                status,
                Response::Ok {
                    data: ResponseData::Status {
                        send_outcome_supported: true,
                        ..
                    } | ResponseData::RemoteReady {
                        send_outcome_supported: true,
                        ..
                    }
                }
            ) {
                return Ok(Response::error_kinded(
                    "Daemon does not support reviewed send outcomes; upgrade before sending",
                    mxr_protocol::IpcErrorKind::Unsupported,
                ));
            }
        }
        if matches!(
            request,
            Request::SaveDraft { .. }
                | Request::UpdateDraft { .. }
                | Request::DeleteDraft { .. }
                | Request::SaveDraftToServer { .. }
        ) {
            let status = self
                .request_response_unchecked(
                    if self.is_paired_remote() {
                        Request::GetRemoteReady
                    } else {
                        Request::GetStatus
                    },
                    &mut on_event,
                    request_timeout,
                )
                .await?;
            if !matches!(
                status,
                Response::Ok {
                    data: ResponseData::Status {
                        draft_revision_supported: true,
                        send_outcome_supported: true,
                        ..
                    } | ResponseData::RemoteReady {
                        draft_revision_supported: true,
                        send_outcome_supported: true,
                        ..
                    }
                }
            ) {
                return Ok(Response::error_kinded("Daemon does not support durable draft revisions; upgrade the daemon before saving or deleting drafts", mxr_protocol::IpcErrorKind::Unsupported));
            }
        }
        let response = self
            .request_response_unchecked_owned(
                request,
                on_event,
                request_timeout,
                expected_workspace,
            )
            .await?;
        self.verify_selection().await?;
        if safety_preview {
            if let Response::Ok {
                data: ResponseData::DraftPreview { report, .. },
            } = response
            {
                return Ok(Response::Ok {
                    data: ResponseData::DraftSafetyReportResponse { report },
                });
            }
        }
        Ok(response)
    }

    async fn request_response_unchecked<F>(
        &mut self,
        request: Request,
        on_event: F,
        request_timeout: Option<Duration>,
    ) -> Result<Response, ClientError>
    where
        F: FnMut(DaemonEvent),
    {
        self.request_response_unchecked_owned(request, on_event, request_timeout, None)
            .await
    }

    async fn request_response_unchecked_owned<F>(
        &mut self,
        request: Request,
        mut on_event: F,
        request_timeout: Option<Duration>,
        expected_workspace: Option<&mxr_protocol::ArchiveWorkspaceData>,
    ) -> Result<Response, ClientError>
    where
        F: FnMut(DaemonEvent),
    {
        let id = self.take_id();
        let mut message = self.envelope(id, request);
        message.expected_workspace = expected_workspace.cloned();
        self.framed.send(message).await?;

        let wait_for_response = async {
            loop {
                match self.framed.next().await {
                    Some(Ok(frame)) => match frame.payload {
                        IpcPayload::Response(response) if frame.id == id => return Ok(response),
                        IpcPayload::Event(event) => on_event(event),
                        IpcPayload::Response(_) => {
                            return Err(ClientError::UnexpectedFrame {
                                frame_id: frame.id,
                                expected_id: id,
                                is_response: true,
                            })
                        }
                        _ => {
                            return Err(ClientError::UnexpectedFrame {
                                frame_id: frame.id,
                                expected_id: id,
                                is_response: false,
                            })
                        }
                    },
                    Some(Err(error)) => return Err(ClientError::Io(error)),
                    None => return Err(ClientError::Closed),
                }
            }
        };

        match request_timeout {
            Some(duration) => match tokio::time::timeout(duration, wait_for_response).await {
                Ok(result) => result,
                Err(_) => Err(ClientError::Timeout(duration)),
            },
            None => wait_for_response.await,
        }
    }

    pub async fn capture_workspace(
        &mut self,
    ) -> Result<mxr_protocol::ArchiveWorkspaceData, ClientError> {
        self.capture_workspace_with_events(drop_event).await
    }

    pub async fn capture_workspace_with_events<F>(
        &mut self,
        mut on_event: F,
    ) -> Result<mxr_protocol::ArchiveWorkspaceData, ClientError>
    where
        F: FnMut(DaemonEvent),
    {
        self.verify_selection().await?;
        if let Some(expected) = &self.remote_workspace {
            let expected = expected.clone();
            match self
                .request_response_unchecked(
                    Request::GetRemoteReady,
                    &mut on_event,
                    self.default_timeout,
                )
                .await?
            {
                Response::Ok {
                    data:
                        ResponseData::RemoteReady {
                            workspace,
                            protocol_version,
                            ..
                        },
                } if workspace == expected
                    && protocol_version == mxr_protocol::IPC_PROTOCOL_VERSION =>
                {
                    return Ok(workspace)
                }
                _ => {
                    return Err(workspace_error(
                        "workspace_conflict",
                        "Remote workspace identity changed; reopen the original workspace",
                    ))
                }
            }
        }
        match self.request_response_unchecked(Request::GetStatus, &mut on_event, self.default_timeout).await? {
            Response::Ok { data: ResponseData::Status { workspace_owner_supported: true, .. } } => (),
            _ => return Err(workspace_error("workspace_owner_unsupported", "Daemon does not support workspace ownership; upgrade before replaying private work")),
        }
        match self
            .request_response_unchecked(
                Request::GetArchiveWorkspace,
                &mut on_event,
                self.default_timeout,
            )
            .await?
        {
            Response::Ok {
                data: ResponseData::ArchiveWorkspace { workspace },
            } => Ok(workspace),
            response => into_data(response).and_then(|_| {
                Err(workspace_error(
                    "workspace_owner_unavailable",
                    "Cannot obtain selected workspace identity",
                ))
            }),
        }
    }

    pub async fn verify_workspace(
        &mut self,
        expected: &mxr_protocol::ArchiveWorkspaceData,
    ) -> Result<(), ClientError> {
        self.verify_workspace_with_events(expected, drop_event)
            .await
    }

    async fn verify_workspace_with_events<F>(
        &mut self,
        expected: &mxr_protocol::ArchiveWorkspaceData,
        on_event: F,
    ) -> Result<(), ClientError>
    where
        F: FnMut(DaemonEvent),
    {
        if self.capture_workspace_with_events(on_event).await? != *expected {
            return Err(workspace_error("workspace_conflict", "Selected workspace or recovery generation changed; return to the original workspace to recover retained private work"));
        }
        Ok(())
    }

    pub async fn request_owned(
        &mut self,
        request: Request,
        workspace: &mxr_protocol::ArchiveWorkspaceData,
    ) -> Result<ResponseData, ClientError> {
        into_data(
            self.request_response_owned(request, Some(workspace), drop_event, self.default_timeout)
                .await?,
        )
    }

    /// Send `request` and return its [`ResponseData`] payload, mapping a
    /// `Response::Error` to [`ClientError::Daemon`]. Applies the connection's
    /// [default timeout](Self::with_default_timeout).
    pub async fn request(&mut self, request: Request) -> Result<ResponseData, ClientError> {
        let request_timeout = self.default_timeout;
        into_data(
            self.request_response(request, drop_event, request_timeout)
                .await?,
        )
    }

    /// Like [`Self::request`], but forwards interleaved [`DaemonEvent`] frames
    /// to `on_event` — the long-running-operation progress pattern (sync,
    /// reindex, rebuild). No timeout is applied.
    pub async fn request_with_events<F>(
        &mut self,
        request: Request,
        on_event: F,
    ) -> Result<ResponseData, ClientError>
    where
        F: FnMut(DaemonEvent),
    {
        into_data(self.request_response(request, on_event, None).await?)
    }

    pub(crate) async fn request_with_events_default_timeout<F>(
        &mut self,
        request: Request,
        on_event: F,
    ) -> Result<ResponseData, ClientError>
    where
        F: FnMut(DaemonEvent),
    {
        let request_timeout = self.default_timeout;
        into_data(
            self.request_response(request, on_event, request_timeout)
                .await?,
        )
    }

    /// Fire-and-forget: send `request` without awaiting a response.
    pub async fn notify(&mut self, request: Request) -> Result<(), ClientError> {
        self.verify_selection().await?;
        if matches!(
            request,
            Request::PreviewArchive { .. }
                | Request::SubmitArchive { .. }
                | Request::SaveDraft { .. }
                | Request::UpdateDraft { .. }
                | Request::DeleteDraft { .. }
                | Request::SaveDraftToServer { .. }
        ) {
            return self.request(request).await.map(|_| ());
        }
        let id = self.take_id();
        let message = self.envelope(id, request);
        self.framed.send(message).await?;
        Ok(())
    }

    /// Read the next frame from the connection (a [`Response`] or an
    /// unsolicited [`DaemonEvent`], surfaced as the raw [`IpcMessage`]).
    ///
    /// Cancel-safe: dropping the returned future leaves any partially-read
    /// frame buffered inside the connection, so it is safe to use as a
    /// `tokio::select!` branch.
    pub async fn next_event(&mut self) -> Result<IpcMessage, ClientError> {
        loop {
            self.verify_selection().await?;
            remote_connection::schedule_cleanup();
            tokio::select! {
                message = self.framed.next() => return match message {
                    Some(Ok(message)) => Ok(message), Some(Err(error)) => Err(ClientError::Io(error)), None => Err(ClientError::Closed),
                },
                _ = tokio::time::sleep(Duration::from_secs(1)), if self.selection_generation.is_some() => (),
            }
        }
    }
    async fn verify_selection(&mut self) -> Result<(), ClientError> {
        if let Some(generation) = self.selection_generation {
            let state = tokio::task::spawn_blocking(|| {
                mxr_config::connection_state::read(&mxr_config::connection_state::path())
            })
            .await
            .map_err(|_| ClientError::Closed)??;
            if state.generation != generation {
                let _ = tokio::time::timeout(Duration::from_secs(1), self.framed.close()).await;
                return Err(ClientError::Io(std::io::Error::other(
                    "Saved connection changed; close and reopen this client before continuing",
                )));
            }
        }
        Ok(())
    }
}

fn supervised_connection_error(path: &Path, reason: &str) -> ClientError {
    ClientError::Io(std::io::Error::other(format!(
        "{} ({}): {reason}",
        daemon_autostart_disabled_message(),
        path.display()
    )))
}

fn drop_event(_event: DaemonEvent) {}

fn into_data(response: Response) -> Result<ResponseData, ClientError> {
    match response {
        Response::Ok { data } => Ok(data),
        Response::Error {
            message,
            kind,
            retryable,
            code,
            details,
        } => Err(ClientError::Daemon {
            message,
            kind,
            retryable,
            code,
            details,
        }),
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
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
    use tokio::net::UnixListener;

    /// A minimal daemon stand-in that speaks the real [`IpcCodec`] over a Unix
    /// socket. `responder` maps a decoded [`Request`] to an optional
    /// [`Response`] (return `None` to stay silent — e.g. to exercise the
    /// timeout path); `preamble` frames are pushed to every accepted
    /// connection before any request is read (used for event-stream tests).
    async fn spawn_fake_daemon<F>(
        socket_path: &Path,
        preamble: Vec<IpcMessage>,
        responder: F,
    ) -> tokio::task::JoinHandle<()>
    where
        F: Fn(&IpcMessage) -> Option<Response> + Send + Sync + 'static,
    {
        let responder = Arc::new(responder);
        let listener = UnixListener::bind(socket_path).unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let responder = responder.clone();
                let preamble = preamble.clone();
                tokio::spawn(async move {
                    let mut framed = Framed::new(stream, IpcCodec::new());
                    for frame in preamble {
                        if framed.send(frame).await.is_err() {
                            return;
                        }
                    }
                    while let Some(Ok(message)) = framed.next().await {
                        if let Some(response) = responder(&message) {
                            let reply = IpcMessage {
                                expected_workspace: None,
                                id: message.id,
                                source: ClientKind::Daemon,
                                payload: IpcPayload::Response(response),
                            };
                            if framed.send(reply).await.is_err() {
                                return;
                            }
                        }
                    }
                });
            }
        })
    }

    fn status_response(
        protocol_version: u32,
        daemon_version: Option<&str>,
        daemon_build_id: Option<String>,
    ) -> Response {
        Response::Ok {
            data: serde_json::from_value(serde_json::json!({
                "kind": "Status",
                "uptime_secs": 0,
                "accounts": [],
                "total_messages": 0,
                "protocol_version": protocol_version,
                "daemon_version": daemon_version,
                "daemon_build_id": daemon_build_id,
            }))
            .expect("status fixture should deserialize"),
        }
    }

    fn status_identity() -> Response {
        status_response(
            mxr_protocol::IPC_PROTOCOL_VERSION,
            Some(env!("CARGO_PKG_VERSION")),
            Some(current_daemon_build_id()),
        )
    }

    fn event_frame(event: DaemonEvent) -> IpcMessage {
        IpcMessage {
            expected_workspace: None,
            id: 0,
            source: ClientKind::Daemon,
            payload: IpcPayload::Event(event),
        }
    }

    fn sync_completed() -> DaemonEvent {
        DaemonEvent::SyncCompleted {
            account_id: mxr_core::id::AccountId::new(),
            messages_synced: 3,
        }
    }

    #[tokio::test]
    async fn request_correlates_response_by_id() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("mxr.sock");
        let _server = spawn_fake_daemon(&sock, Vec::new(), |message| match &message.payload {
            IpcPayload::Request(Request::Ping) => Some(Response::Ok {
                data: ResponseData::Pong,
            }),
            _ => None,
        })
        .await;

        let mut conn = IpcConnection::connect(&sock, ClientKind::Cli)
            .await
            .unwrap();
        // Two sequential requests: each must receive its own correlated reply.
        assert!(matches!(
            conn.request(Request::Ping).await.unwrap(),
            ResponseData::Pong
        ));
        assert!(matches!(
            conn.request(Request::Ping).await.unwrap(),
            ResponseData::Pong
        ));
    }

    /// A daemon older than the request would fail to decode it and drop the
    /// connection, so the client must not send it: it answers "unsupported"
    /// itself and the connection stays usable.
    #[tokio::test]
    async fn system_mailbox_counts_are_not_sent_to_a_daemon_that_predates_them() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("mxr.sock");
        let delivered = Arc::new(std::sync::Mutex::new(0usize));
        let delivered_writer = delivered.clone();
        let _server = spawn_fake_daemon(&sock, Vec::new(), move |message| match &message.payload {
            IpcPayload::Request(Request::GetStatus) => Some(status_response(
                mxr_protocol::SYSTEM_MAILBOX_COUNTS_PROTOCOL - 1,
                Some("old"),
                None,
            )),
            IpcPayload::Request(Request::SystemMailboxCounts { .. }) => {
                *delivered_writer.lock().unwrap() += 1;
                None
            }
            IpcPayload::Request(Request::Ping) => Some(Response::Ok {
                data: ResponseData::Pong,
            }),
            _ => None,
        })
        .await;

        let mut conn = IpcConnection::connect(&sock, ClientKind::Tui)
            .await
            .unwrap();
        let response = conn
            .request_response(
                Request::SystemMailboxCounts { account_id: None },
                drop_event,
                None,
            )
            .await
            .unwrap();
        assert!(
            matches!(
                response,
                Response::Error {
                    kind: IpcErrorKind::Unsupported,
                    ..
                }
            ),
            "{response:?}"
        );
        assert_eq!(*delivered.lock().unwrap(), 0, "the request was sent");
        // The connection is still good for the next call.
        assert!(matches!(
            conn.request(Request::Ping).await.unwrap(),
            ResponseData::Pong
        ));
    }

    #[tokio::test]
    async fn system_mailbox_counts_are_sent_to_a_daemon_that_supports_them() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("mxr.sock");
        let _server = spawn_fake_daemon(&sock, Vec::new(), |message| match &message.payload {
            IpcPayload::Request(Request::GetStatus) => Some(status_identity()),
            IpcPayload::Request(Request::SystemMailboxCounts { .. }) => Some(Response::Ok {
                data: ResponseData::SystemMailboxCounts {
                    mailboxes: Vec::new(),
                },
            }),
            _ => None,
        })
        .await;
        let mut conn = IpcConnection::connect(&sock, ClientKind::Tui)
            .await
            .unwrap();
        assert!(matches!(
            conn.request(Request::SystemMailboxCounts { account_id: None })
                .await
                .unwrap(),
            ResponseData::SystemMailboxCounts { .. }
        ));
    }

    #[tokio::test]
    async fn tags_requests_with_the_configured_client_kind() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("mxr.sock");
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen_writer = seen.clone();
        let _server = spawn_fake_daemon(&sock, Vec::new(), move |message| {
            seen_writer.lock().unwrap().push(message.source);
            Some(Response::Ok {
                data: ResponseData::Pong,
            })
        })
        .await;

        let mut conn = IpcConnection::connect(&sock, ClientKind::Web)
            .await
            .unwrap();
        conn.request(Request::Ping).await.unwrap();
        assert_eq!(seen.lock().unwrap().as_slice(), &[ClientKind::Web]);
    }

    #[tokio::test]
    async fn supervised_connection_checks_identity_before_normal_requests() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("mxr.sock");
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen_writer = seen.clone();
        let _server = spawn_fake_daemon(&sock, Vec::new(), move |message| match &message.payload {
            IpcPayload::Request(Request::GetStatus) => {
                seen_writer.lock().unwrap().push("status");
                Some(status_identity())
            }
            IpcPayload::Request(Request::Ping) => {
                seen_writer.lock().unwrap().push("ping");
                Some(Response::Ok {
                    data: ResponseData::Pong,
                })
            }
            _ => None,
        })
        .await;

        let mut connection = IpcConnection::connect_supervised(&sock, ClientKind::Tui)
            .await
            .unwrap();
        assert_eq!(seen.lock().unwrap().as_slice(), &["status"]);
        assert!(matches!(
            connection.request(Request::Ping).await.unwrap(),
            ResponseData::Pong
        ));
        assert_eq!(seen.lock().unwrap().as_slice(), &["status", "ping"]);
    }

    #[tokio::test]
    async fn supervised_connection_rejects_mismatched_protocol_version_or_build() {
        let mismatches = [
            (
                mxr_protocol::IPC_PROTOCOL_VERSION - 1,
                Some(env!("CARGO_PKG_VERSION")),
                Some(current_daemon_build_id()),
            ),
            (
                mxr_protocol::IPC_PROTOCOL_VERSION + 1,
                Some(env!("CARGO_PKG_VERSION")),
                Some(current_daemon_build_id()),
            ),
            (
                mxr_protocol::IPC_PROTOCOL_VERSION,
                Some("0.0.0"),
                Some(current_daemon_build_id()),
            ),
            (
                mxr_protocol::IPC_PROTOCOL_VERSION,
                Some(env!("CARGO_PKG_VERSION")),
                Some("other-build".to_string()),
            ),
        ];

        for (index, (protocol, version, build_id)) in mismatches.into_iter().enumerate() {
            let dir = tempfile::tempdir().unwrap();
            let sock = dir.path().join(format!("mxr-{index}.sock"));
            let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
            let seen_writer = seen.clone();
            let _server =
                spawn_fake_daemon(&sock, Vec::new(), move |message| match &message.payload {
                    IpcPayload::Request(Request::GetStatus) => {
                        seen_writer.lock().unwrap().push("status");
                        Some(status_response(protocol, version, build_id.clone()))
                    }
                    IpcPayload::Request(Request::Ping) => {
                        seen_writer.lock().unwrap().push("ping");
                        Some(Response::Ok {
                            data: ResponseData::Pong,
                        })
                    }
                    _ => None,
                })
                .await;

            let error = match IpcConnection::connect_supervised(&sock, ClientKind::Tui).await {
                Ok(_) => panic!("mismatched supervised daemon identity was accepted"),
                Err(error) => error,
            };
            assert!(error.to_string().contains("incompatible"), "{error}");
            assert_eq!(seen.lock().unwrap().as_slice(), &["status"]);
        }
    }

    #[tokio::test]
    async fn supervised_status_failure_has_service_manager_remediation() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("mxr.sock");
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen_writer = seen.clone();
        let _server = spawn_fake_daemon(&sock, Vec::new(), move |_message| {
            seen_writer.lock().unwrap().push("status");
            Some(Response::error("status unavailable"))
        })
        .await;

        let error = match IpcConnection::connect_supervised(&sock, ClientKind::Tui).await {
            Ok(_) => panic!("status failure should reject supervised connection"),
            Err(error) => error,
        };
        let message = error.to_string();
        assert!(message.contains("service manager"), "{message}");
        assert!(message.contains("status request failed"), "{message}");
        assert!(message.contains("status unavailable"), "{message}");
        assert_eq!(seen.lock().unwrap().as_slice(), &["status"]);
    }

    #[tokio::test]
    async fn every_supervised_reconnect_checks_identity_before_requests() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("mxr.sock");
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen_writer = seen.clone();
        let status_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let status_count_writer = status_count.clone();
        let _server = spawn_fake_daemon(&sock, Vec::new(), move |message| match &message.payload {
            IpcPayload::Request(Request::GetStatus) => {
                seen_writer.lock().unwrap().push("status");
                if status_count_writer.fetch_add(1, Ordering::SeqCst) == 0 {
                    Some(status_identity())
                } else {
                    Some(status_response(
                        mxr_protocol::IPC_PROTOCOL_VERSION + 1,
                        Some(env!("CARGO_PKG_VERSION")),
                        Some(current_daemon_build_id()),
                    ))
                }
            }
            IpcPayload::Request(Request::Ping) => {
                seen_writer.lock().unwrap().push("ping");
                Some(Response::Ok {
                    data: ResponseData::Pong,
                })
            }
            _ => None,
        })
        .await;

        let mut connection = IpcConnection::connect_supervised(&sock, ClientKind::Tui)
            .await
            .unwrap();
        connection.request(Request::Ping).await.unwrap();
        let error = match IpcConnection::connect_supervised(&sock, ClientKind::Tui).await {
            Ok(_) => panic!("replacement daemon with a different identity was accepted"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("incompatible"), "{error}");
        assert_eq!(
            seen.lock().unwrap().as_slice(),
            &["status", "ping", "status"]
        );
    }

    #[tokio::test]
    async fn with_start_id_puts_exact_id_on_the_wire() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("mxr.sock");
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen_writer = seen.clone();
        let _server = spawn_fake_daemon(&sock, Vec::new(), move |message| {
            seen_writer.lock().unwrap().push(message.id);
            Some(Response::Ok {
                data: ResponseData::Pong,
            })
        })
        .await;

        // The web bridge's correlation seam: `with_start_id(N)` must put exactly
        // `N` on the wire so daemon logs line up with the bridge's request id.
        let mut conn = IpcConnection::connect(&sock, ClientKind::Web)
            .await
            .unwrap()
            .with_start_id(4242);
        conn.request(Request::Ping).await.unwrap();
        assert_eq!(seen.lock().unwrap().as_slice(), &[4242]);
    }

    #[tokio::test]
    async fn unexpected_response_id_is_a_typed_error() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("mxr.sock");
        // Reply with a mismatched correlation id (never happens against the real
        // daemon, but pins the typed fail-fast disposition).
        let listener = UnixListener::bind(&sock).unwrap();
        let server = tokio::spawn(async move {
            if let Ok((stream, _)) = listener.accept().await {
                let mut framed = Framed::new(stream, IpcCodec::new());
                if let Some(Ok(message)) = framed.next().await {
                    let _ = framed
                        .send(IpcMessage {
                            expected_workspace: None,
                            id: message.id.wrapping_add(100),
                            source: ClientKind::Daemon,
                            payload: IpcPayload::Response(Response::Ok {
                                data: ResponseData::Pong,
                            }),
                        })
                        .await;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });

        let mut conn = IpcConnection::connect(&sock, ClientKind::Cli)
            .await
            .unwrap();
        match conn.request(Request::Ping).await {
            Err(ClientError::UnexpectedFrame {
                is_response,
                expected_id,
                frame_id,
            }) => {
                assert!(is_response);
                assert_eq!(expected_id, 1);
                assert_eq!(frame_id, 101);
            }
            other => panic!("expected UnexpectedFrame, got {other:?}"),
        }
        server.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn request_times_out_when_daemon_never_replies() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("mxr.sock");
        // Accept and read forever, never replying.
        let _server = spawn_fake_daemon(&sock, Vec::new(), |_| None).await;

        let mut conn = IpcConnection::connect(&sock, ClientKind::Cli)
            .await
            .unwrap()
            .with_default_timeout(Some(Duration::from_secs(120)));
        let error = conn.request(Request::Ping).await.unwrap_err();
        assert!(
            matches!(error, ClientError::Timeout(d) if d == Duration::from_secs(120)),
            "unexpected error: {error:?}"
        );
    }

    #[tokio::test]
    async fn daemon_error_maps_to_kinded_client_error() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("mxr.sock");
        let _server = spawn_fake_daemon(&sock, Vec::new(), |_| {
            Some(Response::error_kinded(
                "mailbox not found",
                IpcErrorKind::NotFound,
            ))
        })
        .await;

        let mut conn = IpcConnection::connect(&sock, ClientKind::Cli)
            .await
            .unwrap();
        let error = conn.request(Request::Ping).await.unwrap_err();
        match error {
            ClientError::Daemon {
                message,
                kind,
                retryable,
                ..
            } => {
                assert_eq!(message, "mailbox not found");
                assert_eq!(kind, IpcErrorKind::NotFound);
                assert!(!retryable);
            }
            other => panic!("expected Daemon error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn request_response_returns_error_variant_verbatim() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("mxr.sock");
        let _server = spawn_fake_daemon(&sock, Vec::new(), |_| {
            Some(Response::error_kinded("boom", IpcErrorKind::Provider))
        })
        .await;

        let mut conn = IpcConnection::connect(&sock, ClientKind::Mcp)
            .await
            .unwrap();
        // The raw primitive does NOT collapse Response::Error into ClientError.
        let response = conn
            .request_response(Request::Ping, drop_event, None)
            .await
            .unwrap();
        match response {
            Response::Error {
                message,
                kind,
                retryable,
                code,
                ..
            } => {
                assert_eq!(message, "boom");
                assert_eq!(kind, IpcErrorKind::Provider);
                assert!(retryable, "Provider errors are retryable");
                assert_eq!(code, "provider");
            }
            other => panic!("expected verbatim Error response, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn remote_preflights_return_unsupported_without_dispatching_the_operation() {
        for request in [
            Request::ExplainCategoryMembership {
                message_id: mxr_core::MessageId::new(),
                category_id: mxr_core::CategoryId::new(),
            },
            Request::GetArchiveOperation {
                identity: mxr_protocol::ArchiveIdentityData {
                    operation_id: "fixture-operation".into(),
                    caller_id: "fixture-caller".into(),
                    workspace: mxr_protocol::ArchiveWorkspaceData {
                        workspace_id: "fixture-workspace".into(),
                        recovery_generation: "fixture-generation".into(),
                    },
                },
            },
            Request::GetAttachmentTransferMetadata {
                message_id: mxr_core::MessageId::new(),
                attachment_id: mxr_core::AttachmentId::new(),
            },
        ] {
            let dir = tempfile::tempdir().unwrap();
            let sock = dir.path().join("mxr.sock");
            let calls = Arc::new(AtomicU64::new(0));
            let server_calls = calls.clone();
            let server = spawn_fake_daemon(&sock, Vec::new(), move |message| {
                assert!(matches!(
                    message.payload,
                    IpcPayload::Request(Request::GetRemoteReady)
                ));
                server_calls.fetch_add(1, Ordering::SeqCst);
                Some(Response::Ok {
                    data: ResponseData::RemoteReady {
                        attachment_transfer_supported: false,
                        capabilities: vec![mxr_core::RemoteCapability::MailRead],
                        draft_attachment_upload_supported: false,
                        draft_revision_supported: false,
                        send_outcome_supported: false,
                        protocol_version: mxr_protocol::IPC_PROTOCOL_VERSION,
                        host_id: "fixture-host".into(),
                        client_id: "fixture-client".into(),
                        workspace: mxr_protocol::ArchiveWorkspaceData {
                            workspace_id: "fixture-workspace".into(),
                            recovery_generation: "fixture-generation".into(),
                        },
                    },
                })
            })
            .await;
            let mut connection = IpcConnection::connect(&sock, ClientKind::Cli)
                .await
                .unwrap();
            connection.selected_remote = true;
            connection.remote_registration_identity =
                Some(("fixture-host".into(), "fixture-client".into()));
            let response = connection
                .request_response(request, |_| {}, None)
                .await
                .unwrap();
            assert!(matches!(
                response,
                Response::Error {
                    kind: IpcErrorKind::Unsupported,
                    ..
                }
            ));
            assert_eq!(calls.load(Ordering::SeqCst), 1);
            server.abort();
        }
    }

    #[tokio::test]
    async fn archive_capability_check_preserves_get_status_daemon_error() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("mxr.sock");
        let expected = Response::Error {
            message: "status lookup failed".into(),
            kind: IpcErrorKind::Provider,
            code: "status_unavailable".into(),
            retryable: true,
            details: Some(serde_json::json!({"request":"GetStatus"})),
        };
        let server_response = expected.clone();
        let _server = spawn_fake_daemon(&sock, Vec::new(), move |message| {
            matches!(message.payload, IpcPayload::Request(Request::GetStatus))
                .then(|| server_response.clone())
        })
        .await;

        let mut connection = IpcConnection::connect(&sock, ClientKind::Cli)
            .await
            .unwrap();
        let response = connection
            .request_response(Request::GetArchiveWorkspace, |_| {}, None)
            .await
            .unwrap();
        match (response, expected) {
            (
                Response::Error {
                    message: actual_message,
                    kind: actual_kind,
                    code: actual_code,
                    retryable: actual_retryable,
                    details: actual_details,
                },
                Response::Error {
                    message: expected_message,
                    kind: expected_kind,
                    code: expected_code,
                    retryable: expected_retryable,
                    details: expected_details,
                },
            ) => {
                assert_eq!(actual_message, expected_message);
                assert_eq!(actual_kind, expected_kind);
                assert_eq!(actual_code, expected_code);
                assert_eq!(actual_retryable, expected_retryable);
                assert_eq!(actual_details, expected_details);
            }
            other => panic!("GetStatus error was changed: {other:?}"),
        }
    }

    #[tokio::test]
    async fn events_interleaved_before_response_are_forwarded_then_response_returned() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("mxr.sock");
        // Push two events, then reply to the request.
        let _server = spawn_fake_daemon(
            &sock,
            vec![event_frame(sync_completed()), event_frame(sync_completed())],
            |message| match &message.payload {
                IpcPayload::Request(Request::Ping) => Some(Response::Ok {
                    data: ResponseData::Pong,
                }),
                _ => None,
            },
        )
        .await;

        let mut conn = IpcConnection::connect(&sock, ClientKind::Cli)
            .await
            .unwrap();
        let count = Arc::new(AtomicU64::new(0));
        let sink = count.clone();
        let data = conn
            .request_with_events(Request::Ping, move |_event| {
                sink.fetch_add(1, Ordering::Relaxed);
            })
            .await
            .unwrap();
        assert!(matches!(data, ResponseData::Pong));
        assert_eq!(count.load(Ordering::Relaxed), 2, "both events forwarded");
    }

    #[tokio::test]
    async fn notify_sends_without_awaiting_a_response() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("mxr.sock");
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let writer = seen.clone();
        let _server = spawn_fake_daemon(&sock, Vec::new(), move |message| {
            if let IpcPayload::Request(request) = &message.payload {
                writer.lock().unwrap().push(request.clone());
            }
            // Deliberately never reply — notify must not block on a response.
            None
        })
        .await;

        let mut conn = IpcConnection::connect(&sock, ClientKind::Cli)
            .await
            .unwrap();
        conn.notify(Request::Ping).await.unwrap();

        // The server observes the request even though no response was sent.
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if !seen.lock().unwrap().is_empty() {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("server received the notify request");
    }

    #[tokio::test]
    async fn next_event_yields_unsolicited_event_frames() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("mxr.sock");
        let _server = spawn_fake_daemon(&sock, vec![event_frame(sync_completed())], |_| None).await;

        let mut conn = IpcConnection::connect(&sock, ClientKind::Tui)
            .await
            .unwrap();
        let message = conn.next_event().await.unwrap();
        assert!(
            matches!(
                message.payload,
                IpcPayload::Event(DaemonEvent::SyncCompleted { .. })
            ),
            "expected a SyncCompleted event frame"
        );
    }

    #[tokio::test]
    async fn next_event_reports_closed_connection() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("mxr.sock");
        // Accept, then immediately drop the connection (no preamble, no reply).
        let listener = UnixListener::bind(&sock).unwrap();
        let server = tokio::spawn(async move {
            if let Ok((stream, _)) = listener.accept().await {
                drop(stream);
            }
        });

        let mut conn = IpcConnection::connect(&sock, ClientKind::Tui)
            .await
            .unwrap();
        let error = conn.next_event().await.unwrap_err();
        assert!(
            matches!(error, ClientError::Closed),
            "unexpected error: {error:?}"
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn connect_failure_surfaces_connect_error_with_io_kind() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("does-not-exist.sock");
        match IpcConnection::connect(&missing, ClientKind::Cli).await {
            Err(ClientError::Connect { source, .. }) => {
                assert_eq!(source.kind(), std::io::ErrorKind::NotFound);
            }
            Err(other) => panic!("expected Connect error, got {other:?}"),
            Ok(_) => panic!("expected connect to a missing socket to fail"),
        }
    }
    #[tokio::test]
    async fn category_rules_refuse_old_daemon_on_same_connection_without_sending_variant() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("old.sock");
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let writer = seen.clone();
        let _server = spawn_fake_daemon(&socket, Vec::new(), move |message| {
            writer
                .lock()
                .unwrap()
                .push(serde_json::to_value(&message.payload).unwrap());
            Some(status_response(
                mxr_protocol::IPC_PROTOCOL_VERSION - 1,
                None,
                None,
            ))
        })
        .await;
        let mut connection = IpcConnection::connect(&socket, ClientKind::Cli)
            .await
            .unwrap();
        let response = connection
            .request_response(
                Request::ExplainCategoryMembership {
                    message_id: mxr_core::MessageId::new(),
                    category_id: mxr_core::CategoryId::new(),
                },
                |_| {},
                None,
            )
            .await
            .unwrap();
        assert!(matches!(
            response,
            Response::Error {
                kind: IpcErrorKind::Unsupported,
                ..
            }
        ));
        assert_eq!(seen.lock().unwrap().len(), 1);
        assert!(seen.lock().unwrap()[0].to_string().contains("GetStatus"));
    }
}

#[cfg(test)]
mod draft_revision_compatibility_tests {
    use super::*;
    use futures::{SinkExt, StreamExt};
    use mxr_core::AccountId;
    use tokio::net::UnixListener;

    #[tokio::test]
    async fn legacy_daemon_is_readable_but_receives_no_unconditional_delete() {
        let dir = tempfile::tempdir().expect("compatibility test fixture");
        let sock = dir.path().join("legacy.sock");
        let listener = UnixListener::bind(&sock).expect("compatibility test fixture");
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("compatibility test fixture");
            let mut framed = Framed::new(stream, IpcCodec::new());
            let first = framed
                .next()
                .await
                .expect("compatibility test fixture")
                .expect("compatibility test fixture");
            assert!(matches!(first.payload, IpcPayload::Request(Request::Ping)));
            framed
                .send(IpcMessage {
                    expected_workspace: None,
                    id: first.id,
                    source: ClientKind::Daemon,
                    payload: IpcPayload::Response(Response::Ok {
                        data: ResponseData::Pong,
                    }),
                })
                .await
                .expect("compatibility test fixture");
            let negotiation = framed
                .next()
                .await
                .expect("compatibility test fixture")
                .expect("compatibility test fixture");
            assert!(matches!(
                negotiation.payload,
                IpcPayload::Request(Request::GetStatus)
            ));
            // Missing capability evidence (representative of an older server).
            framed
                .send(IpcMessage {
                    expected_workspace: None,
                    id: negotiation.id,
                    source: ClientKind::Daemon,
                    payload: IpcPayload::Response(Response::Ok {
                        data: ResponseData::Accounts { accounts: vec![] },
                    }),
                })
                .await
                .expect("compatibility test fixture");
            assert!(
                tokio::time::timeout(Duration::from_millis(30), framed.next())
                    .await
                    .is_err()
            );
        });
        let mut connection = IpcConnection::connect(&sock, ClientKind::Cli)
            .await
            .expect("compatibility test fixture");
        assert!(matches!(
            connection
                .request(Request::Ping)
                .await
                .expect("compatibility test fixture"),
            ResponseData::Pong
        ));
        let response = connection
            .capture_workspace()
            .await
            .expect_err("legacy has no owner capability");
        assert!(matches!(
            response,
            ClientError::Daemon { ref code, .. } if code == "workspace_owner_unsupported"
        ));
        server.await.expect("compatibility test fixture");
        let _ = AccountId::new();
    }
    #[tokio::test]
    async fn legacy_daemon_receives_no_reviewed_send_without_outcome_capability() {
        let dir = tempfile::tempdir().expect("send compatibility fixture");
        let sock = dir.path().join("legacy-send.sock");
        let listener = UnixListener::bind(&sock).expect("send compatibility fixture");
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("send compatibility fixture");
            let mut framed = Framed::new(stream, IpcCodec::new());
            let request = framed
                .next()
                .await
                .expect("send compatibility fixture")
                .expect("send compatibility fixture");
            assert!(matches!(
                request.payload,
                IpcPayload::Request(Request::GetStatus)
            ));
            framed
                .send(IpcMessage {
                    expected_workspace: None,
                    id: request.id,
                    source: ClientKind::Daemon,
                    payload: IpcPayload::Response(Response::Ok {
                        data: ResponseData::Ack,
                    }),
                })
                .await
                .expect("send compatibility fixture");
            assert!(
                tokio::time::timeout(Duration::from_millis(100), framed.next())
                    .await
                    .is_err()
            );
        });
        let mut connection = IpcConnection::connect(&sock, ClientKind::Cli)
            .await
            .expect("send compatibility fixture");
        let response = connection
            .capture_workspace()
            .await
            .expect_err("legacy has no owner capability");
        assert!(matches!(
            response,
            ClientError::Daemon { ref code, .. } if code == "workspace_owner_unsupported"
        ));
        server.await.expect("send compatibility fixture");
    }
}

fn workspace_error(code: &str, message: &str) -> ClientError {
    ClientError::Daemon {
        message: message.into(),
        kind: mxr_protocol::IpcErrorKind::InvalidRequest,
        code: code.into(),
        retryable: false,
        details: None,
    }
}

#[cfg(test)]
mod workspace_owner_tests;
