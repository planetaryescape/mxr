use mxr_client::{ClientError, IpcConnection};
use mxr_core::id::*;
use mxr_core::types::*;
use mxr_core::MxrError;
use mxr_protocol::*;
use std::path::Path;
use tokio::sync::mpsc;
use tracing::warn;

#[derive(Clone)]
pub(crate) struct DaemonSession {
    pub(crate) target: mxr_client::SelectedDaemonTarget,
    pub(crate) owner: Option<mxr_protocol::ArchiveWorkspaceData>,
}

pub struct Client {
    conn: IpcConnection,
    event_tx: Option<mpsc::UnboundedSender<DaemonEvent>>,
    // How to reconnect, for requests that may leave the connection unusable.
    target: mxr_client::SelectedDaemonTarget,
}

/// `label_role` that lists every enabled account's mail.
pub(crate) fn all_mail_role() -> Option<String> {
    Role::AllMail.system_mailbox_wire_name().map(str::to_string)
}

/// `label_role` for listing `label` across every account, for a System
/// label; `None` for user labels and folders, which stay per label id.
pub(crate) fn unified_role_for(labels: &[Label], label_id: &LabelId) -> Option<String> {
    labels
        .iter()
        .find(|label| &label.id == label_id)?
        .system_mailbox_role()
        .and_then(Role::system_mailbox_wire_name)
        .map(str::to_string)
}

/// Fold every account's system mailboxes into the default account's labels.
///
/// Each system label's unread/total becomes the mailbox's total across
/// accounts, from the daemon's `counts` or, for a daemon without
/// `SystemMailboxCounts`, the sum of the accounts' labels. A mailbox only
/// another account has gets an entry too (its label as that account has it),
/// so Sent on a second account is reachable when the default account has no
/// Sent label. User labels are left alone.
pub(crate) fn with_unified_system_counts(
    mut labels: Vec<Label>,
    all: &[Label],
    counts: &[SystemMailboxCount],
) -> Vec<Label> {
    let mut sums: std::collections::HashMap<Role, (u32, u32)> = std::collections::HashMap::new();
    for count in counts {
        if let Some(role) = Role::from_system_mailbox_wire_name(&count.role) {
            sums.insert(role, (count.unread, count.total));
        }
    }
    if sums.is_empty() {
        for label in all {
            if let Some(role) = label.system_mailbox_role() {
                let entry = sums.entry(role).or_default();
                entry.0 += label.unread_count;
                entry.1 += label.total_count;
            }
        }
    }
    let mut have: std::collections::HashSet<Role> = labels
        .iter()
        .filter_map(Label::system_mailbox_role)
        .collect();
    for label in all {
        if let Some(role) = label.system_mailbox_role() {
            if have.insert(role) {
                labels.push(label.clone());
            }
        }
    }
    for label in &mut labels {
        if let Some((unread, total)) = label
            .system_mailbox_role()
            .and_then(|role| sums.get(&role).copied())
        {
            label.unread_count = unread;
            label.total_count = total;
        }
    }
    labels
}

impl Client {
    pub(crate) fn replace_connection(&mut self, mut replacement: Self) -> Result<(), MxrError> {
        replacement
            .conn
            .retain_draft_attachment_previews_from(&mut self.conn)
            .map_err(|error| MxrError::Ipc(error.to_string()))?;
        *self = replacement;
        Ok(())
    }

    pub async fn transfer_attachment_to(
        &mut self,
        message_id: &MessageId,
        attachment_id: &AttachmentId,
        destination: &Path,
        replace_existing: bool,
    ) -> Result<AttachmentFile, MxrError> {
        let downloaded = mxr_client::download_attachment_to(
            &mut self.conn,
            message_id,
            attachment_id,
            destination,
            replace_existing,
        )
        .await
        .map_err(|error| MxrError::Ipc(error.to_string()))?;
        Ok(AttachmentFile {
            attachment_id: attachment_id.clone(),
            filename: downloaded.filename,
            path: downloaded.path.display().to_string(),
        })
    }

    pub async fn connect(socket_path: &Path) -> std::io::Result<Self> {
        Self::connect_target(&socket_path.to_path_buf().into()).await
    }

    pub(crate) async fn connect_target(
        target: &mxr_client::SelectedDaemonTarget,
    ) -> std::io::Result<Self> {
        let connection = target.connect(ClientKind::Tui).await;
        let conn = connection.map_err(|error| match error {
            ClientError::Connect { source, .. } => source,
            other => std::io::Error::other(other.to_string()),
        })?;
        Ok(Self {
            conn,
            event_tx: None,
            target: target.clone(),
        })
    }

    pub fn with_event_channel(mut self, tx: mpsc::UnboundedSender<DaemonEvent>) -> Self {
        self.event_tx = Some(tx);
        self
    }

    pub async fn raw_request(&mut self, req: Request) -> Result<Response, MxrError> {
        if let Request::Mutation {
            mutation: MutationCommand::Archive { message_ids },
            ..
        } = req
        {
            let journal = mxr_client::ArchiveJournal::new(&mxr_config::data_dir())
                .map_err(|e| MxrError::Ipc(e.to_string()))?;
            let event_tx = self.event_tx.clone();
            let mut forward_event = move |event| {
                if let Some(tx) = &event_tx {
                    let _ = tx.send(event);
                }
            };
            let operation = self
                .conn
                .preview_archive_with_events(
                    &journal,
                    MutationCommand::Archive { message_ids },
                    &mut forward_event,
                )
                .await
                .map_err(map_request_error)?;
            let id = operation.identity.operation_id.clone();
            self.conn
                .submit_archive_with_events(&journal, &operation, &mut forward_event)
                .await
                .map_err(|error| MxrError::Ipc(format!("{error}; Archive operation {id} retained. Use Archive recovery or mxr archive-operation {id}")))?;
            let outcome = self
                .conn
                .wait_archive_with_events(operation.identity, &mut forward_event)
                .await
                .map_err(|error| MxrError::Ipc(format!("{error}; Archive operation {id} retained. Use Archive recovery or mxr archive-operation {id}")))?;
            let succeeded = outcome
                .items
                .iter()
                .filter(|item| item.state == ArchiveItemStateData::Completed)
                .count() as u32;
            if succeeded != outcome.items.len() as u32 {
                return Ok(Response::Error {
                    message: format!(
                        "Archive operation {id} requires review: {}",
                        outcome
                            .items
                            .iter()
                            .filter_map(|item| item.reason.as_deref())
                            .collect::<Vec<_>>()
                            .join("; ")
                    ),
                    kind: IpcErrorKind::Provider,
                    code: "archive_incomplete".into(),
                    retryable: false,
                    details: Some(
                        serde_json::to_value(outcome).map_err(|e| MxrError::Ipc(e.to_string()))?,
                    ),
                });
            }
            return Ok(Response::Ok {
                data: ResponseData::MutationResult {
                    result: MutationResultData {
                        requested: succeeded,
                        succeeded,
                        skipped: 0,
                        failed: 0,
                        accounts: vec![],
                        mutation_id: outcome.undo_available.then_some(outcome.mutation_id),
                        undo_unavailable: !outcome.undo_available,
                    },
                },
            });
        }
        self.request(req).await
    }

    pub(crate) async fn capture_workspace(
        &mut self,
    ) -> Result<mxr_protocol::ArchiveWorkspaceData, MxrError> {
        let event_tx = self.event_tx.clone();
        self.conn
            .capture_workspace_with_events(move |event| {
                if let Some(tx) = &event_tx {
                    let _ = tx.send(event);
                }
            })
            .await
            .map_err(map_request_error)
    }
    pub(crate) async fn raw_request_owned(
        &mut self,
        req: Request,
        owner: Option<&mxr_protocol::ArchiveWorkspaceData>,
    ) -> Result<Response, MxrError> {
        if let Request::SystemMailboxCounts { account_id } = req {
            return self.system_mailbox_counts(account_id).await;
        }
        let event_tx = self.event_tx.clone();
        self.conn
            .request_response_owned(
                req,
                owner,
                move |event| {
                    if let Some(tx) = &event_tx {
                        let _ = tx.send(event);
                    }
                },
                None,
            )
            .await
            .map_err(map_request_error)
    }

    /// `SystemMailboxCounts` through the shared client helper: a daemon that
    /// does not know it yields an `Unsupported` response on a connection that
    /// is already healthy again, so the labels' own counts stand in.
    async fn system_mailbox_counts(
        &mut self,
        account_id: Option<AccountId>,
    ) -> Result<Response, MxrError> {
        let target = self.target.clone();
        match self
            .conn
            .system_mailbox_counts(account_id, || async move {
                target.connect(ClientKind::Tui).await
            })
            .await
            .map_err(map_request_error)?
        {
            Some(mailboxes) => Ok(Response::Ok {
                data: ResponseData::SystemMailboxCounts { mailboxes },
            }),
            None => Ok(Response::error_kinded(
                "This daemon does not provide system mailbox counts",
                IpcErrorKind::Unsupported,
            )),
        }
    }

    async fn request(&mut self, req: Request) -> Result<Response, MxrError> {
        if let Request::SystemMailboxCounts { account_id } = req {
            return self.system_mailbox_counts(account_id).await;
        }
        // No connection-level timeout: the IPC worker bounds every request
        // (see `ipc::IPC_REQUEST_TIMEOUT`) and owns the reconnect-on-timeout
        // policy, so the mechanism here stays a plain awaited exchange.
        let event_tx = self.event_tx.clone();
        self.conn
            .request_response(
                req,
                |event| {
                    if let Some(ref tx) = event_tx {
                        let _ = tx.send(event);
                    }
                },
                None,
            )
            .await
            .map_err(map_request_error)
    }

    /// Every enabled account's mail (the All Mail mailbox), newest first.
    pub async fn list_envelopes(
        &mut self,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<Envelope>, MxrError> {
        let resp = self
            .request(Request::ListEnvelopes {
                label_id: None,
                account_id: None,
                limit,
                offset,
                label_role: all_mail_role(),
            })
            .await?;

        match resp {
            Response::Ok {
                data: ResponseData::Envelopes { envelopes },
            } => Ok(envelopes),
            Response::Error { message, .. } => Err(MxrError::Ipc(message)),
            _ => Err(MxrError::Ipc("Unexpected response".into())),
        }
    }

    /// The default account's labels, with each system mailbox's counts
    /// summed over every account. A failed sum request keeps the default
    /// account's counts: the sidebar still renders.
    pub async fn list_labels(&mut self) -> Result<Vec<Label>, MxrError> {
        let resp = self
            .request(Request::ListLabels {
                account_id: None,
                all_accounts: false,
            })
            .await?;
        let labels = match resp {
            Response::Ok {
                data: ResponseData::Labels { labels },
            } => labels,
            Response::Error { message, .. } => return Err(MxrError::Ipc(message)),
            _ => return Err(MxrError::Ipc("Unexpected response".into())),
        };
        let all = match self
            .request(Request::ListLabels {
                account_id: None,
                all_accounts: true,
            })
            .await
        {
            Ok(Response::Ok {
                data: ResponseData::Labels { labels },
            }) => labels,
            _ => return Ok(labels),
        };
        let counts = match self
            .request(Request::SystemMailboxCounts { account_id: None })
            .await
        {
            Ok(Response::Ok {
                data: ResponseData::SystemMailboxCounts { mailboxes },
            }) => mailboxes,
            _ => Vec::new(),
        };
        Ok(with_unified_system_counts(labels, &all, &counts))
    }

    /// Phase F: paginated thread list. Provides the TUI's read path
    /// for `Request::ListThreads`. The TUI's existing mailbox screen
    /// renders messages grouped client-side by `thread_id`; a future
    /// dedicated thread-pane view can call this to render threads
    /// directly (each `Thread` carries its `message_ids`).
    pub async fn list_threads(
        &mut self,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<mxr_core::types::Thread>, MxrError> {
        let resp = self
            .request(Request::ListThreads {
                account_id: None,
                label_id: None,
                limit,
                offset,
                sort: None,
                label_role: all_mail_role(),
            })
            .await?;
        match resp {
            Response::Ok {
                data: ResponseData::Threads { threads },
            } => Ok(threads),
            Response::Error { message, .. } => Err(MxrError::Ipc(message)),
            _ => Err(MxrError::Ipc("Unexpected response".into())),
        }
    }

    pub async fn search(
        &mut self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<SearchResultItem>, MxrError> {
        let resp = self
            .request(Request::Search {
                query: query.to_string(),
                limit,
                offset: 0,
                account_id: None,
                mode: None,
                sort: Some(mxr_core::types::SortOrder::DateDesc),
                explain: false,
                limit_from_config: false,
            })
            .await?;
        match resp {
            Response::Ok {
                data: ResponseData::SearchResults { results, .. },
            } => Ok(results),
            Response::Error { message, .. } => Err(MxrError::Ipc(message)),
            _ => Err(MxrError::Ipc("Unexpected response".into())),
        }
    }

    pub async fn get_body(&mut self, message_id: &MessageId) -> Result<MessageBody, MxrError> {
        let resp = self
            .request(Request::GetBody {
                message_id: message_id.clone(),
            })
            .await?;
        match resp {
            Response::Ok {
                data: ResponseData::Body { body },
            } => Ok(body),
            Response::Error { message, .. } => Err(MxrError::Ipc(message)),
            _ => Err(MxrError::Ipc("Unexpected response".into())),
        }
    }

    pub async fn get_thread(
        &mut self,
        thread_id: &ThreadId,
    ) -> Result<(Thread, Vec<Envelope>), MxrError> {
        let resp = self
            .request(Request::GetThread {
                thread_id: thread_id.clone(),
            })
            .await?;
        match resp {
            Response::Ok {
                data:
                    ResponseData::Thread {
                        thread, messages, ..
                    },
            } => Ok((thread, messages)),
            Response::Error { message, .. } => Err(MxrError::Ipc(message)),
            _ => Err(MxrError::Ipc("Unexpected response".into())),
        }
    }

    pub async fn list_saved_searches(
        &mut self,
    ) -> Result<Vec<mxr_core::types::SavedSearch>, MxrError> {
        let resp = self.request(Request::ListSavedSearches).await?;
        match resp {
            Response::Ok {
                data: ResponseData::SavedSearches { searches },
            } => Ok(searches),
            Response::Error { message, .. } => Err(MxrError::Ipc(message)),
            _ => Err(MxrError::Ipc("Unexpected response".into())),
        }
    }

    pub async fn list_subscriptions(
        &mut self,
        limit: u32,
    ) -> Result<Vec<mxr_core::types::SubscriptionSummary>, MxrError> {
        let resp = self
            .request(Request::ListSubscriptions {
                account_id: None,
                limit,
            })
            .await?;
        match resp {
            Response::Ok {
                data: ResponseData::Subscriptions { subscriptions },
            } => Ok(subscriptions),
            Response::Error { message, .. } => Err(MxrError::Ipc(message)),
            _ => Err(MxrError::Ipc("Unexpected response".into())),
        }
    }

    /// Read one frame while no request is in flight, forwarding events.
    /// Returns `Err` when the connection is closed or broken so the caller can
    /// trigger its reconnect path. Cancel-safe: dropping the future leaves
    /// any partial frame buffered inside `framed`.
    pub(crate) async fn read_idle_frame(&mut self) -> Result<(), MxrError> {
        match self.conn.next_event().await {
            Ok(msg) => {
                match msg.payload {
                    IpcPayload::Event(event) => {
                        if let Some(ref tx) = self.event_tx {
                            let _ = tx.send(event);
                        }
                    }
                    other => {
                        warn!(?other, "unexpected idle IPC frame; dropping");
                    }
                }
                Ok(())
            }
            Err(ClientError::Closed) => Err(MxrError::Ipc("connection closed".into())),
            Err(ClientError::Io(source)) => {
                Err(MxrError::Ipc(describe_ipc_failure(&source.to_string())))
            }
            Err(other) => Err(map_request_error(other)),
        }
    }

    pub async fn ping(&mut self) -> Result<(), MxrError> {
        let resp = self.request(Request::Ping).await?;
        match resp {
            Response::Ok {
                data: ResponseData::Pong,
            } => Ok(()),
            _ => Err(MxrError::Ipc("Unexpected response".into())),
        }
    }
}

/// Map a `mxr-client` failure onto the `MxrError::Ipc` strings the TUI worker
/// classifies on (`should_reconnect_ipc` matches substrings such as
/// "connection closed" / "broken pipe" / "connection reset"). The `Io` arm
/// keeps the protocol-mismatch hint via [`describe_ipc_failure`], which is
/// otherwise a pass-through so those substrings survive.
fn map_request_error(error: ClientError) -> MxrError {
    match error {
        ClientError::Closed => MxrError::Ipc(
            "Connection closed. The running daemon may be using an incompatible protocol. Restart the daemon after upgrading.".into(),
        ),
        ClientError::Io(source) => MxrError::Ipc(describe_ipc_failure(&source.to_string())),
        ClientError::DaemonIncompatible { message } => {
            MxrError::Ipc(describe_ipc_failure(&message))
        }
        ClientError::Daemon { message, .. } => MxrError::Ipc(message),
        // Intentional deviation: the old `request` loop silently skipped a
        // rogue frame and kept reading; we now surface it as an error. A frame
        // that does not correlate means the connection is out of step, and
        // skipping risks parking the worker; it is also unreachable in practice
        // (one in-flight request per connection, and the worker drops the
        // connection on timeout — see ipc.rs). Not a reconnect trigger.
        ClientError::UnexpectedFrame {
            frame_id,
            expected_id,
            ..
        } => MxrError::Ipc(format!(
            "IPC protocol error: unexpected frame id {frame_id} while awaiting response {expected_id}"
        )),
        ClientError::Timeout(duration) => MxrError::Ipc(format!(
            "IPC request timed out after {} seconds",
            duration.as_secs()
        )),
        ClientError::Connect { source, .. } => MxrError::Ipc(source.to_string()),
        // Transport-level connect failure (generic-connector path). The TUI
        // dials via the path constructor, so this is unreachable here; the arm
        // keeps the match exhaustive.
        ClientError::Transport(error) => MxrError::Ipc(error.to_string()),
    }
}

fn describe_ipc_failure(message: &str) -> String {
    if message.contains("unknown variant") || message.contains("missing field") {
        format!("IPC protocol mismatch: {message}. Restart the daemon after upgrading.")
    } else {
        message.to_string()
    }
}
