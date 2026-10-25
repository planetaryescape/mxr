use crate::app::{self, App, AttachmentOperation};
use crate::client::Client;
use crate::local_state;
use crossterm::event::EventStream;
use futures::StreamExt;
use mxr_config::load_config;
use mxr_core::MxrError;
use mxr_protocol::{Request, Response, ResponseData};
use ratatui::crossterm::event::Event;
use tokio::sync::mpsc;
use tokio::time::Instant;

use crate::account_workflow::{
    ipc_get_auth_session, ipc_start_auth_session, request_account_operation,
    run_account_save_workflow, selected_daemon_target,
};
use crate::accounts_helpers::load_accounts_page_accounts;
use crate::async_result::{AsyncResult, StatusSnapshot, UnsubscribeResultData};
use crate::compose_flow::{
    fetch_reply_context_dedicated, handle_compose_action, handle_compose_editor_status,
    handle_draft_edit_status, prepare_draft_edit,
};
use crate::daemon_events::{
    apply_all_envelopes_refresh, apply_labels_refresh, apply_thread_context_loaded,
    apply_thread_gist_loaded, apply_thread_summary_loaded, format_mutation_failure,
    handle_daemon_event, handle_sync_batch, handle_sync_request_failed, mutation_verb_past,
    restore_mail_list_selection,
};
use crate::editor::{edit_tui_config, open_diagnostics_pane_details, open_tui_log_file};
use crate::ipc::{
    ipc_call, ipc_call_dedicated, ipc_call_owned, ipc_transfer_attachment,
    spawn_ipc_worker_for_session, AttachmentTransferRequest, IpcRequest,
};
use crate::local_io::{handle_result as handle_local_io_result, submit_bug_report_write};
use crate::runtime::{
    spawn_replaceable_request_worker, spawn_task_worker, submit_task, ReplaceableRequest,
    ReplaceableRequestKey,
};

const SUMMARY_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(150);

fn require_sync_all_protocol(protocol_version: u32) -> Result<(), MxrError> {
    if protocol_version < mxr_protocol::IPC_PROTOCOL_VERSION {
        return Err(MxrError::Ipc(format!(
            "Global sync requires daemon IPC protocol {}; running daemon uses {protocol_version}. Update or restart the daemon before retrying.",
            mxr_protocol::IPC_PROTOCOL_VERSION
        )));
    }
    Ok(())
}

fn archive_partial_undo(
    details: Option<serde_json::Value>,
    verb: &str,
) -> Option<app::PendingUndo> {
    let operation: mxr_protocol::ArchiveOperationData = serde_json::from_value(details?).ok()?;
    let count = operation
        .items
        .iter()
        .filter(|item| item.state == mxr_protocol::ArchiveItemStateData::Completed)
        .count() as u32;
    (operation.undo_available && count > 0).then(|| app::PendingUndo {
        action: app::UndoAction::Mutations(vec![operation.mutation_id]),
        verb_past: verb.into(),
        count,
        applied_at: std::time::Instant::now(),
    })
}

fn mutation_can_auto_retry(request: &Request) -> bool {
    !matches!(
        request,
        Request::SendDraft { .. }
            | Request::SendStoredDraft { .. }
            | Request::SubmitArchive { .. }
            // A retry after the daemon applied it would find nothing to
            // remove and bury the undo it made.
            | Request::RemoveHighlight { .. }
            | Request::Mutation {
                mutation: mxr_protocol::MutationCommand::Archive { .. },
                ..
            }
    )
}

fn run_with_terminal_suspended_with<
    Terminal,
    Events,
    RestoreTerminal,
    InitTerminal,
    InitEvents,
    Action,
    R,
>(
    terminal: &mut Terminal,
    events: &mut Option<Events>,
    restore_terminal: RestoreTerminal,
    init_terminal: InitTerminal,
    init_events: InitEvents,
    action: Action,
) -> R
where
    RestoreTerminal: FnOnce(),
    InitTerminal: FnOnce() -> Terminal,
    InitEvents: FnOnce() -> Events,
    Action: FnOnce() -> R,
{
    drop(events.take());
    restore_terminal();
    let result = action();
    *terminal = init_terminal();
    *events = Some(init_events());
    result
}

fn run_with_terminal_suspended<R>(
    terminal: &mut ratatui::DefaultTerminal,
    events: &mut Option<EventStream>,
    action: impl FnOnce() -> R,
) -> R {
    run_with_terminal_suspended_with(
        terminal,
        events,
        crate::terminal_session::restore,
        crate::terminal_session::init,
        EventStream::new,
        action,
    )
}

/// Drives the Outlook device-code flow in a background task, forwarding each
/// `GetAuthSession` state update as an `AsyncResult::AuthSession` event so the
/// TUI can render the device-code overlay and react to terminal states.
async fn spawn_outlook_auth_session(
    bg: &mpsc::UnboundedSender<IpcRequest>,
    result_tx: mpsc::UnboundedSender<AsyncResult>,
    account: mxr_protocol::AccountConfigData,
    reauthorize: bool,
) {
    use mxr_protocol::AuthSessionStateData;

    let session = match ipc_start_auth_session(bg, account, reauthorize).await {
        Ok(s) => s,
        Err(e) => {
            let _ = result_tx.send(AsyncResult::AccountOperation(Err(e)));
            return;
        }
    };

    let is_terminal = |s: &AuthSessionStateData| {
        matches!(
            s,
            AuthSessionStateData::Authorized
                | AuthSessionStateData::Failed
                | AuthSessionStateData::Cancelled
        )
    };

    if is_terminal(&session.state) {
        let _ = result_tx.send(AsyncResult::AuthSession(session));
        return;
    }

    let session_id = session.session_id.clone();
    let poll_interval = std::time::Duration::from_secs(session.poll_interval_secs.unwrap_or(5));

    const MAX_CONSECUTIVE_POLL_FAILURES: u32 = 5;
    let mut consecutive_failures = 0u32;

    loop {
        tokio::time::sleep(poll_interval).await;

        match ipc_get_auth_session(bg, session_id.clone()).await {
            Ok(updated) => {
                consecutive_failures = 0;
                let done = is_terminal(&updated.state);
                let _ = result_tx.send(AsyncResult::AuthSession(updated));
                if done {
                    break;
                }
            }
            Err(e) => {
                consecutive_failures += 1;
                if consecutive_failures >= MAX_CONSECUTIVE_POLL_FAILURES {
                    let _ = result_tx.send(AsyncResult::AccountOperation(Err(e)));
                    break;
                }
                tracing::debug!(error = %e, consecutive_failures, "auth session poll failed; retrying");
            }
        }
    }
}

async fn load_open_commitment_counts(
    bg: &mpsc::UnboundedSender<IpcRequest>,
    envelopes: Vec<mxr_core::Envelope>,
) -> std::collections::HashMap<(mxr_core::AccountId, mxr_core::ThreadId), u32> {
    let account_ids = envelopes
        .iter()
        .map(|envelope| envelope.account_id.clone())
        .collect::<std::collections::HashSet<_>>();
    let mut counts = std::collections::HashMap::new();

    for account_id in account_ids {
        let response = ipc_call(
            bg,
            Request::ListCommitments {
                account_id,
                email: None,
                status: Some(mxr_protocol::CommitmentStatusData::Open),
            },
        )
        .await;

        let Ok(Response::Ok {
            data: ResponseData::CommitmentList { commitments },
        }) = response
        else {
            continue;
        };

        for commitment in commitments {
            *counts
                .entry((commitment.account_id, commitment.thread_id))
                .or_insert(0) += 1;
        }
    }

    counts
}

fn format_platform_response(data: &ResponseData) -> String {
    match data {
        ResponseData::DraftSuggestion {
            body,
            model,
            voice_match,
            humanizer,
            rewrite_iterations,
            context_note,
            provenance,
            ..
        } => {
            let mut lines = vec![body.trim().to_string(), String::new()];
            match provenance {
                Some(provenance) => {
                    lines.push(provenance.summary_line());
                    lines.extend(provenance.source_lines());
                }
                None => lines.push(format!("Model: {model}")),
            }
            if let Some(note) = context_note {
                lines.push(note.clone());
            }
            if let Some(voice_match) = voice_match {
                lines.push(format!(
                    "Voice match: {:.0}% ({:?})",
                    voice_match.score * 100.0,
                    voice_match.confidence
                ));
                if !voice_match.notable_deltas.is_empty() {
                    lines.push(format!("Deltas: {}", voice_match.notable_deltas.join(", ")));
                }
            }
            if let Some(humanizer) = humanizer {
                lines.push(format!("Humanizer: {}/100", humanizer.score));
            }
            if *rewrite_iterations > 0 {
                lines.push(format!("Rewritten: {rewrite_iterations}x"));
            }
            lines.join("\n")
        }
        ResponseData::CommitmentList { commitments } => {
            if commitments.is_empty() {
                return "No open commitments.".into();
            }
            commitments
                .iter()
                .map(|commitment| {
                    let due = commitment
                        .by_when
                        .map(|date| {
                            let unverified = if commitment.date_unverified {
                                " (date unverified)"
                            } else {
                                ""
                            };
                            format!(" due {}{unverified}", date.format("%Y-%m-%d"))
                        })
                        .unwrap_or_default();
                    format!(
                        "- {:?} · {}{}\n  {}\n  id: {}",
                        commitment.direction,
                        commitment.who_owes,
                        due,
                        commitment.what,
                        commitment.id
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
        ResponseData::UserVoice { profile } => match profile {
            Some(profile) => {
                let mut lines = vec![
                    format!("Samples: {} outbound messages", profile.msg_count_used),
                    format!("Formality: {:.2}", profile.formality_score),
                    format!("Average sentence: {:.1} words", profile.avg_sentence_len),
                    String::new(),
                    "Registers:".into(),
                ];
                for mode in &profile.register_modes {
                    lines.push(format!(
                        "- {:?}: formality {:.2}, avg sentence {:.1}, exemplars {}",
                        mode.register,
                        mode.formality_score,
                        mode.avg_sentence_len,
                        mode.exemplar_message_ids.len()
                    ));
                }
                lines.join("\n")
            }
            None => "No usable voice profile yet. Send more outbound mail, then rebuild.".into(),
        },
        ResponseData::HumanizerReport { report } => {
            format!(
                "Humanizer: {}/100\n{} hits",
                report.score,
                report.hits.len()
            )
        }
        ResponseData::HumanizedText {
            text,
            report,
            iterations,
            rewrite,
        } => {
            let by = rewrite
                .as_ref()
                .map_or_else(String::new, |rewrite| format!("\n{}", rewrite.label()));
            format!(
                "{}\n\nHumanizer: {}/100\nRewritten: {}x{by}",
                text.trim(),
                report.score,
                iterations
            )
        }
        ResponseData::ArchiveOperation { operation } => {
            let mut lines = vec![
                format!("Archive operation {}", operation.identity.operation_id),
                format!(
                    "Accepted: {}; running: {}",
                    operation.accepted, operation.running
                ),
            ];
            for item in &operation.items {
                lines.push(format!(
                    "{} {:?}: {}",
                    item.message_id,
                    item.state,
                    item.reason.as_deref().unwrap_or("")
                ));
            }
            if !operation.accepted {
                lines.push("Review retained. Open the command palette and choose Submit reviewed with this ID to apply this exact selection.".into());
            }
            lines.push("Unknown effects require reconciliation. Query again; never start a new operation to repeat them.".into());
            lines.join("\n")
        }
        ResponseData::SendOutcome {
            outcome: Some(outcome),
        } => {
            let state = match outcome.state {
                mxr_core::SendOutcomeState::Accepted => {
                    "Provider accepted this send. Do not resend."
                }
                mxr_core::SendOutcomeState::Unknown => "Acceptance uncertain. Do not resend.",
                mxr_core::SendOutcomeState::NotAccepted => {
                    "Provider did not accept this send. Review before a new attempt."
                }
            };
            format!(
                "{state}\n\nOperation: {}\nAccount: {}\nReviewed revision: {}\n\nOpen the command palette and choose Check last send outcome to query this retained outcome again.",
                outcome.identity.operation_id,
                outcome.account_id,
                outcome.identity.expected_revision
            )
        }
        ResponseData::SendOutcome { outcome: None } => {
            "No submission outcome recorded for this operation.".into()
        }
        ResponseData::Ack => "Done.".into(),
        other => serde_json::to_string_pretty(other).unwrap_or_else(|_| format!("{other:?}")),
    }
}

pub async fn run() -> anyhow::Result<()> {
    let target = selected_daemon_target()?;
    let mut client = Client::connect_target(&target).await?;
    let owner = tokio::time::timeout(crate::ipc::IPC_REQUEST_TIMEOUT, client.capture_workspace())
        .await
        .ok()
        .and_then(Result::ok);
    let session = crate::client::DaemonSession { target, owner };
    let config = load_config()?;
    let local_state = local_state::load();

    let mut app = App::from_config(&config);
    let config_path = mxr_config::config_file_path();
    if let Some(directory) = config_path.parent() {
        app.load_keybindings_from(directory);
    }
    if session.target.is_paired_remote() {
        app.enable_paired_remote_read_only_session();
    }
    app.compose.workspace_owner = session.owner.clone();
    match crate::send_recovery_state::last() {
        Ok(identity) => {
            app.compose.last_send_identity = identity;
            app.compose.last_send_query = crate::send_recovery_state::last_query().ok().flatten();
        }
        Err(error) => app.push_toast(crate::app::Toast::error(format!(
            "Cannot read send recovery; do not resend: {error}"
        ))),
    }
    app.command_palette
        .palette
        .restore_recents_from_labels(&local_state.recent_action_labels);
    if config.accounts.is_empty() && !session.target.is_remote() {
        app.accounts.page.refresh_pending = true;
    } else {
        app.load(&mut client).await?;
        // Now is the front page; the inbox stays loaded one key away.
        // No tour: each mode teaches itself with hints at the element they
        // explain, the first time it is needed (blueprint 22, D118).
        if session.target.is_paired_remote() {
            app.apply(crate::action::Action::OpenMailboxScreen);
            app.status_message = Some("Remote mailbox: reading and downloads are available; replies require host-approved send access".into());
        } else {
            app.apply(crate::action::Action::OpenNow);
        }
        // Load accounts for sidebar account section
        app.accounts.page.refresh_pending = !session.target.is_paired_remote();
    }

    let mut terminal = crate::terminal_session::init();
    let _restore = crate::terminal_session::RestoreOnDrop;
    app.set_terminal_image_support(crate::terminal_images::TerminalImageSupport::detect());
    let mut events = Some(EventStream::new());

    // Channels for async results
    let (result_tx, mut result_rx) = mpsc::unbounded_channel::<AsyncResult>();

    // Background IPC worker — also forwards daemon events to result_tx
    let bg = spawn_ipc_worker_for_session(session.clone(), result_tx.clone());
    let html_assets =
        crate::terminal_images::spawn_html_image_asset_worker(bg.clone(), result_tx.clone());
    let html_decodes = crate::terminal_images::spawn_html_image_decode_worker(result_tx.clone());
    let replaceable = spawn_replaceable_request_worker(bg.clone(), result_tx.clone());
    let queued = spawn_task_worker(result_tx.clone());
    let local_io = spawn_task_worker(result_tx.clone());
    let assistant_io = spawn_task_worker(result_tx.clone());

    loop {
        crate::local_io::submit_pending_work(&mut app, &local_io);

        if std::mem::take(&mut app.pending_suspend) {
            // Restores the terminal, stops until the shell resumes us, then
            // takes the terminal back; the next draw repaints everything.
            run_with_terminal_suspended(
                &mut terminal,
                &mut events,
                crate::terminal_session::suspend_process,
            );
        }

        if app.diagnostics.pending_config_edit {
            app.diagnostics.pending_config_edit = false;
            let result = run_with_terminal_suspended(&mut terminal, &mut events, || {
                edit_tui_config(&mut app)
            });
            match result {
                Ok(message) => {
                    app.status_message = Some(message);
                }
                Err(error) => {
                    app.modals.error = Some(app::ErrorModalState::new(
                        "Config Reload Failed",
                        format!(
                            "Config could not be reloaded after editing.\n\n{error}\n\nFix the file and run Edit Config again."
                        ),
                    ));
                    app.status_message = Some(format!("Config reload failed: {error}"));
                }
            }
        }
        if app.diagnostics.pending_log_open {
            app.diagnostics.pending_log_open = false;
            let result = run_with_terminal_suspended(&mut terminal, &mut events, open_tui_log_file);
            match result {
                Ok(message) => {
                    app.status_message = Some(message);
                }
                Err(error) => {
                    app.modals.error = Some(app::ErrorModalState::new(
                        "Open Logs Failed",
                        format!(
                            "The log file could not be opened.\n\n{error}\n\nCheck that the daemon has created the log file and try again."
                        ),
                    ));
                    app.status_message = Some(format!("Open logs failed: {error}"));
                }
            }
        }
        if let Some(pane) = app.diagnostics.pending_details.take() {
            let result = run_with_terminal_suspended(&mut terminal, &mut events, || {
                open_diagnostics_pane_details(&app.diagnostics.page, pane)
            });
            match result {
                Ok(message) => {
                    app.status_message = Some(message);
                }
                Err(error) => {
                    app.modals.error = Some(app::ErrorModalState::new(
                        "Diagnostics Open Failed",
                        format!(
                            "The diagnostics source could not be opened.\n\n{error}\n\nTry refresh first, then open details again."
                        ),
                    ));
                    app.status_message = Some(format!("Open diagnostics failed: {error}"));
                }
            }
        }

        // Batch body fetches, keeping the selected message ahead of window prefetches.
        let body_fetch_ids = if !app.mailbox.priority_body_fetches.is_empty() {
            let ids = std::mem::take(&mut app.mailbox.priority_body_fetches);
            for id in &ids {
                app.mailbox
                    .queued_body_fetches
                    .retain(|queued| queued != id);
            }
            Some(ids)
        } else if !app.mailbox.queued_body_fetches.is_empty() {
            Some(std::mem::take(&mut app.mailbox.queued_body_fetches))
        } else {
            None
        };
        if let Some(ids) = body_fetch_ids {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let requested = ids;
                let resp = ipc_call(
                    &bg,
                    Request::ListBodies {
                        message_ids: requested.clone(),
                    },
                )
                .await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::Bodies { bodies, failures },
                    }) => Ok((bodies, failures)),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response to ListBodies".into())),
                };
                AsyncResult::Bodies { requested, result }
            });
        }

        if !app.queued_html_image_asset_fetches.is_empty() {
            let ids = std::mem::take(&mut app.queued_html_image_asset_fetches);
            for message_id in ids {
                app.in_flight_html_image_asset_requests
                    .insert(message_id.clone());
                let allow_remote = app.mailbox.remote_content_enabled;
                let _ = html_assets.send(crate::terminal_images::HtmlImageAssetRequest {
                    message_id,
                    allow_remote,
                });
            }
        }

        if !app.queued_html_image_decodes.is_empty() {
            let keys = std::mem::take(&mut app.queued_html_image_decodes);
            for key in keys {
                let path = app
                    .html_image_assets
                    .get(&key.message_id)
                    .and_then(|assets| assets.get(&key.source))
                    .and_then(|entry| entry.asset.path.clone());
                if let Some(path) = path {
                    let _ = html_decodes
                        .send(crate::terminal_images::HtmlImageDecodeRequest { key, path });
                }
            }
        }

        if let Some(thread_id) = app.mailbox.pending_thread_fetch.take() {
            app.mailbox.in_flight_thread_fetch = Some(thread_id.clone());
            app.mailbox.thread_request_id = app.mailbox.thread_request_id.wrapping_add(1);
            let _ = replaceable.send(ReplaceableRequest::Thread {
                thread_id,
                request_id: app.mailbox.thread_request_id,
                enqueued_at: Instant::now(),
            });
        }

        terminal.draw(|frame| app.draw(frame))?;

        let timeout = if app.input_pending() {
            std::time::Duration::from_millis(500)
        } else {
            std::time::Duration::from_secs(60)
        };
        let timeout = app.next_background_timeout(timeout);

        // Spawn non-blocking search
        if let Some(pending) = app.search.pending.take() {
            let _ = replaceable.send(ReplaceableRequest::Search(pending));
        }

        if let Some(pending) = app.search.pending_count.take() {
            let _ = replaceable.send(ReplaceableRequest::SearchCount(pending));
        }

        if let Some(pending) = app.modals.pending_unsubscribe_action.take() {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let result = if pending.archive_message_ids.is_empty() {
                    let unsubscribe_resp = ipc_call(
                        &bg,
                        Request::Unsubscribe {
                            message_id: pending.message_id.clone(),
                        },
                    )
                    .await;
                    match unsubscribe_resp {
                        Ok(Response::Ok {
                            data: ResponseData::Ack,
                        }) => Ok(UnsubscribeResultData {
                            archived_ids: Vec::new(),
                            message: format!("Unsubscribed from {}", pending.sender_email),
                        }),
                        Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                        Err(error) => Err(error),
                        _ => Err(MxrError::Ipc("unexpected response to Unsubscribe".into())),
                    }
                } else {
                    let purge_resp = ipc_call(
                        &bg,
                        Request::UnsubscribePurge {
                            address: pending.sender_email.clone(),
                            account_id: Some(pending.account_id.clone()),
                            dry_run: false,
                            archive_on_no_method: false,
                            preview_token: None,
                        },
                    )
                    .await;
                    match purge_resp {
                        Ok(Response::Ok {
                            data: ResponseData::UnsubscribePurgeResult { result },
                        }) if result.archived_count > 0 => Ok(UnsubscribeResultData {
                            archived_ids: result.message_ids,
                            message: format!(
                                "Unsubscribed and cleared {} messages from {}{}",
                                result.archived_count,
                                pending.sender_email,
                                result
                                    .mutation_id
                                    .as_ref()
                                    .map(|id| format!(" (undo: {id})"))
                                    .unwrap_or_default()
                            ),
                        }),
                        Ok(Response::Ok {
                            data: ResponseData::UnsubscribePurgeResult { result },
                        }) => Err(MxrError::Ipc(result.error.unwrap_or_else(|| {
                            "unsubscribe-and-clear did not archive messages".into()
                        }))),
                        Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                        Err(error) => Err(error),
                        _ => Err(MxrError::Ipc(
                            "unexpected response to UnsubscribePurge".into(),
                        )),
                    }
                };
                AsyncResult::Unsubscribe(result)
            });
        }

        if app.rules.page.refresh_pending {
            app.rules.page.refresh_pending = false;
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let resp = ipc_call(&bg, Request::ListRules).await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::Rules { rules },
                    }) => Ok(rules),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response to ListRules".into())),
                };
                AsyncResult::Rules(result)
            });
        }

        if app.pending_snippets_refresh {
            app.pending_snippets_refresh = false;
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let resp = ipc_call(&bg, Request::ListSnippets).await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::Snippets { snippets },
                    }) => Ok(snippets),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response to ListSnippets".into())),
                };
                AsyncResult::SnippetsList(result)
            });
        }

        if app.pending_reply_queue_refresh {
            app.pending_reply_queue_refresh = false;
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let resp = ipc_call(&bg, Request::ListReplyQueue).await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::ReplyQueue { messages },
                    }) => Ok(messages),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc(
                        "unexpected response to ListReplyQueue".into(),
                    )),
                };
                AsyncResult::ReplyQueueList(result)
            });
        }

        if app.pending_drafts_refresh {
            app.pending_drafts_refresh = false;
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let resp = ipc_call(&bg, Request::ListDrafts).await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::Drafts { drafts },
                    }) => {
                        // Which drafts are scheduled is extra detail: a
                        // failure here must not hide the drafts themselves.
                        let sends =
                            match ipc_call(&bg, Request::ListScheduledSends { account_id: None })
                                .await
                            {
                                Ok(Response::Ok {
                                    data: ResponseData::ScheduledSends { sends },
                                }) => sends,
                                _ => Vec::new(),
                            };
                        Ok((drafts, sends))
                    }
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response to ListDrafts".into())),
                };
                AsyncResult::StoredDraftsLoaded(result)
            });
        }

        if let Some(draft) = app.pending_draft_edit_request.take() {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                AsyncResult::DraftEditReady(prepare_draft_edit(&bg, draft).await)
            });
        }

        if let Some(operation) = app.pending_draft_operation.take() {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                match operation {
                    app::StoredDraftOperation::Delete { draft } => {
                        let draft_id = draft.id.clone();
                        let result = match ipc_call(
                            &bg,
                            Request::DeleteDraft {
                                expected_revision: draft.revision,
                                draft_id: draft.id,
                            },
                        )
                        .await
                        {
                            Ok(Response::Ok {
                                data: ResponseData::Ack,
                            }) => Ok(()),
                            Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                            Err(error) => Err(error),
                            _ => Err(MxrError::Ipc("unexpected response to DeleteDraft".into())),
                        };
                        AsyncResult::StoredDraftDeleted { draft_id, result }
                    }
                    app::StoredDraftOperation::CancelSchedule { draft, .. } => {
                        let draft_id = draft.id.clone();
                        let result = match ipc_call(
                            &bg,
                            Request::CancelScheduledSend { draft_id: draft.id },
                        )
                        .await
                        {
                            Ok(Response::Ok {
                                data: ResponseData::Ack,
                            }) => Ok(()),
                            Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                            Err(error) => Err(error),
                            _ => Err(MxrError::Ipc(
                                "unexpected response to CancelScheduledSend".into(),
                            )),
                        };
                        AsyncResult::StoredDraftUnscheduled { draft_id, result }
                    }
                    app::StoredDraftOperation::Push { draft, provider } => {
                        let draft_id = draft.id.clone();
                        let result = match ipc_call(&bg, Request::SaveDraftToServer { draft }).await
                        {
                            Ok(Response::Ok {
                                data: ResponseData::Ack | ResponseData::Draft { .. },
                            }) => Ok(()),
                            Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                            Err(error) => Err(error),
                            _ => Err(MxrError::Ipc(
                                "unexpected response to SaveDraftToServer".into(),
                            )),
                        };
                        AsyncResult::StoredDraftPushed {
                            draft_id,
                            provider,
                            result,
                        }
                    }
                }
            });
        }

        if app.pending_deliveries_refresh {
            app.pending_deliveries_refresh = false;
            let filter = app.deliveries.filter.as_str().to_string();
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let resp = ipc_call(
                    &bg,
                    Request::ListDeliveries {
                        account_id: None,
                        filter: Some(filter),
                    },
                )
                .await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::Deliveries { deliveries },
                    }) => Ok(deliveries),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc(
                        "unexpected response to ListDeliveries".into(),
                    )),
                };
                AsyncResult::DeliveriesList(result)
            });
        }

        if let Some(delivery_id) = app.pending_delivery_resolve.take() {
            let filter = app.deliveries.filter.as_str().to_string();
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let _ = ipc_call(&bg, Request::ResolveDelivery { delivery_id }).await;
                let resp = ipc_call(
                    &bg,
                    Request::ListDeliveries {
                        account_id: None,
                        filter: Some(filter),
                    },
                )
                .await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::Deliveries { deliveries },
                    }) => Ok(deliveries),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc(
                        "unexpected response to ListDeliveries".into(),
                    )),
                };
                AsyncResult::DeliveriesList(result)
            });
        }

        if let Some(delivery_id) = app.pending_delivery_dismiss.take() {
            let filter = app.deliveries.filter.as_str().to_string();
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let _ = ipc_call(&bg, Request::DismissDelivery { delivery_id }).await;
                let resp = ipc_call(
                    &bg,
                    Request::ListDeliveries {
                        account_id: None,
                        filter: Some(filter),
                    },
                )
                .await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::Deliveries { deliveries },
                    }) => Ok(deliveries),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc(
                        "unexpected response to ListDeliveries".into(),
                    )),
                };
                AsyncResult::DeliveriesList(result)
            });
        }

        if let Some(thread_id) = app.pending_assistant_thread_open.take() {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let resp = ipc_call(&bg, Request::GetThread { thread_id }).await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::Thread { messages, .. },
                    }) => Ok(messages),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response to GetThread".into())),
                };
                AsyncResult::AssistantThreadOpened(result)
            });
        }

        if let Some(thread_id) = app.pending_delivery_open.take() {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let resp = ipc_call(&bg, Request::GetThread { thread_id }).await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::Thread { messages, .. },
                    }) => Ok(messages),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response to GetThread".into())),
                };
                AsyncResult::DeliveryThreadOpened(result)
            });
        }

        if app.pending_activity_refresh {
            app.pending_activity_refresh = false;
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let resp = ipc_call(
                    &bg,
                    Request::ListActivity {
                        filter: mxr_protocol::ActivityFilter {
                            since: Some(chrono::Utc::now().timestamp_millis() - 86_400_000),
                            ..Default::default()
                        },
                        limit: 100,
                        cursor: None,
                    },
                )
                .await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::ActivityEntries { entries, .. },
                    }) => Ok(entries),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response to ListActivity".into())),
                };
                AsyncResult::ActivityList(result)
            });
        }

        if app.pending_activity_pause_toggle {
            app.pending_activity_pause_toggle = false;
            let bg = bg.clone();
            let now_paused = app.modals.activity.paused;
            let _ = submit_task(&queued, async move {
                let req = if now_paused {
                    Request::ResumeActivity
                } else {
                    Request::PauseActivity { until_ts: None }
                };
                let resp = ipc_call(&bg, req).await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::Acknowledged,
                    }) => Ok(!now_paused),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response to PauseActivity".into())),
                };
                AsyncResult::ActivityPauseToggled(result)
            });
        }

        if let Some(account_id) = app.pending_screener_refresh.take() {
            let bg = bg.clone();
            let captured_account = account_id.clone();
            let _ = submit_task(&queued, async move {
                let resp = ipc_call(
                    &bg,
                    Request::ListScreenerQueue {
                        account_id: account_id.clone(),
                        limit: 100,
                        all_senders: false,
                    },
                )
                .await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::ScreenerQueue { entries },
                    }) => Ok(entries),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc(
                        "unexpected response to ListScreenerQueue".into(),
                    )),
                };
                AsyncResult::ScreenerQueueLoaded {
                    account_id: captured_account,
                    result,
                }
            });
        }

        for decision in std::mem::take(&mut app.pending_screener_decisions) {
            let bg = bg.clone();
            let captured_account = decision.account_id.clone();
            let captured_email = decision.sender_email.clone();
            let _ = submit_task(&queued, async move {
                let resp = ipc_call(
                    &bg,
                    Request::SetScreenerDecision {
                        account_id: decision.account_id,
                        sender_email: decision.sender_email,
                        disposition: decision.disposition,
                        route_label: None,
                    },
                )
                .await;
                let result = match resp {
                    Ok(Response::Ok { .. }) => Ok(()),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                };
                AsyncResult::ScreenerDecisionApplied {
                    account_id: captured_account,
                    sender_email: captured_email,
                    result,
                }
            });
        }

        if let Some((account_id, email)) = app.pending_sender_profile_request.take() {
            let bg = bg.clone();
            let captured_email = email.clone();
            let _ = submit_task(&queued, async move {
                let resp = ipc_call(
                    &bg,
                    Request::GetSenderProfile {
                        account_id,
                        email: email.clone(),
                    },
                )
                .await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::SenderProfile { profile },
                    }) => Ok(profile),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc(
                        "unexpected response to GetSenderProfile".into(),
                    )),
                };
                AsyncResult::SenderProfileLoaded {
                    email: captured_email,
                    result: Box::new(result),
                }
            });
        }

        // Drain a debounced summary request once its deadline has
        // elapsed. Holding down-arrow through the mail list re-stamps
        // the debounce on every row; only the thread the user actually
        // lands on (the last to outlast the 250ms window) goes to the
        // daemon. In-flight requests fired by an earlier debounce are
        // not cancelled — they complete and the daemon caches the
        // result for when the user comes back.
        if let Some((thread_id, deadline)) = app.pending_summary_debounce.as_ref() {
            if tokio::time::Instant::now() >= *deadline {
                let thread_id = thread_id.clone();
                app.pending_summary_debounce = None;
                // Only schedule the auto-summary if the thread is worth
                // summarizing. The user can still force a summary by
                // pressing `y` (Action::SummarizeCurrentThread bypasses
                // this gate entirely).
                let eligible = app
                    .mailbox
                    .viewed_thread
                    .as_ref()
                    .is_some_and(|thread| thread.id == thread_id)
                    && crate::app::auto_summary_eligible(
                        &app.mailbox.viewed_thread_messages,
                        &app.mailbox.body_cache,
                    );
                if eligible
                    && !app.mailbox.thread_summary_in_flight.contains(&thread_id)
                    && !app
                        .pending_summary_requests
                        .iter()
                        .any(|pending| pending == &thread_id)
                {
                    let _ = app.queue_thread_summary(thread_id);
                }
            }
        }

        if let Some(thread_id) = app.pending_summary_requests.pop_front() {
            let session = session.clone();
            let captured_id = thread_id.clone();
            let result_tx = result_tx.clone();
            tokio::spawn(async move {
                let resp = tokio::time::timeout(
                    SUMMARY_REQUEST_TIMEOUT,
                    ipc_call_dedicated(
                        &session,
                        Request::SummarizeThread {
                            thread_id: thread_id.clone(),
                        },
                    ),
                )
                .await;
                let result = match resp {
                    Ok(Ok(Response::Ok {
                        data: ResponseData::ThreadSummary { text, model },
                    })) => Ok((text, model)),
                    Ok(Ok(Response::Error { message, .. })) => Err(MxrError::Ipc(message)),
                    Ok(Ok(_)) => Err(MxrError::Ipc(
                        "unexpected response to SummarizeThread".into(),
                    )),
                    Ok(Err(e)) => Err(e),
                    Err(_) => Err(MxrError::Ipc(format!(
                        "summary request timed out after {}s",
                        SUMMARY_REQUEST_TIMEOUT.as_secs()
                    ))),
                };
                let _ = result_tx.send(AsyncResult::ThreadSummaryLoaded {
                    thread_id: captured_id,
                    result,
                });
            });
        }

        if let Some(message_id) = app.pending_promise_check.take() {
            let session = session.clone();
            let result_tx = result_tx.clone();
            tokio::spawn(async move {
                let resp = ipc_call_dedicated(
                    &session,
                    Request::DetectPromises {
                        source: mxr_protocol::PromiseSourceData::SentMessage {
                            message_id: message_id.clone(),
                        },
                        now: None,
                        // The TUI runs next to the daemon: its zone is ours.
                        time_zone: None,
                    },
                )
                .await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::Promises { detection },
                    }) => Ok(Box::new(detection)),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Ok(_) => Err(MxrError::Ipc(
                        "unexpected response to DetectPromises".into(),
                    )),
                    Err(e) => Err(e),
                };
                let _ = result_tx.send(AsyncResult::PromisesDetected { message_id, result });
            });
        }

        if let Some(thread_id) = app.pending_thread_context.take() {
            let session = session.clone();
            let result_tx = result_tx.clone();
            tokio::spawn(async move {
                let resp = ipc_call_dedicated(
                    &session,
                    Request::GetThreadContext {
                        thread_id: thread_id.clone(),
                    },
                )
                .await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::ThreadContext { context },
                    }) => Ok(Box::new(context)),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Ok(_) => Err(MxrError::Ipc(
                        "unexpected response to GetThreadContext".into(),
                    )),
                    Err(e) => Err(e),
                };
                let _ = result_tx.send(AsyncResult::ThreadContextLoaded { thread_id, result });
            });
        }

        if let Some((thread_id, deadline)) = app.pending_gist_debounce.as_ref() {
            if tokio::time::Instant::now() >= *deadline {
                let thread_id = thread_id.clone();
                app.pending_gist_debounce = None;
                let session = session.clone();
                let result_tx = result_tx.clone();
                tokio::spawn(async move {
                    let resp = tokio::time::timeout(
                        SUMMARY_REQUEST_TIMEOUT,
                        ipc_call_dedicated(
                            &session,
                            Request::GetThreadGist {
                                thread_id: thread_id.clone(),
                                refresh: false,
                            },
                        ),
                    )
                    .await;
                    let result = match resp {
                        Ok(Ok(Response::Ok {
                            data: ResponseData::ThreadGist { gist },
                        })) => Ok(Box::new(gist)),
                        Ok(Ok(Response::Error { message, .. })) => Err(MxrError::Ipc(message)),
                        Ok(Ok(_)) => {
                            Err(MxrError::Ipc("unexpected response to GetThreadGist".into()))
                        }
                        Ok(Err(e)) => Err(e),
                        Err(_) => Err(MxrError::Ipc("gist request timed out".into())),
                    };
                    let _ = result_tx.send(AsyncResult::ThreadGistLoaded { thread_id, result });
                });
            }
        }

        // Gists for the rows around the cursor on the desk and triage
        // lists; the daemon answers from its cache and queues the rest.
        let now = std::time::Instant::now();
        if app.row_gists.due(now) {
            let candidates = app.row_gist_candidates();
            if let Some(thread_ids) = app.row_gists.take_request(candidates, now) {
                let epoch = app.row_gists.epoch();
                let session = session.clone();
                let result_tx = result_tx.clone();
                tokio::spawn(async move {
                    let resp = ipc_call_dedicated(
                        &session,
                        Request::GetThreadGists {
                            thread_ids: thread_ids.clone(),
                            generate: true,
                            automated: false,
                        },
                    )
                    .await;
                    let result = match resp {
                        Ok(Response::Ok {
                            data: ResponseData::ThreadGists { batch },
                        }) => Ok(Box::new(batch)),
                        Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                        Ok(_) => Err(MxrError::Ipc(
                            "unexpected response to GetThreadGists".into(),
                        )),
                        Err(e) => Err(e),
                    };
                    let _ = result_tx.send(AsyncResult::RowGistsLoaded {
                        thread_ids,
                        epoch,
                        result,
                    });
                });
            }
        }

        if let Some(rule) = app.rules.pending_detail.take() {
            app.rules.detail_request_id = app.rules.detail_request_id.wrapping_add(1);
            let _ = replaceable.send(ReplaceableRequest::RuleDetail {
                rule,
                request_id: app.rules.detail_request_id,
                enqueued_at: Instant::now(),
            });
        }

        if let Some(rule) = app.rules.pending_history.take() {
            app.rules.history_request_id = app.rules.history_request_id.wrapping_add(1);
            let _ = replaceable.send(ReplaceableRequest::RuleHistory {
                rule,
                request_id: app.rules.history_request_id,
                enqueued_at: Instant::now(),
            });
        }

        if let Some((form, delete)) = app.rules.pending_category_preview.take() {
            let bg = bg.clone();
            app.rules.dry_run_request_id = app.rules.dry_run_request_id.wrapping_add(1);
            let request_id = app.rules.dry_run_request_id;
            let _ = submit_task(&queued, async move {
                let result = match ipc_call(
                    &bg,
                    Request::RuleCategories {
                        form: form.clone(),
                        preview_token: None,
                        delete,
                    },
                )
                .await
                {
                    Ok(Response::Ok {
                        data: ResponseData::RuleCategoriesResult { preview },
                    }) => category_preview_json(preview, form).map(|p| vec![p]),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("Unexpected category preview response".into())),
                };
                AsyncResult::RuleDryRun { request_id, result }
            });
        }
        if let Some((form, token, delete)) = app.rules.pending_category_apply.take() {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let result = match ipc_call(
                    &bg,
                    Request::RuleCategories {
                        form: form.clone(),
                        preview_token: Some(token),
                        delete,
                    },
                )
                .await
                {
                    Ok(Response::Ok {
                        data: ResponseData::RuleCategoriesResult { preview },
                    }) => category_preview_json(preview, form),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("Unexpected category apply response".into())),
                };
                AsyncResult::RuleTreatment(result)
            });
        }
        if let Some(form) = app.rules.pending_sorting_preview.take() {
            let bg = bg.clone();
            app.rules.dry_run_request_id = app.rules.dry_run_request_id.wrapping_add(1);
            let request_id = app.rules.dry_run_request_id;
            let _ = submit_task(&queued, async move {
                let result = match ipc_call(
                    &bg,
                    Request::RuleTreatment {
                        form: form.clone(),
                        preview_token: None,
                    },
                )
                .await
                {
                    Ok(Response::Ok {
                        data: ResponseData::RuleTreatmentResult { preview },
                    }) => match serde_json::to_value(preview) {
                        Ok(mut preview) => match serde_json::to_value(form) {
                            Ok(form) => {
                                preview["form"] = form;
                                Ok(vec![preview])
                            }
                            Err(e) => Err(MxrError::Ipc(e.to_string())),
                        },
                        Err(e) => Err(MxrError::Ipc(e.to_string())),
                    },
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("Unexpected sorting preview response".into())),
                };
                AsyncResult::RuleDryRun { request_id, result }
            });
        }
        if let Some(rule) = app.rules.pending_dry_run.take() {
            let bg = bg.clone();
            app.rules.dry_run_request_id = app.rules.dry_run_request_id.wrapping_add(1);
            let request_id = app.rules.dry_run_request_id;
            let _ = submit_task(&queued, async move {
                let result = async {
                    let form_response =
                        ipc_call(&bg, Request::GetRuleForm { rule: rule.clone() }).await?;
                    if let Response::Ok {
                        data: ResponseData::RuleFormData { form },
                    } = form_response
                    {
                        if form.action.split([',', ';']).any(|v| {
                            v.trim().starts_with("add-category:")
                                || v.trim().starts_with("remove-category:")
                        }) {
                            return match ipc_call(
                                &bg,
                                Request::RuleCategories {
                                    form: form.clone(),
                                    preview_token: None,
                                    delete: false,
                                },
                            )
                            .await?
                            {
                                Response::Ok {
                                    data: ResponseData::RuleCategoriesResult { preview },
                                } => category_preview_json(preview, form).map(|p| vec![p]),
                                Response::Error { message, .. } => Err(MxrError::Ipc(message)),
                                _ => Err(MxrError::Ipc(
                                    "Unexpected category preview response".into(),
                                )),
                            };
                        }
                        if form.action.contains("treatment:") {
                            return match ipc_call(
                                &bg,
                                Request::RuleTreatment {
                                    form: form.clone(),
                                    preview_token: None,
                                },
                            )
                            .await?
                            {
                                Response::Ok {
                                    data: ResponseData::RuleTreatmentResult { preview },
                                } => {
                                    let mut preview = serde_json::to_value(preview)
                                        .map_err(|e| MxrError::Ipc(e.to_string()))?;
                                    preview["form"] = serde_json::to_value(form)
                                        .map_err(|e| MxrError::Ipc(e.to_string()))?;
                                    Ok(vec![preview])
                                }
                                Response::Error { message, .. } => Err(MxrError::Ipc(message)),
                                _ => {
                                    Err(MxrError::Ipc("Unexpected sorting preview response".into()))
                                }
                            };
                        }
                    }
                    match ipc_call(
                        &bg,
                        Request::DryRunRules {
                            rule: Some(rule),
                            all: false,
                            after: None,
                        },
                    )
                    .await?
                    {
                        Response::Ok {
                            data: ResponseData::RuleDryRun { results },
                        } => Ok(results),
                        Response::Error { message, .. } => Err(MxrError::Ipc(message)),
                        _ => Err(MxrError::Ipc("Unexpected dry run response".into())),
                    }
                }
                .await;
                AsyncResult::RuleDryRun { request_id, result }
            });
        }
        if let Some((form, token)) = app.rules.pending_treatment.take() {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let result = match ipc_call(
                    &bg,
                    Request::RuleTreatment {
                        form,
                        preview_token: Some(token),
                    },
                )
                .await
                {
                    Ok(Response::Ok {
                        data: ResponseData::RuleTreatmentResult { preview },
                    }) => serde_json::to_value(preview).map_err(|e| MxrError::Ipc(e.to_string())),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("Unexpected sorting apply response".into())),
                };
                AsyncResult::RuleTreatment(result)
            });
        }

        if let Some(rule) = app.rules.pending_form_load.take() {
            app.rules.form_request_id = app.rules.form_request_id.wrapping_add(1);
            let _ = replaceable.send(ReplaceableRequest::RuleForm {
                rule,
                request_id: app.rules.form_request_id,
                enqueued_at: Instant::now(),
            });
        }

        if let Some(rule) = app.rules.pending_delete.take() {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let resp = ipc_call(&bg, Request::DeleteRule { rule }).await;
                let result = match resp {
                    Ok(Response::Ok { .. }) => Ok(()),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                };
                AsyncResult::RuleDeleted(result)
            });
        }

        if let Some(rule) = app.rules.pending_upsert.take() {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let resp = ipc_call(&bg, Request::UpsertRule { rule }).await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::RuleData { rule },
                    }) => Ok(rule),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response to UpsertRule".into())),
                };
                AsyncResult::RuleUpsert(result)
            });
        }

        if app.rules.pending_form_save {
            app.rules.pending_form_save = false;
            let bg = bg.clone();
            let account_id = app.rules.page.form.account_id.clone();
            let existing_rule = app.rules.page.form.existing_rule.clone();
            let name = app.rules.page.form.name.clone();
            let condition = app.rules.page.form.condition.clone();
            let action = app.rules.page.form.action.clone();
            let priority = app.rules.page.form.priority.parse::<i32>().unwrap_or(100);
            let enabled = app.rules.page.form.enabled;
            let _ = submit_task(&queued, async move {
                let resp = ipc_call(
                    &bg,
                    Request::UpsertRuleForm {
                        account_id,
                        existing_rule,
                        name,
                        condition,
                        action,
                        priority,
                        enabled,
                    },
                )
                .await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::RuleData { rule },
                    }) => Ok(rule),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc(
                        "unexpected response to UpsertRuleForm".into(),
                    )),
                };
                AsyncResult::RuleUpsert(result)
            });
        }

        // Drain any queued saved-search requests (Create / Delete). The edit
        // path queues `[Delete, Create]` so we dispatch them in order; the
        // last response triggers a list refresh.
        let saved_search_dispatch = app.take_pending_saved_search_dispatch();
        if !saved_search_dispatch.is_empty() {
            let total = saved_search_dispatch.len();
            for (index, request) in saved_search_dispatch.into_iter().enumerate() {
                let bg = bg.clone();
                let is_last = index + 1 == total;
                let _ = submit_task(&queued, async move {
                    let resp = ipc_call(&bg, request).await;
                    let result: Result<(), MxrError> = match resp {
                        Ok(Response::Ok { .. }) => Ok(()),
                        Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                        Err(e) => Err(e),
                    };
                    let _ = is_last; // last-flag is observed by the response handler.
                    AsyncResult::SavedSearchMutation(result)
                });
            }
            app.modals.pending_saved_search_refresh = true;
        }

        // Refresh the sidebar list once a saved-search mutation completes.
        // Triggered either by the dispatch above or by the response handler
        // when it observes `pending_saved_search_refresh`.
        if app.modals.pending_saved_search_refresh
            && app.modals.pending_saved_search_dispatch.is_empty()
        {
            // Don't refire if a refresh task is already in flight: clear
            // the flag here so the response handler can re-set it on the
            // *next* mutation. Avoids spamming ListSavedSearches every
            // tick while a save is in-flight.
            app.modals.pending_saved_search_refresh = false;
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let resp = ipc_call(&bg, Request::ListSavedSearches).await;
                let result: Result<Vec<mxr_core::types::SavedSearch>, MxrError> = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::SavedSearches { searches },
                    }) => Ok(searches),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc(
                        "unexpected response to ListSavedSearches".into(),
                    )),
                };
                AsyncResult::SavedSearchListRefreshed(result)
            });
        }

        let category_requests = std::mem::take(&mut app.pending_category_requests);
        for (generation, request) in category_requests {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let report_if_stale = matches!(
                    &request,
                    Request::UndoCategoryMembership { .. }
                        | Request::ApplyCategoryMembership { .. }
                );
                let view_category_id = match &request {
                    Request::ListCategoryMessages { category_id, .. } => Some(category_id.clone()),
                    _ => None,
                };
                let result = match ipc_call(&bg, request).await {
                    Ok(Response::Ok { data }) => Ok(Box::new(data)),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(error) => Err(error),
                };
                AsyncResult::PersonalCategory {
                    generation,
                    report_if_stale,
                    view_category_id,
                    result,
                }
            });
        }

        // Drain queued semantic-runtime requests (Enable / Disable / Reindex
        // / InstallProfile). Each is a one-shot; result routes through the
        // shared error reporter so failures aren't swallowed.
        let semantic_dispatch = app.take_pending_semantic_dispatch();
        for request in semantic_dispatch {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let resp = ipc_call(&bg, request).await;
                let result: Result<(), MxrError> = match resp {
                    Ok(Response::Ok { .. }) => Ok(()),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                };
                AsyncResult::SemanticOperationResult(result)
            });
        }

        if let Some(message_ids) = app.pending_archive_preview.take() {
            let session = session.clone();
            let _ = submit_task(&queued, async move {
                let result = async {
                    let owner = session.owner.as_ref().ok_or_else(|| MxrError::Ipc("Archive preview has no original workspace owner; reopen the client before continuing".into()))?;
                    let mut connection = session.target.connect(mxr_protocol::ClientKind::Tui).await.map_err(|error| MxrError::Ipc(error.to_string()))?;
                    let journal = mxr_client::ArchiveJournal::new(&mxr_config::data_dir())
                        .map_err(|error| MxrError::Ipc(error.to_string()))?;
                    let operation = connection
                        .preview_archive_owned(
                            &journal,
                            mxr_protocol::MutationCommand::Archive { message_ids },
                            owner,
                        )
                        .await
                        .map_err(|error| MxrError::Ipc(error.to_string()))?;
                    Ok(format_platform_response(&ResponseData::ArchiveOperation {
                        operation,
                    }))
                }
                .await;
                AsyncResult::PlatformModalLoaded {
                    compose: None,
                    title: "Archive preview".into(),
                    result,
                }
            });
        }

        let platform_dispatch = app.take_pending_platform_dispatch();
        for pending in platform_dispatch {
            let bg = bg.clone();
            let title = pending.title.clone();
            let compose_assist = pending.compose_assist.clone();
            let compose_model = match &pending.request {
                Request::ComposeAssist { model, .. } => model.clone(),
                _ => None,
            };
            let compose_assist_request_id = pending.compose_assist_request_id.unwrap_or_default();
            let compose_model_picker = pending.compose_model_picker.clone();
            let compose_assist_accept = pending.compose_assist_accept.clone();
            let compose_assist_reopen_editor = pending.compose_assist_reopen_editor;
            let result_tx_inner = result_tx.clone();
            let _ = submit_task(&queued, async move {
                let mut pending = pending;
                let compose = pending.compose.clone();
                let result = async {
                    if let Some((account, identity, draft_id)) =
                        crate::compose_persistence::prepare_compose_schedule(&bg, &mut pending)
                            .await?
                    {
                        let _ = result_tx_inner.send(AsyncResult::ScheduledIdentityRetained(
                            account, identity, draft_id,
                        ));
                    }
                    for request in pending.prelude.drain(..) {
                        match ipc_call(&bg, request).await {
                            Ok(Response::Ok { .. }) => {}
                            Ok(Response::Error { message, .. }) => {
                                return Err(MxrError::Ipc(message));
                            }
                            Err(e) => {
                                return Err(e);
                            }
                        }
                    }
                    ipc_call(&bg, pending.request).await
                }
                .await;
                if let Some((pending, purpose)) = compose_model_picker {
                    let result = match result {
                        Ok(Response::Ok {
                            data: ResponseData::ComposeModels { models },
                        }) => Ok(models),
                        Ok(Response::Ok { .. }) => Err(MxrError::Ipc(
                            "The daemon returned no compose model catalogue".into(),
                        )),
                        Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                        Err(error) => Err(error),
                    };
                    return AsyncResult::ComposeModelsLoaded {
                        pending,
                        purpose,
                        result,
                    };
                }
                if let Some(compose) = compose_assist {
                    let proposal = match result {
                        Ok(Response::Ok {
                            data: ResponseData::ComposeProposal { proposal },
                        }) => Ok(proposal),
                        Ok(Response::Ok { .. }) => Err(MxrError::Ipc(
                            "The daemon returned no writing proposal".into(),
                        )),
                        Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                        Err(error) => Err(error),
                    };
                    return AsyncResult::ComposeAssistLoaded {
                        pending: compose,
                        model: compose_model,
                        request_id: compose_assist_request_id,
                        result: proposal,
                    };
                }
                if let Some(compose) = compose_assist_accept {
                    let accepted = match result {
                        Ok(Response::Ok {
                            data: ResponseData::Draft { draft },
                        }) => {
                            let context = mxr_protocol::DraftSafetyContextData {
                                mode: mxr_protocol::DraftSafetyModeData::Check,
                                reply_all: matches!(
                                    compose.intent,
                                    mxr_core::DraftIntent::ReplyAll
                                ),
                                original_message_id: None,
                                thread_id: compose
                                    .fm
                                    .thread_id
                                    .as_ref()
                                    .and_then(|id| id.parse().ok()),
                                allow_llm: true,
                                proposed_send_at: Some(chrono::Utc::now()),
                            };
                            let safety = match ipc_call(
                                &bg,
                                Request::CheckDraftSafety {
                                    draft: draft.clone(),
                                    context,
                                },
                            )
                            .await
                            {
                                Ok(Response::Ok {
                                    data: ResponseData::DraftSafetyReportResponse { report },
                                }) => Ok(report),
                                Ok(Response::Ok { .. }) => Err(MxrError::Ipc(
                                    "The daemon returned no safety report for the accepted draft"
                                        .into(),
                                )),
                                Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                                Err(error) => Err(error),
                            };
                            Ok((draft, safety))
                        }
                        Ok(Response::Ok { .. }) => Err(MxrError::Ipc(
                            "The daemon returned no accepted draft".into(),
                        )),
                        Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                        Err(error) => Err(error),
                    };
                    return AsyncResult::ComposeAssistAccepted {
                        pending: compose,
                        request_id: compose_assist_request_id,
                        reopen_editor: compose_assist_reopen_editor,
                        result: accepted,
                    };
                }
                let result = result.and_then(|response| match response {
                    Response::Ok { data } => Ok(format_platform_response(&data)),
                    Response::Error { message, .. } => Err(MxrError::Ipc(message)),
                });
                AsyncResult::PlatformModalLoaded {
                    compose,
                    title,
                    result,
                }
            });
        }

        if app.diagnostics.page.refresh_pending {
            app.diagnostics.page.refresh_pending = false;
            app.diagnostics.pending_status_refresh = false;
            app.diagnostics.page.pending_requests = 6;
            app.diagnostics.request_id = app.diagnostics.request_id.wrapping_add(1);
            let request_id = app.diagnostics.request_id;
            for (kind, request) in [
                (ReplaceableRequestKey::DiagnosticsStatus, Request::GetStatus),
                (
                    ReplaceableRequestKey::DiagnosticsDoctor,
                    Request::GetDoctorReport,
                ),
                (
                    ReplaceableRequestKey::DiagnosticsEvents,
                    Request::ListEvents {
                        limit: 20,
                        level: None,
                        category: None,
                        since: None,
                        until: None,
                        search: None,
                        category_prefix: None,
                        offset: 0,
                    },
                ),
                (
                    ReplaceableRequestKey::DiagnosticsLogs,
                    Request::GetLogs {
                        limit: 200,
                        level: None,
                        search: None,
                    },
                ),
                (ReplaceableRequestKey::DiagnosticsJobs, Request::ListJobs),
                (
                    ReplaceableRequestKey::DiagnosticsActivity,
                    Request::ListActivity {
                        filter: mxr_protocol::ActivityFilter {
                            since: Some(chrono::Utc::now().timestamp_millis() - 86_400_000),
                            ..Default::default()
                        },
                        limit: 200,
                        cursor: None,
                    },
                ),
            ] {
                let _ = replaceable.send(ReplaceableRequest::Diagnostics {
                    kind,
                    request: Box::new(request),
                    request_id,
                    enqueued_at: Instant::now(),
                });
            }
        }

        if app.diagnostics.pending_status_refresh {
            app.diagnostics.pending_status_refresh = false;
            app.diagnostics.status_request_id = app.diagnostics.status_request_id.wrapping_add(1);
            let _ = replaceable.send(ReplaceableRequest::Status {
                request_id: app.diagnostics.status_request_id,
                enqueued_at: Instant::now(),
            });
        }

        if let Some((request, owner)) = app.diagnostics.pending_recovery_request.take() {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let result = ipc_call_owned(&bg, request.clone(), Some(owner.clone())).await;
                AsyncResult::Recovery {
                    owner,
                    request,
                    result: Box::new(result),
                }
            });
        }

        if app.accounts.page.refresh_pending {
            app.accounts.page.refresh_pending = false;
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let result = load_accounts_page_accounts(&bg).await;
                AsyncResult::Accounts(result)
            });
        }

        if app.mailbox.pending_labels_refresh {
            app.mailbox.pending_labels_refresh = false;
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let resp = ipc_call(&bg, Request::ListLabels { account_id: None }).await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::Labels { labels },
                    }) => Ok(labels),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response to ListLabels".into())),
                };
                AsyncResult::Labels(result)
            });
        }

        if app.mailbox.pending_all_envelopes_refresh {
            app.mailbox.pending_all_envelopes_refresh = false;
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let resp = ipc_call(
                    &bg,
                    Request::ListEnvelopes {
                        label_id: None,
                        account_id: None,
                        limit: 5000,
                        offset: 0,
                    },
                )
                .await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::Envelopes { envelopes },
                    }) => Ok(envelopes),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response to ListEnvelopes".into())),
                };
                AsyncResult::AllEnvelopes(result)
            });
        }

        if app.mailbox.pending_subscriptions_refresh {
            app.mailbox.pending_subscriptions_refresh = false;
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let resp = ipc_call(
                    &bg,
                    Request::ListSubscriptions {
                        account_id: None,
                        limit: 500,
                    },
                )
                .await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::Subscriptions { subscriptions },
                    }) => Ok(subscriptions),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc(
                        "unexpected response to ListSubscriptions".into(),
                    )),
                };
                AsyncResult::Subscriptions(result)
            });
        }

        if let Some(query) = app.pending_expert_query.take() {
            let bg = bg.clone();
            let account_id = app.default_account_id().cloned();
            let _ = submit_task(&queued, async move {
                let Some(account_id) = account_id else {
                    return AsyncResult::Expert(Err(MxrError::Ipc("no default account".into())));
                };
                let resp = ipc_call(
                    &bg,
                    Request::FindExpert {
                        account_id,
                        query,
                        include_self: false,
                        limit: 5,
                    },
                )
                .await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::ExpertSuggestions { experts },
                    }) => Ok(experts),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response to FindExpert".into())),
                };
                AsyncResult::Expert(result)
            });
        }

        if let Some(query) = app.pending_whois_query.take() {
            let bg = bg.clone();
            let account_id = app.default_account_id().cloned();
            let _ = submit_task(&queued, async move {
                let Some(account_id) = account_id else {
                    return AsyncResult::Whois(Err(MxrError::Ipc("no default account".into())));
                };
                let resp = ipc_call(
                    &bg,
                    Request::ExplainEntity {
                        account_id,
                        query,
                        limit: 10,
                    },
                )
                .await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::EntityExplanation { entity },
                    }) => Ok(entity),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response to ExplainEntity".into())),
                };
                AsyncResult::Whois(result)
            });
        }

        if let Some(briefing_req) = app.pending_briefing_request.take() {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let request = match briefing_req {
                    crate::app::BriefingRequest::Thread(thread_id) => Request::GetThreadBriefing {
                        thread_id,
                        refresh: false,
                    },
                    crate::app::BriefingRequest::Recipient { email } => {
                        // Account id resolved daemon-side; the briefing
                        // handler uses the supplied id verbatim. We use
                        // the default account on the client side.
                        // (For now, leave account_id construction up to
                        // the daemon by passing a fresh AccountId — the
                        // handler validates against the contacts table.)
                        let account_id = mxr_core::AccountId::new();
                        Request::GetRecipientBriefing {
                            account_id,
                            email,
                            refresh: false,
                        }
                    }
                };
                let resp = ipc_call(&bg, request).await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::ThreadBriefing { briefing },
                    }) => Ok(briefing),
                    Ok(Response::Ok {
                        data: ResponseData::RecipientBriefing { briefing },
                    }) => Ok(briefing),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc(
                        "unexpected response to GetRecipientBriefing".into(),
                    )),
                };
                AsyncResult::Briefing(result)
            });
        }

        if app.mailbox.pending_owed_refresh {
            app.mailbox.pending_owed_refresh = false;
            let bg = bg.clone();
            let account_id = app.default_account_id().cloned();
            let _ = submit_task(&queued, async move {
                let Some(account_id) = account_id else {
                    return AsyncResult::OwedReplies(Ok((Vec::new(), None)));
                };
                let resp = ipc_call(
                    &bg,
                    Request::ListOwedReplies {
                        account_id,
                        older_than_days: None,
                        within_days: None,
                        limit: 100,
                        all: false,
                    },
                )
                .await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::OwedReplies { rows, people },
                    }) => Ok((rows, people)),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc(
                        "unexpected response to ListOwedReplies".into(),
                    )),
                };
                AsyncResult::OwedReplies(result)
            });
        }

        if app.mailbox.pending_desk_refresh {
            app.mailbox.pending_desk_refresh = false;
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let resp = ipc_call(&bg, desk_request()).await;
                let result = match resp {
                    Ok(Response::Ok { data }) => crate::app::DeskPageState::from_response(data)
                        .ok_or_else(|| MxrError::Ipc("unexpected response to GetDesk".into())),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                };
                AsyncResult::Desk(result)
            });
        }

        if std::mem::take(&mut app.mailbox.now_page.pending_refresh) {
            let now_bg = bg.clone();
            let _ = submit_task(&queued, async move {
                AsyncResult::Now(fetch_now(&now_bg).await.map(Box::new))
            });
            // The arrivals line beside it; only opening Now starts a visit.
            let mark_seen = std::mem::take(&mut app.mailbox.now_page.pending_mark_seen);
            // Refreshes while Now stays open keep the window the visit opened with.
            let open_since = app.mailbox.now_page.arrivals.as_ref().map(|a| a.since);
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                AsyncResult::Arrivals(
                    trust_call(
                        &bg,
                        Request::GetArrivals {
                            account_id: None,
                            mark_seen,
                            since: open_since,
                        },
                        |data| match data {
                            ResponseData::Arrivals { arrivals } => Some(Box::new(arrivals)),
                            _ => None,
                        },
                    )
                    .await,
                )
            });
        }

        spawn_trust_fetches(&mut app, &bg, &queued);
        crate::assistant_ipc::spawn_assistant_requests(&mut app, &bg, &assistant_io);

        if std::mem::take(&mut app.mailbox.messages_page.pending_refresh) {
            let bg = bg.clone();
            let filter = app.mailbox.messages_page.filter;
            let _ = submit_task(&queued, async move {
                AsyncResult::Messages(fetch_messages(&bg, filter).await.map(Box::new))
            });
        }

        if let Some((row_id, topic)) = app.mailbox.messages_page.pending_person.take() {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let resp = ipc_call(
                    &bg,
                    Request::GetPerson {
                        account_id: None,
                        person: row_id.clone(),
                        topic: topic.clone(),
                        conversation: true,
                    },
                )
                .await;
                AsyncResult::PersonPage(
                    row_id,
                    topic,
                    match resp {
                        Ok(Response::Ok {
                            data: ResponseData::PersonPage { page },
                        }) => Ok(Box::new(page)),
                        Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                        Err(e) => Err(e),
                        _ => Err(MxrError::Ipc("unexpected response to GetPerson".into())),
                    },
                )
            });
        }

        if let Some(thread_id) = app.mailbox.messages_page.pending_ack_preview.take() {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let resp = ipc_call(
                    &bg,
                    Request::AckMessage {
                        thread_id,
                        dry_run: true,
                        expect_text: None,
                        preview_token: None,
                    },
                )
                .await;
                AsyncResult::MessagesAck(match resp {
                    Ok(Response::Ok {
                        data: ResponseData::MessagesAck { ack },
                    }) => Ok(Box::new(ack)),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response to AckMessage".into())),
                })
            });
        }

        spawn_reading_fetches(&mut app, &bg, &queued);

        if std::mem::take(&mut app.mailbox.pending_rail_refresh) {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let resp = ipc_call(&bg, Request::GetRail { account_id: None }).await;
                AsyncResult::Rail(match resp {
                    Ok(Response::Ok {
                        data: ResponseData::Rail { rail },
                    }) => Ok(rail),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response to GetRail".into())),
                })
            });
        }

        if std::mem::take(&mut app.mailbox.now_page.pending_digest_preview) {
            let cut = app
                .mailbox
                .now_page
                .now
                .as_ref()
                .and_then(|now| now.updates.as_ref())
                .map(|card| card.since);
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                AsyncResult::NowDigestPreview(preview_let_go(&bg, cut, None).await)
            });
        }

        if std::mem::take(&mut app.mailbox.updates_page.pending_refresh) {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                AsyncResult::Updates(fetch_updates(&bg).await.map(Box::new))
            });
        }

        if std::mem::take(&mut app.mailbox.updates_page.pending_latest_code) {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                AsyncResult::LatestCodes(
                    match ipc_call(
                        &bg,
                        Request::ListOneTimeCodes {
                            account_id: None,
                            limit: Some(5),
                        },
                    )
                    .await
                    {
                        Ok(Response::Ok {
                            data: ResponseData::OneTimeCodes { codes },
                        }) => Ok(codes),
                        Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                        Ok(_) => Err(MxrError::Ipc(
                            "unexpected response to ListOneTimeCodes".into(),
                        )),
                        Err(error) => Err(error),
                    },
                )
            });
        }

        if std::mem::take(&mut app.mailbox.updates_page.pending_let_go_preview) {
            let cut = app
                .mailbox
                .updates_page
                .digest
                .as_ref()
                .map(|digest| digest.cut.at);
            let source = app.mailbox.updates_page.let_go_source.clone();
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                AsyncResult::UpdatesLetGoPreview(preview_let_go(&bg, cut, source).await)
            });
        }

        if std::mem::take(&mut app.mailbox.todo_page.pending_refresh) {
            let mark_seen = std::mem::take(&mut app.mailbox.todo_page.pending_mark_seen);
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                AsyncResult::TodoRunway(fetch_todo_runway(&bg, mark_seen).await)
            });
        }

        spawn_records_fetches(&mut app, &bg, &queued);

        if let Some(request) = app.mailbox.todo_page.pending_microsoft.take() {
            let bg = bg.clone();
            if submit_task(&queued, async move {
                AsyncResult::MicrosoftTodo(match ipc_call(&bg, request).await {
                    Ok(Response::Ok { data }) => Ok(Box::new(data)),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(error) => Err(error),
                })
            })
            .is_err()
            {
                app.mailbox.todo_page.microsoft_busy = false;
                app.status_message = Some(
                    "Could not queue the Microsoft To Do request; reopen To do to try again."
                        .into(),
                );
            }
        }

        if let Some(list) = app.mailbox.todo_page.pending_list.take() {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                match list {
                    crate::app::TodoListFetch::Expired => {
                        let resp = ipc_call(
                            &bg,
                            Request::ListTodos {
                                account_id: None,
                                state: mxr_protocol::TodoStateData::Expired,
                                limit: 200,
                            },
                        )
                        .await;
                        AsyncResult::TodoExpired(match resp {
                            Ok(Response::Ok {
                                data: ResponseData::Todos { todos },
                            }) => Ok(todos),
                            Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                            Err(e) => Err(e),
                            _ => Err(MxrError::Ipc("unexpected response to ListTodos".into())),
                        })
                    }
                    crate::app::TodoListFetch::Catchup => {
                        let resp =
                            ipc_call(&bg, Request::GetTodoCatchup { account_id: None }).await;
                        AsyncResult::TodoCatchup(match resp {
                            Ok(Response::Ok {
                                data: ResponseData::TodoCatchup { catchup },
                            }) => Ok(catchup),
                            Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                            Err(e) => Err(e),
                            _ => Err(MxrError::Ipc(
                                "unexpected response to GetTodoCatchup".into(),
                            )),
                        })
                    }
                }
            });
        }

        if std::mem::take(&mut app.mailbox.todo_page.pending_catchup_preview) {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let resp = ipc_call(
                    &bg,
                    Request::SetTodoCatchup {
                        account_id: None,
                        decision: mxr_protocol::TodoCatchupDecisionData::LetGoAll,
                        dry_run: true,
                    },
                )
                .await;
                AsyncResult::TodoCatchupPreview(match resp {
                    Ok(Response::Ok {
                        data: ResponseData::TodoChange { change },
                    }) => Ok(change),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc(
                        "unexpected response to SetTodoCatchup".into(),
                    )),
                })
            });
        }

        if let Some((todo_id, suggestion_id, _)) = app.mailbox.todo_page.pending_resolution.take() {
            let bg = bg.clone();
            let task_id = todo_id.clone();
            let source_id = suggestion_id.clone();
            let _ = submit_task(&queued, async move {
                let response = ipc_call(
                    &bg,
                    Request::ResolveTodoSuggestion {
                        todo_id,
                        suggestion_id,
                        action: mxr_protocol::TodoResolutionActionData::Preview,
                        dry_run: true,
                    },
                )
                .await;
                AsyncResult::TodoResolutionPreview(
                    task_id,
                    source_id,
                    match response {
                        Ok(Response::Ok {
                            data: ResponseData::TodoChange { change },
                        }) => Ok(change),
                        Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                        Err(error) => Err(error),
                        _ => Err(MxrError::Ipc(
                            "unexpected response to resolution preview".into(),
                        )),
                    },
                )
            });
        }

        let place_fetches = app
            .mailbox
            .pending_place_refresh
            .take()
            .map(|place| app.mailbox.place_page.refresh_fetch(place))
            .into_iter()
            .chain(app.mailbox.pending_place_more.take());
        for fetch in place_fetches {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let resp = ipc_call(&bg, place_request(&fetch)).await;
                let result = match resp {
                    Ok(Response::Ok { data }) => crate::app::PlacePageState::from_response(data)
                        .ok_or_else(|| MxrError::Ipc("unexpected response to ListPlace".into())),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                };
                AsyncResult::Place(fetch, result)
            });
        }

        if let Some(target) = app.mailbox.pending_sweep_preview.take() {
            let from_updates = app.mailbox.mailbox_view == crate::app::MailboxView::Updates;
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let resp = ipc_call(&bg, sweep_request(&target, true, None)).await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::PlaceSwept { preview, .. },
                    }) => Ok(crate::app::PendingSweepConfirm {
                        target,
                        preview,
                        shown: 0,
                        sweep_focused: false,
                        from_updates,
                    }),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Ok(_) => Err(MxrError::Ipc("unexpected response to SweepPlace".into())),
                    Err(e) => Err(e),
                };
                AsyncResult::SweepPreview(result)
            });
        }

        if let Some(confirm) = app.mailbox.pending_sweep.take() {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                AsyncResult::PlaceSwept(run_sweep(&bg, confirm).await)
            });
        }

        if app.mailbox.pending_calendar_invites_refresh {
            app.mailbox.pending_calendar_invites_refresh = false;
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let resp = ipc_call(
                    &bg,
                    Request::ListInvites {
                        account_id: None,
                        limit: 200,
                    },
                )
                .await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::Invites { invites },
                    }) => Ok(invites),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response to ListInvites".into())),
                };
                AsyncResult::CalendarInvites(result)
            });
        }

        if let Some(open) = app.mailbox.todo_page.pending_open.take() {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let resp = ipc_call(
                    &bg,
                    Request::GetEnvelope {
                        message_id: open.message_id,
                    },
                )
                .await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::Envelope { envelope },
                    }) => Ok(envelope),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response to GetEnvelope".into())),
                };
                AsyncResult::TodoEnvelopeOpened(result, open.link)
            });
        }

        if let Some(message_id) = app.mailbox.pending_invite_open.take() {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let resp = ipc_call(&bg, Request::GetEnvelope { message_id }).await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::Envelope { envelope },
                    }) => Ok(envelope),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response to GetEnvelope".into())),
                };
                AsyncResult::InviteEnvelopeOpened(result)
            });
        }

        if app.mailbox.pending_commitment_counts_refresh {
            app.mailbox.pending_commitment_counts_refresh = false;
            let bg = bg.clone();
            let envelopes = app
                .mailbox
                .all_envelopes
                .iter()
                .chain(app.mailbox.envelopes.iter())
                .chain(app.search.page.results.iter())
                .cloned()
                .collect::<Vec<_>>();
            let _ = submit_task(&queued, async move {
                AsyncResult::CommitmentCounts(load_open_commitment_counts(&bg, envelopes).await)
            });
        }

        if let Some(account) = app.accounts.pending_save.take() {
            let is_outlook = matches!(
                account.sync,
                Some(
                    mxr_protocol::AccountSyncConfigData::OutlookPersonal { .. }
                        | mxr_protocol::AccountSyncConfigData::OutlookWork { .. }
                )
            );
            if is_outlook {
                app.accounts.pending_auth_session_account = Some(account.clone());
                app.accounts.page.operation_in_flight = true;
                let bg2 = bg.clone();
                let result_tx2 = result_tx.clone();
                tokio::spawn(async move {
                    spawn_outlook_auth_session(&bg2, result_tx2, account, false).await;
                });
            } else {
                let bg = bg.clone();
                let _ = submit_task(&queued, async move {
                    let result = run_account_save_workflow(&bg, account).await;
                    AsyncResult::AccountOperation(result)
                });
            }
        }

        if let Some(account) = app.accounts.pending_test.take() {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let result =
                    request_account_operation(&bg, Request::TestAccountConfig { account }).await;
                AsyncResult::AccountOperation(result)
            });
        }

        if let Some((account, reauthorize)) = app.accounts.pending_authorize.take() {
            let is_outlook = matches!(
                account.sync,
                Some(
                    mxr_protocol::AccountSyncConfigData::OutlookPersonal { .. }
                        | mxr_protocol::AccountSyncConfigData::OutlookWork { .. }
                )
            );
            if is_outlook {
                app.accounts.pending_auth_session_account = Some(account.clone());
                app.accounts.page.operation_in_flight = true;
                let bg2 = bg.clone();
                let result_tx2 = result_tx.clone();
                tokio::spawn(async move {
                    spawn_outlook_auth_session(&bg2, result_tx2, account, reauthorize).await;
                });
            } else {
                let bg = bg.clone();
                let _ = submit_task(&queued, async move {
                    let result = request_account_operation(
                        &bg,
                        Request::AuthorizeAccountConfig {
                            account,
                            reauthorize,
                        },
                    )
                    .await;
                    AsyncResult::AccountOperation(result)
                });
            }
        }

        if let Some(key) = app.accounts.pending_set_default.take() {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let result =
                    request_account_operation(&bg, Request::SetDefaultAccount { key }).await;
                AsyncResult::AccountOperation(result)
            });
        }

        if app.analytics.refresh_pending {
            app.analytics.refresh_pending = false;
            app.analytics.loading = true;
            app.analytics.error = None;
            let view = app.analytics.view;
            let Some(request) = app.analytics_request_for_active_view() else {
                app.analytics.loading = false;
                app.analytics.error = Some("Cadence drift needs an enabled account.".into());
                continue;
            };
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let resp = ipc_call(&bg, request).await;
                let result: Result<crate::async_result::AnalyticsResultPayload, MxrError> =
                    match resp {
                        Ok(Response::Ok {
                            data: ResponseData::StorageBreakdown { rows },
                        }) => Ok(crate::async_result::AnalyticsResultPayload::Storage(rows)),
                        Ok(Response::Ok {
                            data: ResponseData::LargestMessages { rows },
                        }) => {
                            Ok(crate::async_result::AnalyticsResultPayload::LargestMessages(rows))
                        }
                        Ok(Response::Ok {
                            data: ResponseData::StaleThreads { rows },
                        }) => Ok(crate::async_result::AnalyticsResultPayload::Stale(rows)),
                        Ok(Response::Ok {
                            data: ResponseData::ContactAsymmetry { rows },
                        }) => Ok(crate::async_result::AnalyticsResultPayload::Asymmetry(rows)),
                        Ok(Response::Ok {
                            data: ResponseData::ContactDecay { rows },
                        }) => Ok(crate::async_result::AnalyticsResultPayload::Decay(rows)),
                        Ok(Response::Ok {
                            data: ResponseData::CadenceDriftList { rows },
                        }) => Ok(crate::async_result::AnalyticsResultPayload::CadenceDrift(
                            rows,
                        )),
                        Ok(Response::Ok {
                            data: ResponseData::ResponseTime { summary },
                        }) => Ok(crate::async_result::AnalyticsResultPayload::ResponseTime(
                            summary,
                        )),
                        Ok(Response::Ok {
                            data: ResponseData::Subscriptions { subscriptions },
                        }) => Ok(crate::async_result::AnalyticsResultPayload::Subscriptions(
                            subscriptions,
                        )),
                        Ok(Response::Ok {
                            data: ResponseData::SearchAggregation { groups, .. },
                        }) => Ok(
                            crate::async_result::AnalyticsResultPayload::SearchAggregation(groups),
                        ),
                        Ok(Response::Ok {
                            data: ResponseData::Wrapped { summary },
                        }) => Ok(crate::async_result::AnalyticsResultPayload::Wrapped(
                            Box::new(summary),
                        )),
                        Ok(Response::Ok {
                            data: ResponseData::RefreshedContacts { rows },
                        }) => Ok(
                            crate::async_result::AnalyticsResultPayload::ContactsRefreshed { rows },
                        ),
                        Ok(Response::Ok { .. }) => {
                            Err(MxrError::Ipc("unexpected analytics response".into()))
                        }
                        Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                        Err(e) => Err(e),
                    };
                AsyncResult::AnalyticsResult {
                    view,
                    result: Box::new(result),
                }
            });
        }

        if app.analytics.pending_contacts_refresh {
            app.analytics.pending_contacts_refresh = false;
            let view = app.analytics.view;
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let resp = ipc_call(&bg, Request::RefreshContacts).await;
                let result: Result<crate::async_result::AnalyticsResultPayload, MxrError> =
                    match resp {
                        Ok(Response::Ok {
                            data: ResponseData::RefreshedContacts { rows },
                        }) => Ok(
                            crate::async_result::AnalyticsResultPayload::ContactsRefreshed { rows },
                        ),
                        Ok(Response::Ok { .. }) => {
                            Err(MxrError::Ipc("unexpected analytics response".into()))
                        }
                        Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                        Err(e) => Err(e),
                    };
                AsyncResult::AnalyticsResult {
                    view,
                    result: Box::new(result),
                }
            });
        }

        if let Some(account) = app.accounts.pending_repair.take() {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let result =
                    request_account_operation(&bg, Request::RepairAccountConfig { account }).await;
                AsyncResult::AccountOperation(result)
            });
        }

        if app.diagnostics.pending_bug_report {
            app.diagnostics.pending_bug_report = false;
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let resp = ipc_call(
                    &bg,
                    Request::GenerateBugReport {
                        verbose: false,
                        full_logs: false,
                        since: None,
                    },
                )
                .await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::BugReport { content },
                    }) => Ok(content),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc(
                        "unexpected response to GenerateBugReport".into(),
                    )),
                };
                AsyncResult::BugReport(result)
            });
        }

        if let Some(pending) = app.mailbox.pending_attachment_action.take() {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let destination = match pending.operation {
                    AttachmentOperation::Open => {
                        let dir = mxr_config::data_dir().join("attachments");
                        if let Err(error) = std::fs::create_dir_all(&dir) {
                            return AsyncResult::AttachmentFile {
                                operation: pending.operation,
                                result: Err(MxrError::Ipc(error.to_string())),
                            };
                        }
                        let safe_name = std::path::Path::new(&pending.filename)
                            .file_name()
                            .and_then(|name| name.to_str())
                            .filter(|name| !name.is_empty() && *name != "." && *name != "..")
                            .unwrap_or("attachment");
                        dir.join(format!("{}-{safe_name}", pending.attachment_id))
                    }
                    AttachmentOperation::Download => match pending.destination.clone() {
                        Some(path) => path,
                        None => {
                            return AsyncResult::AttachmentFile {
                                operation: pending.operation,
                                result: Err(MxrError::Ipc(
                                    "download destination is missing".into(),
                                )),
                            };
                        }
                    },
                };
                let response = ipc_transfer_attachment(
                    &bg,
                    AttachmentTransferRequest {
                        message_id: pending.message_id,
                        attachment_id: pending.attachment_id,
                        destination,
                        replace_existing: pending.operation == AttachmentOperation::Open
                            || pending.replace_existing,
                    },
                )
                .await;
                let result = match response {
                    Ok(Response::Ok {
                        data: ResponseData::AttachmentFile { file },
                    }) => {
                        if pending.operation == AttachmentOperation::Open {
                            open::that(&file.path).map(|()| file).map_err(|error| {
                                MxrError::Ipc(format!(
                                    "attachment downloaded but could not be opened: {error}"
                                ))
                            })
                        } else {
                            Ok(file)
                        }
                    }
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(error) => Err(error),
                    _ => Err(MxrError::Ipc(
                        "unexpected response to attachment transfer".into(),
                    )),
                };
                AsyncResult::AttachmentFile {
                    operation: pending.operation,
                    result,
                }
            });
        }

        // Spawn non-blocking label envelope fetch
        if let Some(label_id) = app.mailbox.pending_label_fetch.take() {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let resp = ipc_call(
                    &bg,
                    Request::ListEnvelopes {
                        label_id: Some(label_id),
                        account_id: None,
                        limit: 5000,
                        offset: 0,
                    },
                )
                .await;
                let envelopes = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::Envelopes { envelopes },
                    }) => Ok(envelopes),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response to ListEnvelopes".into())),
                };
                AsyncResult::LabelEnvelopes(envelopes)
            });
        }

        // Drain pending mutations
        let now = std::time::Instant::now();
        let mut deferred_mutations = Vec::new();
        for queued_mutation in app.pending_mutation_queue.drain(..) {
            if !queued_mutation.ready_to_run(now) {
                deferred_mutations.push(queued_mutation);
                continue;
            }
            let app::QueuedMutation {
                expected_workspace,
                id: mutation_id,
                request: req,
                effect,
                best_effort,
                attempts,
                run_after: _,
            } = queued_mutation;
            let retry = (attempts < app::TRANSIENT_MUTATION_MAX_RETRIES
                && mutation_can_auto_retry(&req))
            .then(|| app::QueuedMutation {
                expected_workspace: expected_workspace.clone(),
                id: mutation_id,
                request: req.clone(),
                effect: effect.clone(),
                best_effort,
                attempts,
                run_after: None,
            });
            let bg = bg.clone();
            let result_tx_inner = result_tx.clone();
            let compose_file = app
                .compose
                .pending_send_mutations
                .get(&mutation_id)
                .map(|(pending, _)| pending.draft_path.clone());
            let _ = submit_task(&queued, async move {
                crate::ipc::COMMAND_OWNER.scope(expected_workspace, async move {
                let is_sync_all_request = matches!(&req, Request::SyncAll);
                let mut sync_batch_received = false;
                let verb = mutation_verb_past(&req);
                let move_not_sure = matches!(
                    &req,
                    Request::MoveMessage { source: Some(source), .. } if source == "not_sure"
                );
                let undo_id = match &req {
                    Request::UndoMutation { mutation_id } => Some(mutation_id.clone()),
                    _ => None,
                };
                let resp = async {
                    let mut req = req;
                    if let Request::SendDraft {
                        draft,
                        send_identity: Some(identity),
                        ..
                    } = &mut req
                    {
                        let (reviewed_draft, reviewed) =
                            crate::send_recovery::prepare_reviewed_send(
                                &bg,
                                draft.clone(),
                                identity.clone(),
                            )
                            .await?;
                        *draft = reviewed_draft;
                        *identity = reviewed;
                        #[cfg(not(test))]
                        {
                            let retained = match compose_file.as_deref() {
                                Some(file) => crate::send_recovery_state::retain_reviewed_compose(
                                    &draft.account_id,
                                    &draft.id,
                                    identity,
                                    file,
                                ),
                                None => crate::send_recovery_state::retain_reviewed(
                                    &draft.account_id,
                                    &draft.id,
                                    identity,
                                ),
                            };
                            retained.map_err(|error| {
                                MxrError::Ipc(format!(
                                    "Cannot retain reviewed send identity: {error}"
                                ))
                            })?;
                        }
                        {
                            let _ = result_tx_inner.send(AsyncResult::SendIdentityRetained(
                                draft.account_id.clone(),
                                identity.clone(),
                            ));
                        }
                    }
                    if let (Some(file), Request::SaveDraftToServer { draft }) =
                        (compose_file.as_ref(), &mut req)
                    {
                        let (canonical, identity) =
                            crate::compose_persistence::prepare_reviewed_compose(
                                &bg,
                                draft.clone(),
                                file,
                            )
                            .await?;
                        *draft = canonical;
                        let _ = result_tx_inner.send(AsyncResult::SendIdentityRetained(
                            draft.account_id.clone(),
                            identity,
                        ));
                    }
                    if is_sync_all_request {
                        let protocol_version = match ipc_call(&bg, Request::GetStatus).await? {
                            Response::Ok {
                                data:
                                    ResponseData::Status {
                                        protocol_version, ..
                                    },
                            } => protocol_version,
                            Response::Error { message, .. } => {
                                return Err(MxrError::Ipc(message));
                            }
                            _ => {
                                return Err(MxrError::Ipc(
                                    "unexpected response while checking global sync support".into(),
                                ));
                            }
                        };
                        require_sync_all_protocol(protocol_version)?;
                    }
                    ipc_call(&bg, req).await
                }
                .await;
                let outcome = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::Ack | ResponseData::Draft { .. },
                    }) if !is_sync_all_request => Ok(effect),
                    Ok(Response::Ok {
                        data:
                            ResponseData::SendReceipt {
                                local_message_id, ..
                            },
                    }) => Ok({
                        let mut effect = effect;
                        if let app::MutationEffect::SentSuccess {
                            sent_message_id, ..
                        } = &mut effect
                        {
                            *sent_message_id = Some(local_message_id);
                        }
                        effect
                    }),
                    Ok(Response::Ok {
                        data:
                            ResponseData::SendOutcome {
                                outcome: Some(outcome),
                            },
                    }) if outcome.state == mxr_core::SendOutcomeState::Accepted
                        && matches!(effect, app::MutationEffect::SentSuccess { .. }) =>
                    {
                        Ok(effect)
                    }
                    Ok(Response::Ok {
                        data: ResponseData::SendOutcome { outcome },
                    }) => Ok(app::MutationEffect::StatusOnly(match outcome {
                        Some(outcome) => format!(
                            "Send {}: {}",
                            outcome.identity.operation_id,
                            match outcome.state {
                                mxr_core::SendOutcomeState::Accepted => "provider accepted",
                                mxr_core::SendOutcomeState::Unknown =>
                                    "acceptance uncertain; do not resend",
                                mxr_core::SendOutcomeState::NotAccepted =>
                                    "not accepted; review before a new attempt",
                            }
                        ),
                        None => "No send outcome recorded".into(),
                    })),
                    Ok(Response::Ok {
                        data: ResponseData::MutationResult { result },
                    }) if result.succeeded > 0 => {
                        if let Some(daemon_mutation_id) = result.mutation_id.clone() {
                            let _ =
                                result_tx_inner.send(AsyncResult::UndoCaptured(app::PendingUndo {
                                    action: app::UndoAction::Mutations(vec![daemon_mutation_id]),
                                    verb_past: verb.into(),
                                    count: result.succeeded,
                                    applied_at: std::time::Instant::now(),
                                }));
                        }
                        Ok(effect)
                    }
                    Ok(Response::Ok {
                        data: ResponseData::MutationResult { result },
                    }) => Err(MxrError::Ipc(format_mutation_failure(&result))),
                    Ok(Response::Ok {
                        data: ResponseData::InviteResponseSent { .. },
                    }) => Ok(effect),
                    Ok(Response::Ok {
                        data: ResponseData::SyncBatch { results },
                    }) => {
                        sync_batch_received = true;
                        let _ = result_tx_inner.send(AsyncResult::SyncBatch(results.clone()));
                        if results.is_empty() {
                            Ok(app::MutationEffect::StatusOnly(
                                "No enabled sync-capable accounts".into(),
                            ))
                        } else {
                            let failures = results
                                .iter()
                                .filter(|result| {
                                    result.result == mxr_protocol::SyncAccountResultKind::Failed
                                })
                                .map(|result| {
                                    format!(
                                        "{}: {}",
                                        result.account_name,
                                        result.error.as_deref().unwrap_or("sync couldn't start")
                                    )
                                })
                                .collect::<Vec<_>>();
                            if failures.is_empty() {
                                Ok(effect)
                            } else {
                                Err(MxrError::Ipc(format!(
                                    "Some accounts couldn't sync: {}",
                                    failures.join("; ")
                                )))
                            }
                        }
                    }
                    Ok(Response::Ok {
                        data: ResponseData::ModeGuides { .. },
                    }) => Ok(effect),
                    // Reading: the daemon's own words where it has them.
                    Ok(Response::Ok {
                        data: ResponseData::ReadingLater { copy, .. },
                    }) => Ok(app::MutationEffect::ModeDone(copy)),
                    Ok(Response::Ok {
                        data:
                            ResponseData::ReadingEngagement { .. }
                            | ResponseData::ReadingSource { .. }
                            | ResponseData::ReadingHighlight { .. },
                    }) => Ok(effect),
                    // Changing a highlight: the daemon's undo, offered with `u`.
                    Ok(Response::Ok {
                        data: ResponseData::ReadingHighlightChanged { change },
                    }) if !change.dry_run => {
                        let removed = change.action == "remove";
                        let mut copy = if removed {
                            "Highlight removed".to_string()
                        } else {
                            "Note saved".to_string()
                        };
                        if let Some(daemon_mutation_id) = change.mutation_id {
                            copy.push_str(". u to undo");
                            let _ =
                                result_tx_inner.send(AsyncResult::UndoCaptured(app::PendingUndo {
                                    action: app::UndoAction::Mutations(vec![daemon_mutation_id]),
                                    verb_past: if removed { "Removed" } else { "Edited" }.into(),
                                    count: 1,
                                    applied_at: std::time::Instant::now(),
                                }));
                        } else if change.undo_unavailable {
                            copy.push_str(". Its undo couldn't be saved");
                        }
                        Ok(app::MutationEffect::ModeDone(copy))
                    }
                    // `M`: Marginalia answers with where the item opens.
                    Ok(Response::Ok {
                        data: ResponseData::MarginaliaSend { send },
                    }) => Ok(app::MutationEffect::ModeDone(
                        match send.marginalia.as_ref().map(|m| open::that(&m.read_url)) {
                            Some(Ok(())) => format!("{} Opened in your browser.", send.copy),
                            Some(Err(_)) => format!(
                                "{} Open it at {}",
                                send.copy,
                                send.marginalia.map(|m| m.read_url).unwrap_or_default()
                            ),
                            None => send.copy,
                        },
                    )),
                    Ok(Response::Ok {
                        data: ResponseData::UnsubscribePurgeResult { result },
                    }) => match result.error {
                        Some(error) => Err(MxrError::Ipc(error)),
                        None => Ok(effect),
                    },
                    Ok(Response::Ok {
                        data: ResponseData::MessagesAck { .. },
                    }) => Ok(effect),
                    Ok(Response::Ok {
                        data: ResponseData::RecordChange { change },
                    }) => {
                        if let Some(undo) = records_undo(&change) {
                            let _ = result_tx_inner.send(AsyncResult::UndoCaptured(undo));
                        }
                        // The daemon's own words: "Filed in Archive.", never
                        // the provider's "Archived".
                        Ok(app::MutationEffect::Records(change.message))
                    }
                    Ok(Response::Ok {
                        data: ResponseData::TodoChange { change },
                    }) => {
                        if let Some(undo) = todo_undo(&change) {
                            let _ = result_tx_inner.send(AsyncResult::UndoCaptured(undo));
                        }
                        if change.changed.is_empty() {
                            // Nothing was in a state this applies to: say
                            // so in the daemon's words, and refetch.
                            Ok(app::MutationEffect::Todo(change.summary))
                        } else {
                            Ok(effect)
                        }
                    }
                    Ok(Response::Ok {
                        data:
                            ResponseData::SenderKindSet {
                                account_id,
                                sender_email,
                                previous,
                                ..
                            },
                    }) => {
                        // A user's move offers undo; the undo itself does not.
                        if matches!(effect, app::MutationEffect::SenderMoved(_)) {
                            let _ =
                                result_tx_inner.send(AsyncResult::UndoCaptured(app::PendingUndo {
                                    action: app::UndoAction::SenderKind {
                                        account_id,
                                        sender_email,
                                        previous,
                                    },
                                    verb_past: "Moved sender".into(),
                                    count: 1,
                                    applied_at: std::time::Instant::now(),
                                }));
                        }
                        Ok(effect)
                    }
                    Ok(Response::Ok {
                        data:
                            ResponseData::DeskThreadsDismissed { .. }
                            | ResponseData::DeskThreadsRestored { .. },
                    }) => Ok(effect),
                    Ok(Response::Ok {
                        data: ResponseData::MessagesPinned { changed, .. },
                    }) => match effect {
                        app::MutationEffect::UpdatesPin {
                            message_id,
                            prior_pinned,
                            status,
                        } => {
                            let (effect, undo) =
                                updates_pin_outcome(message_id, prior_pinned, status, changed);
                            if let Some(undo) = undo {
                                let _ = result_tx_inner.send(AsyncResult::UndoCaptured(undo));
                            }
                            Ok(effect)
                        }
                        effect => Ok(effect),
                    },
                    Ok(Response::Ok {
                        data: ResponseData::MessageMoved { outcome },
                    }) => {
                        // `u` puts the move back exactly; the toast is the
                        // daemon's copy and its sender question.
                        if let Some(correction_id) = outcome.correction_id {
                            let _ =
                                result_tx_inner.send(AsyncResult::UndoCaptured(app::PendingUndo {
                                    action: app::UndoAction::Move(correction_id),
                                    verb_past: "Moved".into(),
                                    count: 1,
                                    applied_at: std::time::Instant::now(),
                                }));
                        }
                        let status = app::move_status(&outcome, move_not_sure);
                        let _ = result_tx_inner
                            .send(AsyncResult::Moved(Box::new(outcome), move_not_sure));
                        Ok(app::MutationEffect::ModeDone(status))
                    }
                    Ok(Response::Ok {
                        data: ResponseData::MoveUndone { copy, .. },
                    }) => Ok(app::MutationEffect::ModeDone(copy)),
                    Ok(Response::Ok {
                        data:
                            ResponseData::ModeDone {
                                items,
                                mutation_id,
                                undo_unavailable,
                                dry_run: false,
                            },
                    }) => {
                        let (effect, undo) =
                            mode_done_outcome(&items, mutation_id, undo_unavailable);
                        if let Some(undo) = undo {
                            let _ = result_tx_inner.send(AsyncResult::UndoCaptured(undo));
                        }
                        effect
                    }
                    Ok(Response::Ok {
                        data: ResponseData::UpdatesLetGo { result },
                    }) if !result.dry_run => {
                        let (effect, undo) = updates_let_go_outcome(&result);
                        if let Some(undo) = undo {
                            let _ = result_tx_inner.send(AsyncResult::UndoCaptured(undo));
                        }
                        effect
                    }
                    Ok(Response::Ok {
                        data: ResponseData::UpdateSource { change },
                    }) => {
                        // A user's tuning offers undo; the undo itself does not.
                        let undoing = matches!(
                            &effect,
                            app::MutationEffect::ModeDone(message) if !message.is_empty()
                        );
                        if !undoing {
                            let _ =
                                result_tx_inner.send(AsyncResult::UndoCaptured(app::PendingUndo {
                                    action: app::UndoAction::UpdateSource {
                                        account_id: change.account_id.clone(),
                                        source_key: change.source_key.clone(),
                                        prior: change.prior,
                                    },
                                    verb_past: "Tuned".into(),
                                    count: 1,
                                    applied_at: std::time::Instant::now(),
                                }));
                        }
                        let copy = crate::ui::sanitize::one_line(&change.copy);
                        Ok(app::MutationEffect::ModeDone(if undoing {
                            copy
                        } else {
                            format!("{copy} u to undo")
                        }))
                    }
                    Ok(Response::Ok {
                        data:
                            ResponseData::DeskItemsResolved {
                                items,
                                mutation_id,
                                undo_unavailable,
                                ..
                            },
                    }) => {
                        let done = items.iter().filter(|item| item.error.is_none()).count();
                        // Offer `u` only when there is an undo to run.
                        let effect = if mutation_id.is_some() {
                            effect
                        } else if undo_unavailable {
                            app::MutationEffect::RefreshPlaces(
                                "Done, but its undo couldn't be saved".into(),
                            )
                        } else {
                            app::MutationEffect::RefreshPlaces("Done".into())
                        };
                        if let Some(daemon_mutation_id) = mutation_id {
                            let _ =
                                result_tx_inner.send(AsyncResult::UndoCaptured(app::PendingUndo {
                                    action: app::UndoAction::Mutations(vec![daemon_mutation_id]),
                                    verb_past: "Done".into(),
                                    count: done as u32,
                                    applied_at: std::time::Instant::now(),
                                }));
                        }
                        // A conversation Done could not put away keeps its
                        // row (the failure refetches the desk); `e` retries.
                        match items.iter().find_map(|item| item.error.as_deref()) {
                            None => Ok(effect),
                            Some(error) => Err(MxrError::Ipc(format!("Not done: {error}"))),
                        }
                    }
                    Ok(Response::Ok {
                        data:
                            ResponseData::ThreadsDeferred {
                                items,
                                until,
                                mutation_id,
                                ..
                            },
                    }) => {
                        // Say what the daemon set, in its words: the time and
                        // whether it waits on a reply.
                        let when = until
                            .with_timezone(&chrono::Local)
                            .format("%a %e %b %H:%M")
                            .to_string();
                        let set = items
                            .iter()
                            .filter(|item| item.error.is_none())
                            .filter_map(|item| item.kind)
                            .collect::<Vec<_>>();
                        let status = match set.first() {
                            Some(mxr_protocol::DeferKindData::Waiting) => {
                                format!("Back {when} if nobody replies")
                            }
                            _ => format!("Reply later: back {when}"),
                        };
                        if let Some(daemon_mutation_id) = mutation_id {
                            let _ =
                                result_tx_inner.send(AsyncResult::UndoCaptured(app::PendingUndo {
                                    action: app::UndoAction::Mutations(vec![daemon_mutation_id]),
                                    verb_past: "Time set".into(),
                                    count: set.len() as u32,
                                    applied_at: std::time::Instant::now(),
                                }));
                        }
                        match items.iter().find_map(|item| item.error.as_deref()) {
                            None => Ok(app::MutationEffect::RefreshPlaces(format!(
                                "{status}. Press u to undo"
                            ))),
                            Some(error) => Err(MxrError::Ipc(format!("Not set: {error}"))),
                        }
                    }
                    Ok(Response::Error {
                        message,
                        details,
                        code,
                        ..
                    }) => {
                        if code == "archive_incomplete" {
                            if let Some(undo) = archive_partial_undo(details, verb) {
                                let _ = result_tx_inner.send(AsyncResult::UndoCaptured(undo));
                            }
                        }
                        // A partly failed undo keeps what failed under the
                        // same id: `u` again retries just those.
                        if let (Some(mutation_id), Some(failed)) =
                            (undo_id, undo_retry_count(&message))
                        {
                            let _ =
                                result_tx_inner.send(AsyncResult::UndoCaptured(app::PendingUndo {
                                    action: app::UndoAction::Mutations(vec![mutation_id]),
                                    verb_past: "Not yet restored:".into(),
                                    count: failed,
                                    applied_at: std::time::Instant::now(),
                                }));
                        }
                        Err(MxrError::Ipc(message))
                    }
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response to mutation".into())),
                };
                AsyncResult::MutationResult {
                    id: mutation_id,
                    best_effort,
                    retry: retry.map(Box::new),
                    sync_request_pending_failure: is_sync_all_request && !sync_batch_received,
                    outcome: Box::new(outcome),
                }
                }).await
            });
        }
        app.pending_mutation_queue.extend(deferred_mutations);

        // Thread export: the picker chose the format and destination.
        if let Some(pending) = app.mailbox.pending_export.take() {
            let bg = bg.clone();
            let _ = submit_task(&local_io, async move {
                AsyncResult::ExportResult(crate::export_output::export_thread(&bg, pending).await)
            });
        }

        // Handle compose actions
        if let Some(compose_action) = app.compose.pending_compose.take() {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let result = handle_compose_action(&bg, compose_action).await;
                AsyncResult::ComposeReady(result)
            });
        }

        // Prewarm reply context for the message currently in view on a
        // *dedicated* daemon connection. The shared IPC worker is serial
        // (one request in flight at a time), so the prewarm must run on
        // its own socket — otherwise it would queue ahead of the user's
        // `r`/`a` action. Daemon-side, PrepareReply is now a cheap memo
        // hit on the second call for the same message, so the prewarm
        // fills the cache and the keypress wins immediately.
        if let Some(env_id) = app.context_envelope().map(|env| env.id.clone()) {
            let already_warming = app
                .compose
                .last_prewarmed_message_id
                .as_ref()
                .is_some_and(|id| id == &env_id);
            let already_cached = app.compose.reply_context_cache.contains_key(&env_id);
            if !already_warming && !already_cached {
                app.compose.last_prewarmed_message_id = Some(env_id.clone());
                let session = session.clone();
                let result_tx_clone = result_tx.clone();
                let message_id = env_id.clone();
                let generation = app.compose.reply_context_generation;
                tokio::spawn(async move {
                    let reply =
                        fetch_reply_context_dedicated(&session, message_id.clone(), false).await;
                    let reply_all =
                        fetch_reply_context_dedicated(&session, message_id.clone(), true).await;
                    let _ = result_tx_clone.send(AsyncResult::ReplyContextWarmed {
                        message_id,
                        generation,
                        reply: Box::new(reply),
                        reply_all: Box::new(reply_all),
                    });
                });
            }
        }

        // Future that resolves when the debounced summary request is
        // ready to fire. Returns `pending` (never resolves) when no
        // debounce is set, so the select arm stays armed without
        // forcing a wake-up. Re-evaluated each loop iteration, so a
        // freshly-stamped debounce shifts the deadline correctly.
        let debounce_wait = async {
            match app.pending_summary_debounce.as_ref() {
                Some((_, deadline)) => tokio::time::sleep_until(*deadline).await,
                None => std::future::pending::<()>().await,
            }
        };

        tokio::select! {
            event = events.as_mut().expect("event stream").next() => {
                if let Some(Ok(Event::Key(key))) = event {
                    if let Some(action) = app.handle_key(key) {
                        if matches!(action, crate::action::Action::CancelOutlookAuth) {
                            if let Some(session) =
                                app.accounts.page.active_auth_session.take()
                            {
                                app.accounts.pending_auth_session_account = None;
                                app.accounts.page.operation_in_flight = false;
                                app.accounts.page.throbber = Default::default();
                                let bg2 = bg.clone();
                                tokio::spawn(async move {
                                    let _ = ipc_call(
                                        &bg2,
                                        Request::CancelAuthSession {
                                            session_id: session.session_id,
                                        },
                                    )
                                    .await;
                                });
                            }
                        } else {
                            app.apply(action);
                        }
                    }
                }
                if let Some(Ok(Event::Paste(text))) = event {
                    app.handle_paste(&text);
                }
            }
            _ = debounce_wait => {
                // Wake-up only — the top of the next loop iteration
                // checks `pending_summary_debounce` against the
                // current time and drains it into the real request.
            }
            result = result_rx.recv() => {
                if let Some(msg) = result {
                    match msg {
                        AsyncResult::Search {
                            target,
                            append,
                            session_id,
                            result: Ok(results),
                        } => match target {
                            app::SearchTarget::SearchPage => {
                                if session_id != app.search.page.session_id {
                                    continue;
                                }
                                app.apply_search_page_results(append, results);
                            }
                            app::SearchTarget::Mailbox => {
                                if session_id != app.search.mailbox_session_id {
                                    continue;
                                }
                                let mut envelopes = results.envelopes;
                                app.pending_optimistic.apply(&mut envelopes);
                                app.mailbox.envelopes = envelopes;
                                app.mailbox.selected_index = 0;
                                app.mailbox.scroll_offset = 0;
                                app.mailbox.pending_commitment_counts_refresh = true;
                            }
                        },
                        AsyncResult::Search {
                            target,
                            append: _,
                            session_id,
                            result: Err(error),
                        } => {
                            match target {
                                app::SearchTarget::SearchPage => {
                                    if session_id != app.search.page.session_id {
                                        continue;
                                    }
                                    app.search.page.loading_more = false;
                                    app.search.page.load_to_end = false;
                                    app.search.page.count_pending = false;
                                    app.search.page.total_count = None;
                                    app.search.page.ui_status = app::SearchUiStatus::Error;
                                }
                                app::SearchTarget::Mailbox => {
                                    if session_id != app.search.mailbox_session_id {
                                        continue;
                                    }
                                    app.mailbox.envelopes = app.mailbox.all_envelopes.clone();
                                }
                            }
                            app.status_message = Some(format!("Search failed: {error}"));
                        }
                        AsyncResult::SearchCount {
                            session_id,
                            result: Ok(count),
                        } => {
                            if session_id != app.search.page.session_id {
                                continue;
                            }
                            app.search.page.total_count = Some(count);
                            app.search.page.count_pending = false;
                            if app.search.page.ui_status != app::SearchUiStatus::Error {
                                app.search.page.ui_status = app::SearchUiStatus::Loaded;
                            }
                        }
                        AsyncResult::SearchCount {
                            session_id,
                            result: Err(error),
                        } => {
                            if session_id != app.search.page.session_id {
                                continue;
                            }
                            app.search.page.count_pending = false;
                            if app.search.page.results.is_empty()
                                && matches!(app.search.page.ui_status, app::SearchUiStatus::Searching)
                            {
                                app.search.page.ui_status = app::SearchUiStatus::Error;
                            }
                            app.status_message = Some(format!("Search count failed: {error}"));
                        }
                        AsyncResult::ScheduledIdentityRetained(account, identity, draft_id) => {
                            let alias = format!("draft:{draft_id}");
                            app.apply_retained_send_identity(account.clone(),identity.clone());
                            if app.compose.last_send_identity.as_ref()==Some(&(account,identity)) { app.compose.last_send_query=Some(alias); }
                        }
                        AsyncResult::SendIdentityRetained(account, identity) => {
                            app.apply_retained_send_identity(account, identity);
                        }
                        AsyncResult::Rules(Ok(rules)) => {
                            app.replace_rule_list(rules);
                        }
                        AsyncResult::Rules(Err(e)) => {
                            app.rules.page.status = Some(format!("Rules error: {e}"));
                        }
                        AsyncResult::PersonalCategory {
                            generation,
                            report_if_stale,
                            view_category_id,
                            result,
                        } => {
                            app.handle_personal_category_result(
                                generation,
                                report_if_stale,
                                view_category_id,
                                result,
                            );
                        }
                        AsyncResult::SnippetsList(Ok(snippets)) => {
                            let count = snippets.len();
                            app.modals.snippets.set_snippets(snippets);
                            app.status_message = Some(if count == 0 {
                                "No snippets yet — see `mxr snippets set` to create one".into()
                            } else {
                                format!("{count} snippet(s)")
                            });
                        }
                        AsyncResult::SnippetsList(Err(e)) => {
                            app.modals.snippets.set_error(e.to_string());
                            app.status_message = Some(format!("Snippets load failed: {e}"));
                        }
                        AsyncResult::ReplyQueueList(Ok(messages)) => {
                            let count = messages.len();
                            app.mailbox.reply_later_message_ids =
                                messages.iter().map(|message| message.id.clone()).collect();
                            app.modals.reply_queue.set_messages(messages);
                            app.status_message = Some(if count == 0 {
                                "Reply queue is empty".into()
                            } else {
                                format!("{count} message(s) flagged for reply later")
                            });
                        }
                        AsyncResult::ReplyQueueList(Err(e)) => {
                            app.modals.reply_queue.set_error(e.to_string());
                            app.status_message = Some(format!("Reply queue load failed: {e}"));
                        }
                        AsyncResult::StoredDraftsLoaded(Ok((drafts, sends))) => {
                            let count = drafts.len();
                            app.modals.drafts.set_drafts(drafts);
                            app.modals.drafts.scheduled = sends
                                .into_iter()
                                .map(|send| (send.draft_id, send.send_at))
                                .collect();
                            let scheduled = app.modals.drafts.scheduled.len();
                            app.status_message = Some(match (count, scheduled) {
                                (0, _) => "No saved drafts".into(),
                                (count, 0) => format!("{count} draft(s)"),
                                (count, scheduled) => {
                                    format!("{count} draft(s), {scheduled} scheduled")
                                }
                            });
                        }
                        AsyncResult::StoredDraftUnscheduled {
                            draft_id,
                            result: Ok(()),
                        } => {
                            app.modals.drafts.finish_operation();
                            app.modals.drafts.scheduled.remove(&draft_id);
                            app.status_message =
                                Some("Scheduled send cancelled; the draft is kept".into());
                            app.push_toast(crate::app::Toast::success("Scheduled send cancelled"));
                        }
                        AsyncResult::StoredDraftUnscheduled { result: Err(e), .. } => {
                            app.modals.drafts.finish_operation();
                            app.status_message = Some(format!("Couldn't cancel the send: {e}"));
                            app.push_toast(crate::app::Toast::error(format!(
                                "Couldn't cancel the send: {e}"
                            )));
                        }
                        AsyncResult::StoredDraftsLoaded(Err(e)) => {
                            app.modals.drafts.set_error(e.to_string());
                            app.status_message = Some(format!("Drafts load failed: {e}"));
                        }
                        AsyncResult::StoredDraftDeleted { draft_id, result: Ok(()) } => {
                            app.modals.drafts.finish_operation();
                            app.modals.drafts.remove(&draft_id);
                            app.status_message = Some(format!("Deleted local draft {draft_id}"));
                            app.push_toast(crate::app::Toast::success("Draft deleted"));
                        }
                        AsyncResult::StoredDraftDeleted { result: Err(e), .. } => {
                            app.modals.drafts.finish_operation();
                            app.status_message = Some(format!("Draft delete failed: {e}"));
                            app.push_toast(crate::app::Toast::error(format!(
                                "Draft delete failed: {e}"
                            )));
                        }
                        AsyncResult::StoredDraftPushed {
                            draft_id,
                            provider,
                            result: Ok(()),
                        } => {
                            app.modals.drafts.finish_operation();
                            app.status_message = Some(format!(
                                "Copied draft {draft_id} to {provider}; local draft preserved"
                            ));
                            app.push_toast(crate::app::Toast::success(format!(
                                "Draft copied to {provider}"
                            )));
                        }
                        AsyncResult::StoredDraftPushed { result: Err(e), .. } => {
                            app.modals.drafts.finish_operation();
                            app.status_message = Some(format!("Provider draft copy failed: {e}"));
                            app.push_toast(crate::app::Toast::error(format!(
                                "Provider draft copy failed: {e}"
                            )));
                        }
                        AsyncResult::DeliveriesList(Ok(rows)) => {
                            let count = rows.len();
                            app.deliveries.set_rows(rows);
                            app.status_message = Some(if count == 0 {
                                "No deliveries".into()
                            } else {
                                format!("{count} deliver{} tracked", if count == 1 { "y" } else { "ies" })
                            });
                        }
                        AsyncResult::DeliveriesList(Err(e)) => {
                            app.deliveries.set_error(e.to_string());
                            app.status_message = Some(format!("Deliveries load failed: {e}"));
                        }
                        AsyncResult::DeliveryThreadOpened(Ok(messages)) => {
                            app.open_delivery_thread(messages);
                        }
                        AsyncResult::DeliveryThreadOpened(Err(e)) => {
                            app.status_message = Some(format!("Open source email failed: {e}"));
                        }
                        AsyncResult::AssistantThreadOpened(Ok(messages)) => {
                            app.open_assistant_thread(messages);
                        }
                        AsyncResult::AssistantThreadOpened(Err(e)) => {
                            app.status_message = Some(format!("Couldn't open that email: {e}"));
                        }
                        AsyncResult::ActivityList(Ok(entries)) => {
                            let count = entries.len();
                            app.modals.activity.set_entries(entries);
                            app.status_message = Some(if count == 0 {
                                "Activity log is empty for the last 24h".into()
                            } else {
                                format!("Loaded {count} activity rows (last 24h)")
                            });
                        }
                        AsyncResult::ActivityList(Err(e)) => {
                            app.modals.activity.set_error(e.to_string());
                            app.status_message = Some(format!("Activity load failed: {e}"));
                        }
                        AsyncResult::ActivityPauseToggled(Ok(now_paused)) => {
                            app.modals.activity.paused = now_paused;
                            app.status_message = Some(if now_paused {
                                "Activity recording paused".into()
                            } else {
                                "Activity recording resumed".into()
                            });
                            app.pending_activity_refresh = true;
                        }
                        AsyncResult::ActivityPauseToggled(Err(e)) => {
                            app.status_message = Some(format!("Pause toggle failed: {e}"));
                        }
                        AsyncResult::ScreenerQueueLoaded { account_id, result } => {
                            let still_relevant = app
                                .modals
                                .screener
                                .account_id
                                .as_ref()
                                .is_some_and(|current| current == &account_id);
                            if !still_relevant {
                                continue;
                            }
                            match result {
                                Ok(entries) => {
                                    let count = entries.len();
                                    app.modals.screener.set_entries(entries);
                                    app.status_message = Some(format!(
                                        "Screener queue: {count} sender(s)",
                                    ));
                                }
                                Err(e) => {
                                    app.modals.screener.set_error(e.to_string());
                                    app.status_message =
                                        Some(format!("Screener load failed: {e}"));
                                }
                            }
                        }
                        AsyncResult::ScreenerDecisionApplied {
                            account_id,
                            sender_email,
                            result,
                        } => {
                            if let Err(e) = result {
                                app.status_message = Some(format!(
                                    "Screener disposition for {sender_email} failed: {e}"
                                ));
                                // Re-fetch to recover from optimistic removal.
                                app.pending_screener_refresh = Some(account_id);
                            }
                        }
                        AsyncResult::SenderProfileLoaded { email, result } => {
                            // Drop late responses for a sender other than
                            // the one currently shown in the modal.
                            let still_relevant = app
                                .modals
                                .sender_profile
                                .email
                                .as_deref()
                                .is_some_and(|current| current == email);
                            if !still_relevant {
                                continue;
                            }
                            match *result {
                                Ok(profile) => {
                                    app.modals.sender_profile.set_profile(profile);
                                }
                                Err(e) => {
                                    app.modals.sender_profile.set_error(e.to_string());
                                    app.status_message =
                                        Some(format!("Sender profile failed: {e}"));
                                }
                            }
                        }
                        AsyncResult::ThreadSummaryLoaded { thread_id, result } => {
                            apply_thread_summary_loaded(&mut app, thread_id, result);
                        }
                        AsyncResult::ThreadContextLoaded { thread_id, result } => {
                            apply_thread_context_loaded(&mut app, &thread_id, result);
                        }
                        AsyncResult::PromisesDetected { message_id, result } => {
                            // A promise check never interrupts: failures are
                            // logged, not shown.
                            match result {
                                Ok(detection) => app.offer_promises(message_id, *detection),
                                Err(error) => {
                                    tracing::debug!(%error, "promise check after send failed");
                                }
                            }
                        }
                        AsyncResult::ThreadGistLoaded { thread_id, result } => {
                            apply_thread_gist_loaded(&mut app, &thread_id, result);
                        }
                        AsyncResult::RowGistsLoaded {
                            thread_ids,
                            epoch,
                            result,
                        } => match result {
                            Ok(batch) => app.row_gists.apply_batch(
                                &thread_ids,
                                epoch,
                                &batch,
                                std::time::Instant::now(),
                            ),
                            // Quiet: rows keep their snippets and are asked
                            // about again.
                            Err(error) => {
                                tracing::debug!(%error, "row gists request failed");
                                app.row_gists.request_failed(&thread_ids);
                            }
                        },
                        AsyncResult::RuleDetail {
                            request_id,
                            result: Ok(rule),
                        } => {
                            if request_id != app.rules.detail_request_id {
                                tracing::trace!(request_id, current_id = app.rules.detail_request_id, "tui stale rule detail dropped");
                                continue;
                            }
                            app.rules.page.detail = Some(rule);
                            app.rules.page.panel = app::RulesPanel::Details;
                        }
                        AsyncResult::RuleDetail {
                            request_id,
                            result: Err(e),
                        } => {
                            if request_id != app.rules.detail_request_id {
                                tracing::trace!(request_id, current_id = app.rules.detail_request_id, "tui stale rule detail dropped");
                                continue;
                            }
                            app.rules.page.status = Some(format!("Rule error: {e}"));
                        }
                        AsyncResult::RuleHistory {
                            request_id,
                            result: Ok(entries),
                        } => {
                            if request_id != app.rules.history_request_id {
                                tracing::trace!(request_id, current_id = app.rules.history_request_id, "tui stale rule history dropped");
                                continue;
                            }
                            app.rules.page.history = entries;
                        }
                        AsyncResult::RuleHistory {
                            request_id,
                            result: Err(e),
                        } => {
                            if request_id != app.rules.history_request_id {
                                tracing::trace!(request_id, current_id = app.rules.history_request_id, "tui stale rule history dropped");
                                continue;
                            }
                            app.rules.page.status = Some(format!("History error: {e}"));
                        }
                        AsyncResult::RuleDryRun { request_id, result: Ok(results) } => {
                            if request_id != app.rules.dry_run_request_id { continue; }
                            app.rules.page.dry_run = results;
                            app.rules.page.dry_run_page = 0;
                        }
                        AsyncResult::RuleDryRun { request_id, result: Err(e) } => {
                            if request_id != app.rules.dry_run_request_id { continue; }
                            app.rules.page.status = Some(format!("Dry-run error: {e}"));
                        }
                        AsyncResult::RuleTreatment(result) => {
                            app.rules.page.status = Some(match result {
                                Ok(value) if value["category"] == true => "Category preview applied; manual choices preserved".into(),
                                Ok(_) => "Sorting preview applied; personal corrections preserved".into(),
                                Err(e) => format!("Sorting failed: {e}"),
                            });
                            app.rules.page.refresh_pending = true;
                        }
                        AsyncResult::RuleForm {
                            request_id,
                            result: Ok(form),
                        } => {
                            if request_id != app.rules.form_request_id {
                                tracing::trace!(request_id, current_id = app.rules.form_request_id, "tui stale rule form dropped");
                                continue;
                            }
                            app.rules.page.form.visible = true;
                            app.rules.page.form.existing_rule = form.id;
                            app.rules.page.form.account_id = form.account_id;
                            app.rules.page.form.name = form.name;
                            app.rules.page.form.condition = form.condition;
                            app.rules.page.form.action = form.action;
                            app.rules.page.form.priority = form.priority.to_string();
                            app.rules.page.form.enabled = form.enabled;
                            app.rules.page.form.active_field = 0;
                            app.sync_rule_form_editors();
                            app.rules.page.panel = app::RulesPanel::Form;
                        }
                        AsyncResult::RuleForm {
                            request_id,
                            result: Err(e),
                        } => {
                            if request_id != app.rules.form_request_id {
                                tracing::trace!(request_id, current_id = app.rules.form_request_id, "tui stale rule form dropped");
                                continue;
                            }
                            app.rules.page.status = Some(format!("Form error: {e}"));
                        }
                        AsyncResult::RuleDeleted(Ok(())) => {
                            app.rules.page.status = Some("Rule deleted".into());
                            app.rules.page.refresh_pending = true;
                        }
                        AsyncResult::RuleDeleted(Err(e)) => {
                            app.rules.page.status = Some(format!("Delete error: {e}"));
                        }
                        AsyncResult::RuleUpsert(Ok(rule)) => {
                            app.rules.page.detail = Some(rule.clone());
                            app.rules.page.form.visible = false;
                            app.rules.page.panel = app::RulesPanel::Details;
                            app.rules.page.status = Some("Rule saved".into());
                            app.rules.page.refresh_pending = true;
                        }
                        AsyncResult::RuleUpsert(Err(e)) => {
                            app.rules.page.status = Some(format!("Save error: {e}"));
                        }
                        AsyncResult::Diagnostics { request_id, result } => {
                            if request_id != app.diagnostics.request_id {
                                tracing::trace!(request_id, current_id = app.diagnostics.request_id, "tui stale diagnostics dropped");
                                continue;
                            }
                            app.diagnostics.page.pending_requests =
                                app.diagnostics.page.pending_requests.saturating_sub(1);
                            match *result {
                                Ok(response) => match response {
                                Response::Ok {
                                    data:
                                        ResponseData::Status {
                                            uptime_secs,
                                            daemon_pid,
                                            accounts,
                                            total_messages,
                                            sync_statuses,
                                            degraded,
                                            recovery,
                                            ..
                                        },
                                } => {
                                    app.apply_status_snapshot(StatusSnapshot {
                                        uptime_secs,
                                        daemon_pid,
                                        accounts,
                                        total_messages,
                                        sync_statuses,
                                        degraded,
                                        freshness: None,
                                    });
                                    app.apply_recovery_status_snapshot(recovery);
                                }
                                Response::Ok {
                                    data: ResponseData::DoctorReport { report },
                                } => {
                                    app.diagnostics.page.doctor = Some(report);
                                }
                                Response::Ok {
                                    data: ResponseData::EventLogEntries { entries },
                                } => {
                                    app.diagnostics.page.events = entries;
                                }
                                Response::Ok {
                                    data: ResponseData::LogLines { lines },
                                } => {
                                    app.diagnostics.page.logs = lines;
                                }
                                Response::Ok {
                                    data: ResponseData::ActivityEntries { entries, .. },
                                } => {
                                    app.diagnostics.page.activity = entries;
                                }
                                Response::Ok {
                                    data: ResponseData::Jobs { jobs },
                                } => {
                                    app.diagnostics.page.jobs = jobs;
                                }
                                Response::Error { message, .. } => {
                                    app.diagnostics.page.status = Some(message);
                                }
                                _ => {}
                                },
                                Err(e) => {
                                    app.diagnostics.page.status =
                                        Some(format!("Diagnostics error: {e}"));
                                }
                            }
                        }
                        AsyncResult::Status {
                            request_id,
                            result: Ok(snapshot),
                        } => {
                            if request_id != app.diagnostics.status_request_id {
                                tracing::trace!(request_id, current_id = app.diagnostics.status_request_id, "tui stale status dropped");
                                continue;
                            }
                            app.apply_status_snapshot(snapshot);
                        }
                        AsyncResult::Recovery { owner, request, result } => {
                            let is_preview_request = matches!(
                                &request,
                                Request::ActivateRecovery { dry_run: true, .. }
                                    | Request::PrepareRecoveredDraft { dry_run: true, .. }
                            );
                            if is_preview_request {
                                app.diagnostics.page.recovery_preview = None;
                            }
                            match *result {
                            Ok(Response::Ok { data: ResponseData::RecoveryAction { action } }) => {
                                let is_preview = action.dry_run;
                                if is_preview {
                                    app.apply_recovery_preview_result(owner, request, action);
                                } else {
                                    app.diagnostics.page.recovery_preview = None;
                                    app.status_message = Some(if action.prepared_draft.is_some() {
                                        "New draft prepared. Review it normally before sending.".into()
                                    } else {
                                        "Recovery activated. Original drafts and intentions remain held.".into()
                                    });
                                    app.apply_recovery_status_snapshot(Some(action.status));
                                }
                            }
                            Ok(Response::Error { message, .. }) => {
                                app.status_message = Some(format!("Recovery action failed: {message}"));
                            }
                            Ok(_) => {
                                app.status_message = Some("Recovery action returned an unexpected response.".into());
                            }
                            Err(error) => {
                                app.status_message = Some(format!("Recovery action failed: {error}"));
                            }
                        }
                        },
                        AsyncResult::Status {
                            request_id,
                            result: Err(e),
                        } => {
                            if request_id != app.diagnostics.status_request_id {
                                tracing::trace!(request_id, current_id = app.diagnostics.status_request_id, "tui stale status dropped");
                                continue;
                            }
                            app.status_message = Some(format!("Status refresh failed: {e}"));
                        }
                        AsyncResult::Accounts(Ok(accounts)) => {
                            app.accounts.page.accounts = accounts;
                            app.accounts.page.selected_index = app
                                .accounts
                                .page
                                .selected_index
                                .min(app.accounts.page.accounts.len().saturating_sub(1));
                            if app.accounts.page.accounts.is_empty() {
                                app.accounts.page.onboarding_required = true;
                            } else {
                                app.accounts.page.onboarding_required = false;
                                app.accounts.page.onboarding_modal_open = false;
                            }
                        }
                        AsyncResult::Accounts(Err(e)) => {
                            app.accounts.page.status = Some(format!("Accounts error: {e}"));
                        }
                        AsyncResult::Labels(Ok(labels)) => {
                            apply_labels_refresh(&mut app, labels);
                        }
                        AsyncResult::Labels(Err(e)) => {
                            app.status_message = Some(format!("Label refresh failed: {e}"));
                        }
                        AsyncResult::AllEnvelopes(Ok(envelopes)) => {
                            apply_all_envelopes_refresh(&mut app, envelopes);
                        }
                        AsyncResult::AllEnvelopes(Err(e)) => {
                            app.mailbox.mailbox_loading_message = None;
                            app.status_message =
                                Some(format!("Mailbox refresh failed: {e}"));
                        }
                        AsyncResult::AccountOperation(Ok(result)) => {
                            let was_switch = app.accounts.pending_switch;
                            app.accounts.pending_switch = false;
                            app.apply_account_operation_result(result);
                            if was_switch {
                                app.handle_account_switch_complete();
                            }
                        }
                        AsyncResult::AccountOperation(Err(e)) => {
                            app.accounts.pending_switch = false;
                            app.mailbox.mailbox_loading_message = None;
                            app.accounts.page.operation_in_flight = false;
                            app.accounts.page.throbber = Default::default();
                            app.accounts.page.status = Some(format!("Account error: {e}"));
                            app.modals.error = Some(app::ErrorModalState::new(
                                "Account Operation Failed",
                                format!("The account test or save request failed.\n\n{e}"),
                            ));
                        }
                        AsyncResult::AuthSession(session) => {
                            use mxr_protocol::AuthSessionStateData;
                            match session.state {
                                AuthSessionStateData::WaitingForUser => {
                                    app.accounts.page.active_auth_session = Some(session);
                                }
                                AuthSessionStateData::Authorized => {
                                    app.accounts.page.active_auth_session = None;
                                    if let Some(account) =
                                        app.accounts.pending_auth_session_account.take()
                                    {
                                        let bg2 = bg.clone();
                                        let _ = submit_task(&queued, async move {
                                            use crate::account_workflow::run_post_auth_save_workflow;
                                            let result =
                                                run_post_auth_save_workflow(&bg2, account).await;
                                            AsyncResult::AccountOperation(result)
                                        });
                                    } else {
                                        app.accounts.page.operation_in_flight = false;
                                        app.accounts.page.throbber = Default::default();
                                    }
                                }
                                AuthSessionStateData::Failed => {
                                    let err = session
                                        .error
                                        .unwrap_or_else(|| "Authorization failed".into());
                                    app.accounts.page.active_auth_session = None;
                                    app.accounts.pending_auth_session_account = None;
                                    app.accounts.page.operation_in_flight = false;
                                    app.accounts.page.throbber = Default::default();
                                    app.accounts.page.status = Some(format!("Auth error: {err}"));
                                    app.modals.error = Some(app::ErrorModalState::new(
                                        "Outlook Authorization Failed",
                                        format!("Microsoft authorization failed.\n\n{err}"),
                                    ));
                                }
                                AuthSessionStateData::Cancelled => {
                                    app.accounts.page.active_auth_session = None;
                                    app.accounts.pending_auth_session_account = None;
                                    app.accounts.page.operation_in_flight = false;
                                    app.accounts.page.throbber = Default::default();
                                }
                                AuthSessionStateData::Starting => {}
                            }
                        }
                        AsyncResult::BugReport(Ok(content)) => {
                            submit_bug_report_write(&local_io, content);
                        }
                        AsyncResult::BugReport(Err(e)) => {
                            app.diagnostics.page.status = Some(format!("Bug report error: {e}"));
                        }
                        AsyncResult::AttachmentFile {
                            operation,
                            result: Ok(file),
                        } => {
                            app.resolve_attachment_file(&file);
                            let action = match operation {
                                AttachmentOperation::Open => "Opened",
                                AttachmentOperation::Download => "Downloaded",
                            };
                            let message = format!("{action} {} -> {}", file.filename, file.path);
                            app.mailbox.attachment_panel.status = Some(message.clone());
                            app.status_message = Some(message);
                        }
                        AsyncResult::AttachmentFile {
                            result: Err(e), ..
                        } => {
                            let message = format!("Attachment error: {e}");
                            app.mailbox.attachment_panel.status = Some(message.clone());
                            app.status_message = Some(message);
                        }
                        AsyncResult::LabelEnvelopes(Ok(envelopes)) => {
                            let selected_id =
                                app.selected_mail_row().map(|row| row.representative.id);
                            // Mask in-flight optimistic state so a daemon
                            // refresh response can't undo an optimistic remove
                            // or flag change before the mutation acks.
                            let mut envelopes = envelopes;
                            app.pending_optimistic.apply(&mut envelopes);
                            app.mailbox.envelopes = envelopes;
                            // Only update active_label when this is a user-initiated
                            // label switch (pending_active_label was set). For
                            // refresh-only fetches triggered by sync or mutations,
                            // pending_active_label is None — preserve current label.
                            if app.mailbox.pending_active_label.is_some() {
                                app.mailbox.active_label = app.mailbox.pending_active_label.take();
                                // A selection made in the old list while this
                                // fetch was in flight must not follow into the new one.
                                app.clear_selection();
                            }
                            restore_mail_list_selection(&mut app, selected_id);
                            app.queue_body_window();
                            app.mailbox.pending_commitment_counts_refresh = true;
                        }
                        AsyncResult::LabelEnvelopes(Err(e)) => {
                            app.mailbox.pending_active_label = None;
                            app.status_message = Some(format!("Label filter failed: {e}"));
                        }
                        AsyncResult::Bodies {
                            requested,
                            result: Ok((bodies, failures)),
                        } => {
                            app.resolve_body_batch(requested, bodies, failures);
                        }
                        AsyncResult::Bodies { requested, result: Err(e) } => {
                            let message = e.to_string();
                            for message_id in requested {
                                app.resolve_body_fetch_error(&message_id, message.clone());
                            }
                        }
                        AsyncResult::HtmlImageAssets {
                            message_id,
                            allow_remote,
                            result: Ok(assets),
                        } => {
                            app.resolve_html_image_assets_success(
                                message_id,
                                assets,
                                allow_remote,
                            );
                        }
                        AsyncResult::HtmlImageAssets {
                            message_id,
                            result: Err(error),
                            ..
                        } => {
                            app.resolve_html_image_assets_error(&message_id, error.to_string());
                        }
                        AsyncResult::HtmlImageDecoded { key, result: Ok(image) } => {
                            if let Some(support) = app.html_image_support.as_ref() {
                                let protocol =
                                    support.build_protocol(image, key.clone(), result_tx.clone());
                                app.resolve_html_image_protocol(&key, protocol);
                            } else {
                                app.resolve_html_image_failure(
                                    &key,
                                    "terminal image support unavailable".into(),
                                );
                            }
                        }
                        AsyncResult::HtmlImageDecoded { key, result: Err(error) } => {
                            app.resolve_html_image_failure(&key, error.to_string());
                        }
                        AsyncResult::HtmlImageResized { key, result: Ok(response) } => {
                            app.resolve_html_image_resize(&key, response);
                        }
                        AsyncResult::HtmlImageResized { key, result: Err(error) } => {
                            app.resolve_html_image_failure(&key, error.to_string());
                        }
                        AsyncResult::Thread {
                            thread_id,
                            request_id,
                            result: Ok((thread, messages, summary)),
                        } => {
                            if request_id != app.mailbox.thread_request_id {
                                tracing::trace!(request_id, current_id = app.mailbox.thread_request_id, "tui stale thread dropped");
                                continue;
                            }
                            app.resolve_thread_success(thread, messages, summary);
                            let _ = thread_id;
                        }
                        AsyncResult::Thread {
                            thread_id,
                            request_id,
                            result: Err(e),
                        } => {
                            if request_id != app.mailbox.thread_request_id {
                                tracing::trace!(request_id, current_id = app.mailbox.thread_request_id, "tui stale thread dropped");
                                continue;
                            }
                            app.resolve_thread_fetch_error(&thread_id);
                            app.status_message = Some(format!("Thread load failed: {e}"));
                        }
                        AsyncResult::MutationResult {
                            id,
                            best_effort,
                            retry,
                            sync_request_pending_failure,
                            outcome,
                        } => {
                            app.finish_pending_mutation();
                            match *outcome {
                                Ok(effect) => {
                                    app.finish_compose_mutation(id);
                                    // Daemon ack'd the mutation: discard the rollback
                                    // snapshot — there's no longer anything to revert.
                                    let _ = app.mutation_snapshots.take(id);
                                    // The optimistic change is now authoritative on
                                    // both sides; subsequent refreshes will reflect it
                                    // naturally, so stop masking refresh responses for
                                    // this mutation.
                                    app.pending_optimistic.clear(id);
                                    let show_completion_status = app.pending_mutation_count == 0;
                                    let effect = app.with_done_note(id, effect);
                                    app.apply_mutation_completion(effect, show_completion_status);
                                    // In the invites lens, a completed mutation is an
                                    // RSVP — refetch so the row's RSVP status updates.
                                    if app.mailbox.mailbox_view
                                        == crate::app::MailboxView::CalendarInvites
                                    {
                                        app.mailbox.pending_calendar_invites_refresh = true;
                                    }
                                    // Any completed mutation can move a thread
                                    // on or off the desk or a place (archive,
                                    // reply, snooze).
                                    app.refresh_places();
                                }
                                Err(e) => {
                                    if app.should_retry_mutation_failure(&e) {
                                        if let Some(retry) = retry {
                                            app.schedule_mutation_retry(*retry, &e);
                                            continue;
                                        }
                                    }
                                    if sync_request_pending_failure {
                                        handle_sync_request_failed(&mut app);
                                    }
                                    app.handle_mutation_failure_result(id, best_effort, &e);
                                }
                            }
                        }
                        AsyncResult::ComposeReady(Ok(data)) => {
                            app.note_focus_compose(data.intent, &data.draft_path);
                            let status = run_with_terminal_suspended(&mut terminal, &mut events, || {
                                let editor = mxr_compose::editor::resolve_editor(None);
                                std::process::Command::new(&editor)
                                    .arg(format!("+{}", data.cursor_line))
                                    .arg(&data.draft_path)
                                    .status()
                            });
                            handle_compose_editor_status(&mut app, &data, status, &bg).await;
                        }
                        AsyncResult::ComposeReady(Err(e)) => {
                            app.compose.assist_after_editor = false;
                            app.status_message = Some(format!("Compose error: {e}"));
                        }
                        AsyncResult::DraftEditReady(Ok(data)) => {
                            let status = run_with_terminal_suspended(&mut terminal, &mut events, || {
                                let editor = mxr_compose::editor::resolve_editor(None);
                                std::process::Command::new(&editor)
                                    .arg(&data.path)
                                    .status()
                            });
                            handle_draft_edit_status(&mut app, &data, status, &bg).await;
                        }
                        AsyncResult::DraftEditReady(Err(e)) => {
                            app.status_message = Some(format!("Edit draft error: {e}"));
                        }
                        AsyncResult::ReplyContextWarmed {
                            message_id,
                            generation,
                            reply,
                            reply_all,
                        } => {
                            let reply_ok = (*reply).ok();
                            let reply_all_ok = (*reply_all).ok();
                            // A result from before an address change is stale: a
                            // deferred action then takes the cold path below.
                            let cached = app.compose.cache_warmed_reply_context(
                                generation,
                                message_id.clone(),
                                crate::app::ReplyContextPair {
                                    reply: reply_ok.clone(),
                                    reply_all: reply_all_ok.clone(),
                                },
                            );
                            // Drain a deferred compose action that was
                            // parked waiting on this prewarm. The user
                            // pressed `r`/`a` while the IPC was still
                            // in flight; firing their action now uses
                            // the cached context with no extra IPC.
                            if let Some(deferred) = app.compose.deferred_compose.take() {
                                if deferred.message_id == message_id {
                                    app.status_message = None;
                                    let preloaded = match (cached, deferred.reply_all) {
                                        (false, _) => None,
                                        (true, true) => reply_all_ok,
                                        (true, false) => reply_ok,
                                    };
                                    app.compose.pending_compose = Some(
                                        if deferred.reply_all {
                                            crate::app::ComposeAction::ReplyAll {
                                                message_id: deferred.message_id,
                                                account_id: deferred.account_id,
                                                preloaded,
                                            }
                                        } else {
                                            crate::app::ComposeAction::Reply {
                                                message_id: deferred.message_id,
                                                account_id: deferred.account_id,
                                                preloaded,
                                            }
                                        },
                                    );
                                } else {
                                    // Stale prewarm — the user navigated
                                    // away. Put the deferral back so the
                                    // *next* prewarm for `deferred.message_id`
                                    // (or a fresh keypress) can claim it.
                                    app.compose.deferred_compose = Some(deferred);
                                }
                            }
                        }
                        AsyncResult::ExportResult(Ok(msg)) => {
                            app.status_message = Some(msg);
                        }
                        AsyncResult::ExportResult(Err(e)) => {
                            app.status_message = Some(format!("Export failed: {e}"));
                        }
                        AsyncResult::Unsubscribe(Ok(result)) => {
                            if !result.archived_ids.is_empty() {
                                app.apply_removed_message_ids(&result.archived_ids);
                            }
                            app.status_message = Some(result.message);
                            app.mailbox.pending_subscriptions_refresh = true;
                        }
                        AsyncResult::Unsubscribe(Err(e)) => {
                            app.status_message = Some(format!("Unsubscribe failed: {e}"));
                        }
                        AsyncResult::Subscriptions(Ok(subscriptions)) => {
                            app.set_subscriptions(subscriptions);
                        }
                        AsyncResult::Subscriptions(Err(e)) => {
                            app.status_message = Some(format!("Subscriptions error: {e}"));
                        }
                        AsyncResult::OwedReplies(Ok((rows, people))) => {
                            if rows.is_empty() && !app.mailbox.owed_page.entries.is_empty() {
                                // Say what the Focus finish says: it is everyone you
                                // write with, and where the others are.
                                app.status_message = Some(match people {
                                    Some(people) if people.others > 0 => format!(
                                        "Low tide. That's everyone you write with. {} who wrote to you {} in Messages.",
                                        if people.others == 1 {
                                            "1 other".to_string()
                                        } else {
                                            format!("{} others", people.others)
                                        },
                                        if people.others == 1 { "is" } else { "are" },
                                    ),
                                    _ => crate::app::LOW_TIDE.to_string(),
                                });
                            }
                            app.mailbox.owed_page.entries = rows;
                            app.mailbox.owed_page.people = people;
                            app.mailbox.selected_index = app
                                .mailbox
                                .selected_index
                                .min(app.mailbox.owed_page.entries.len().saturating_sub(1));
                        }
                        AsyncResult::OwedReplies(Err(e)) => {
                            app.status_message = Some(format!("Owed replies error: {e}"));
                        }
                        AsyncResult::Place(fetch, Ok(page)) => app.set_place(&fetch, page),
                        AsyncResult::Place(_, Err(e)) => {
                            app.status_message = Some(format!("Couldn't load the place: {e}"));
                        }
                        AsyncResult::SweepPreview(Ok(confirm)) => app.show_sweep_preview(confirm),
                        AsyncResult::SweepPreview(Err(e)) => {
                            app.status_message = Some(format!("Couldn't preview the sweep: {e}"));
                        }
                        AsyncResult::PlaceSwept(Ok((archived, undo_ids))) => {
                            app.finish_sweep(archived, undo_ids);
                        }
                        AsyncResult::PlaceSwept(Err(e)) => {
                            app.refresh_places();
                            app.status_message = Some(format!("Sweep stopped: {e}"));
                        }
                        AsyncResult::Now(Ok(loaded)) => {
                            let (now, guide) = *loaded;
                            app.set_now(now, guide);
                        }
                        AsyncResult::Now(Err(e)) => {
                            app.status_message = Some(format!("Couldn't load Now: {e}"));
                        }
                        AsyncResult::Arrivals(Ok(arrivals)) => app.set_arrivals(*arrivals),
                        // Now still works without its line.
                        AsyncResult::Arrivals(Err(e)) => {
                            tracing::warn!(error = %e, "arrivals line unavailable");
                        }
                        AsyncResult::ArrivalsList(fetch, Ok(list)) => {
                            app.set_arrivals_list(&fetch, *list);
                        }
                        AsyncResult::ArrivalsList(_, Err(e)) => {
                            app.status_message = Some(format!("Couldn't list them: {e}"));
                        }
                        AsyncResult::ArrivalModes(result) => {
                            app.mailbox.trust.chips_in_flight = false;
                            match result {
                                Ok(items) => app.mailbox.trust.set_chips(items),
                                // Asked ids stay asked: a failing daemon is
                                // not asked again every frame.
                                Err(e) => tracing::warn!(error = %e, "mode chips unavailable"),
                            }
                        }
                        AsyncResult::Moved(outcome, not_sure) => {
                            app.after_move(&outcome, not_sure);
                        }
                        AsyncResult::Messages(Ok(loaded)) => {
                            let (messages, guide) = *loaded;
                            app.set_messages(messages, guide);
                        }
                        AsyncResult::Messages(Err(e)) => {
                            app.status_message = Some(format!("Couldn't load Messages: {e}"));
                        }
                        AsyncResult::PersonPage(row_id, topic, Ok(page)) => {
                            app.set_person_page(row_id, topic, *page);
                        }
                        AsyncResult::PersonPage(_, _, Err(e)) => {
                            app.status_message = Some(format!("Couldn't load the person: {e}"));
                        }
                        AsyncResult::MessagesAck(Ok(plan)) => {
                            app.show_messages_ack(*plan, std::time::Instant::now());
                        }
                        AsyncResult::MessagesAck(Err(e)) => {
                            app.status_message = Some(format!("Got it isn't possible here: {e}"));
                        }
                        AsyncResult::Rail(Ok(rail)) => app.mailbox.rail = Some(rail),
                        AsyncResult::Rail(Err(e)) => {
                            tracing::debug!(error = %e, "rail fetch failed");
                        }
                        AsyncResult::NowDigestPreview(Ok(preview)) => {
                            app.show_now_digest_preview(preview);
                        }
                        AsyncResult::UpdatesLetGoPreview(Ok(preview)) => {
                            app.show_updates_let_go_preview(preview);
                        }
                        AsyncResult::LatestCodes(Ok(codes)) => app.copy_latest_code(&codes),
                        AsyncResult::LatestCodes(Err(e)) => {
                            app.status_message = Some(format!("Couldn't find a code: {e}"));
                        }
                        AsyncResult::NowDigestPreview(Err(e))
                        | AsyncResult::UpdatesLetGoPreview(Err(e)) => {
                            app.status_message = Some(format!("Couldn't preview: {e}"));
                        }
                        AsyncResult::ReadingEdition(Ok(loaded)) => {
                            let (edition, guide) = *loaded;
                            app.set_reading_edition(edition, guide);
                        }
                        AsyncResult::ReadingEdition(Err(e)) => {
                            app.status_message = Some(format!("Couldn't load Reading: {e}"));
                        }
                        AsyncResult::ReadingItem(key, Ok(detail)) => {
                            app.set_reading_item(&key, *detail);
                        }
                        AsyncResult::ReadingItem(key, Err(e)) => {
                            app.reading_item_failed(&key, &e.to_string());
                        }
                        AsyncResult::ReadingArticle(key, Ok(fetch)) => {
                            app.set_reading_article(&key, fetch);
                            app.refresh_reading();
                        }
                        AsyncResult::ReadingArticle(_, Err(e)) => {
                            app.status_message = Some(format!("Couldn't fetch the article: {e}"));
                        }
                        AsyncResult::ReadingLetGoPreview(thread_ids, Ok(items)) => {
                            app.show_reading_let_go_preview(&thread_ids, items);
                        }
                        AsyncResult::ReadingUnsubscribePreview(target, Ok(result)) => {
                            app.show_reading_unsubscribe_preview(target, *result);
                        }
                        AsyncResult::ReadingLetGoPreview(_, Err(e))
                        | AsyncResult::ReadingUnsubscribePreview(_, Err(e)) => {
                            app.status_message = Some(format!("Couldn't preview: {e}"));
                        }
                        AsyncResult::ReadingOriginal(Ok(text)) => app.set_reading_original(text),
                        AsyncResult::ReadingOriginal(Err(e)) => {
                            app.status_message = Some(format!("Couldn't read the email: {e}"));
                        }
                        AsyncResult::Updates(Ok(loaded)) => {
                            let (digest, guide) = *loaded;
                            app.set_updates_digest(digest, guide);
                        }
                        AsyncResult::Updates(Err(e)) => {
                            app.status_message = Some(format!("Couldn't load Updates: {e}"));
                        }
                        AsyncResult::TodoRunway(Ok((runway, guide))) => {
                            app.set_todo_runway(runway, guide);
                        }
                        AsyncResult::TodoRunway(Err(e)) => {
                            app.status_message = Some(format!("Couldn't load To do: {e}"));
                        }
                        AsyncResult::TodoExpired(Ok(todos)) => app.set_todo_expired(todos),
                        AsyncResult::MicrosoftTodo(Ok(answer)) => app.microsoft_todo_answer(*answer),
                        AsyncResult::MicrosoftTodo(Err(error)) => {
                            app.mailbox.todo_page.microsoft_busy = false;
                            app.mailbox.todo_page.microsoft_preview = None;
                            app.status_message = Some(format!("Microsoft To Do: {error}"));
                        }
                        AsyncResult::TodoCatchup(Ok(catchup)) => app.set_todo_catchup(catchup),
                        AsyncResult::TodoExpired(Err(e)) | AsyncResult::TodoCatchup(Err(e)) => {
                            app.status_message = Some(format!("Couldn't load the list: {e}"));
                        }
                        AsyncResult::TodoCatchupPreview(Ok(preview)) => {
                            app.show_catchup_preview(preview);
                        }
                        AsyncResult::TodoCatchupPreview(Err(e)) => {
                            app.status_message = Some(format!("Couldn't preview: {e}"));
                        }
                        AsyncResult::TodoResolutionPreview(todo_id, suggestion_id, Ok(preview)) => {
                            app.show_resolution_preview(todo_id, suggestion_id, preview);
                        }
                        AsyncResult::TodoResolutionPreview(_, _, Err(e)) => {
                            app.status_message = Some(format!("Couldn't review that resolution: {e}"));
                        }
                        AsyncResult::Desk(Ok(desk)) => app.set_desk(desk),
                        AsyncResult::Desk(Err(e)) => {
                            app.status_message = Some(format!("Desk error: {e}"));
                        }
                        AsyncResult::CalendarInvites(Ok(invites)) => {
                            app.mailbox.calendar_invites_page.entries = invites;
                            app.mailbox.selected_index = app.mailbox.selected_index.min(
                                app.mailbox
                                    .calendar_invites_page
                                    .entries
                                    .len()
                                    .saturating_sub(1),
                            );
                        }
                        AsyncResult::CalendarInvites(Err(e)) => {
                            app.status_message = Some(format!("Calendar invites error: {e}"));
                        }
                        AsyncResult::RecordsLedger(Ok(loaded)) => {
                            let (ledger, guide) = *loaded;
                            app.set_records_ledger(ledger, guide);
                        }
                        AsyncResult::RecordsLedger(Err(e)) => {
                            app.status_message = Some(format!("Couldn't load Archive: {e}"));
                        }
                        AsyncResult::RecordsAnswer(Ok(answer)) => {
                            app.status_message = None;
                            app.set_records_answer(*answer);
                        }
                        AsyncResult::RecordCard(Ok(record)) => app.set_record_card(*record),
                        AsyncResult::RecordSubscriptions(Ok(data)) => {
                            app.set_record_subscriptions(*data);
                        }
                        AsyncResult::RecordsExport(Ok(export)) => app.show_export(*export),
                        AsyncResult::RecordFixPreview(Ok(preview)) => app.show_fix_preview(*preview),
                        AsyncResult::RecordFixPreview(Err(e)) => app.show_fix_error(e.to_string()),
                        AsyncResult::RecordFilePreview(Ok(preview)) => {
                            app.show_file_preview(*preview);
                        }
                        AsyncResult::RecordsAnswer(Err(e))
                        | AsyncResult::RecordCard(Err(e))
                        | AsyncResult::RecordSubscriptions(Err(e))
                        | AsyncResult::RecordsExport(Err(e))
                        | AsyncResult::RecordFilePreview(Err(e)) => {
                            app.status_message = Some(format!("Archive: {e}"));
                        }
                        AsyncResult::RecordEnvelopeOpened(Ok(envelope)) => {
                            app.open_record_envelope(envelope);
                        }
                        AsyncResult::RecordEnvelopeOpened(Err(e)) => {
                            app.status_message = Some(format!("Open email failed: {e}"));
                        }
                        AsyncResult::TodoEnvelopeOpened(Ok(envelope), link) => {
                            app.open_todo_envelope(envelope, link);
                        }
                        AsyncResult::TodoEnvelopeOpened(Err(e), _) => {
                            app.status_message = Some(format!("Open email failed: {e}"));
                        }
                        AsyncResult::InviteEnvelopeOpened(Ok(envelope)) => {
                            app.open_invite_envelope(envelope);
                        }
                        AsyncResult::InviteEnvelopeOpened(Err(e)) => {
                            app.status_message = Some(format!("Open invite failed: {e}"));
                        }
                        AsyncResult::Briefing(Ok(briefing)) => {
                            app.modals.briefing.set_briefing(
                                briefing.body_markdown,
                                briefing.citations,
                                briefing.generated_at,
                                briefing.from_cache,
                            );
                        }
                        AsyncResult::Briefing(Err(e)) => {
                            app.modals.briefing.set_error(e.to_string());
                        }
                        AsyncResult::Whois(Ok(entity)) => {
                            app.modals.whois.set_entity(entity);
                        }
                        AsyncResult::Whois(Err(e)) => {
                            app.modals.whois.set_error(e.to_string());
                        }
                        AsyncResult::Expert(Ok(experts)) => {
                            app.modals.expert.set_experts(experts);
                        }
                        AsyncResult::Expert(Err(e)) => {
                            app.modals.expert.set_error(e.to_string());
                        }
                        AsyncResult::CommitmentCounts(counts) => {
                            app.mailbox.open_commitment_counts = counts;
                        }
                        AsyncResult::WorkspaceOwner(owner) => { if app.compose.workspace_owner.is_none() { app.compose.workspace_owner = Some(owner); } }
                        AsyncResult::ConnectionState(state) => {
                            app.set_connection_state(state);
                        }
                        AsyncResult::ReportedError(entry) => {
                            app.push_reported_error(entry);
                        }
                        AsyncResult::UndoCaptured(undo) => {
                            app.set_pending_undo(undo);
                        }
                        AsyncResult::SyncBatch(results) => {
                            handle_sync_batch(&mut app, results);
                        }
                        AsyncResult::SavedSearchMutation(Ok(())) => {
                            // Trigger a follow-up list refresh on the next
                            // dispatcher tick. We don't refresh inline so
                            // pipelined edits (delete+create) update the
                            // sidebar with the final state, not mid-flight.
                            app.modals.pending_saved_search_refresh = true;
                        }
                        AsyncResult::SavedSearchMutation(Err(e)) => {
                            app.report_error(
                                "Saved search failed",
                                format!("Could not save: {e}"),
                            );
                        }
                        AsyncResult::SavedSearchListRefreshed(Ok(searches)) => {
                            app.mailbox.saved_searches = searches;
                            // Chain a follow-up to refresh the per-tab
                            // unread counts so the strip badges stay
                            // in sync with what the user just changed.
                            let bg_inner = bg.clone();
                            let _ = submit_task(&queued, async move {
                                let resp = ipc_call(
                                    &bg_inner,
                                    Request::ListSavedSearchUnreadCounts,
                                )
                                .await;
                                let result = match resp {
                                    Ok(Response::Ok {
                                        data: ResponseData::SavedSearchUnreadCounts { counts },
                                    }) => Ok(counts),
                                    Ok(Response::Error { message, .. }) => {
                                        Err(MxrError::Ipc(message))
                                    }
                                    Err(e) => Err(e),
                                    _ => Err(MxrError::Ipc("unexpected response to ListSavedSearchUnreadCounts".into())),
                                };
                                AsyncResult::SavedSearchUnreadCountsRefreshed(result)
                            });
                        }
                        AsyncResult::SavedSearchListRefreshed(Err(e)) => {
                            app.report_warn(format!("Could not refresh saved searches: {e}"));
                        }
                        AsyncResult::SavedSearchUnreadCountsRefreshed(Ok(counts)) => {
                            app.mailbox.saved_search_unread_counts = counts;
                        }
                        AsyncResult::SavedSearchUnreadCountsRefreshed(Err(e)) => {
                            // Non-fatal — bare labels render fine
                            // without counts. Log so the user sees the
                            // failure but keep the strip visible.
                            tracing::debug!(error = %e, "saved-search unread counts refresh failed");
                        }
                        AsyncResult::SemanticOperationResult(Ok(())) => {
                            // Don't overwrite a fresh `Sent!`-style success
                            // status from a competing flow; only clear if we
                            // were the one who set it ("Enabling..." etc).
                            if let Some(status) = app.status_message.as_deref() {
                                if status.starts_with("Enabling")
                                    || status.starts_with("Disabling")
                                    || status.starts_with("Reindexing")
                                    || status.starts_with("Backfilling")
                                    || status.starts_with("Installing semantic profile")
                                {
                                    app.status_message = Some("Semantic action complete".into());
                                }
                            }
                        }
                        AsyncResult::SemanticOperationResult(Err(e)) => {
                            app.report_error(
                                "Semantic action failed",
                                format!("The daemon returned: {e}"),
                            );
                        }
                        AsyncResult::PlatformModalLoaded { compose, title, mut result } => {
                            if let Some(pending) = compose {
                                let pending = *pending;
                                result = result.map(|body| format!("{body}\n\nQuery scheduled delivery with draft:{} for account {}", pending.draft_id(), pending.account_id));
                                app.finish_compose_schedule(pending, result.is_ok());
                            }
                            app.modals.platform.title = title;
                            match result {
                                Ok(body) => app.modals.platform.set_body(body),
                                Err(e) => app.modals.platform.set_error(e.to_string()),
                            }
                        }
                        AsyncResult::ComposeAssistLoaded { pending, model, request_id, result } => {
                            let current_matches = app.compose.pending_send_confirm.as_ref()
                                .is_some_and(|current| current.draft_path == pending.draft_path);
                            if !current_matches
                                || !app.modals.platform.visible
                                || app.compose.assist_request_id != request_id
                            {
                                continue;
                            }
                            app.modals.platform.close();
                            match result {
                                Ok(proposal) => {
                                    if let Some(review) = app.compose.assist_review.as_mut().filter(|review| {
                                        review.pending.draft_path == pending.draft_path
                                    }) {
                                        review.push_proposal(proposal, model);
                                    } else {
                                        app.compose.assist_review = Some(app::ComposeAssistReview {
                                            pending: *pending,
                                            proposals: vec![proposal],
                                            proposal_models: vec![model],
                                            current_index: 0,
                                            note_input: None,
                                            proof_scroll_offset: 0,
                                            request_pending: false,
                                        });
                                    }
                                }
                                Err(error) => {
                                    if let Some(review) = app.compose.assist_review.as_mut() {
                                        review.request_pending = false;
                                    }
                                    app.report_error("Writing proof failed", error.to_string());
                                }
                            }
                        }
                        AsyncResult::ComposeModelsLoaded { pending, purpose, result } => {
                            if !app.modals.platform.visible
                                || app.modals.platform.title != "Choose a writing model"
                            {
                                continue;
                            }
                            app.modals.platform.close();
                            let still_owned = match purpose {
                                app::ComposeModelPickerPurpose::InitialDraft => app
                                    .compose
                                    .pending_send_confirm
                                    .as_ref()
                                    .is_some_and(|current| {
                                        current.draft_path == pending.draft_path
                                    }),
                                app::ComposeModelPickerPurpose::RetryProof => app
                                    .compose
                                    .assist_review
                                    .as_ref()
                                    .is_some_and(|review| {
                                        review.pending.draft_path == pending.draft_path
                                            && !review.request_pending
                                    }),
                            };
                            if !still_owned {
                                continue;
                            }
                            match result {
                                Ok(choices) => {
                                    app.compose.model_picker = Some(app::ComposeModelPicker {
                                        choices,
                                        selected_index: 0,
                                        pending,
                                        purpose,
                                    });
                                }
                                Err(error) => app.report_error(
                                    "Compose models unavailable",
                                    error.to_string(),
                                ),
                            }
                        }
                        AsyncResult::ComposeAssistAccepted { pending, request_id, reopen_editor, result } => {
                            if app.compose.assist_request_id != request_id {
                                continue;
                            }
                            app.modals.platform.close();
                            match result {
                                Ok((draft, safety)) => {
                                    let Some(body) = draft.content.markdown_source().map(str::to_owned) else {
                                        app.report_error("Accepted draft cannot be edited", "The stored draft is not Markdown.");
                                        continue;
                                    };
                                    let mut pending_send = *pending;
                                    pending_send.body = body;
                                    pending_send.draft_revision = draft.revision;
                                    pending_send.mode = app::PendingSendMode::SendOrSave;
                                    match safety {
                                        Ok(report) => {
                                            pending_send.override_token = report.issues.iter().find_map(|issue| issue.override_token.clone());
                                            pending_send.safety_report = Some(report);
                                            pending_send.safety_check_failed = None;
                                        }
                                        Err(error) => {
                                            pending_send.override_token = None;
                                            pending_send.safety_report = None;
                                            pending_send.safety_check_failed = Some(error.to_string());
                                        }
                                    }
                                    let content = match mxr_compose::draft_codec::draft_to_compose_file(&draft, &pending_send.fm.from) {
                                        Ok(content) => content,
                                        Err(error) => {
                                            app.compose.pending_send_confirm = Some(pending_send);
                                            app.report_error("Accepted draft cannot be reopened", error.to_string());
                                            continue;
                                        }
                                    };
                                    if let Err(error) = std::fs::write(&pending_send.draft_path, content) {
                                        app.compose.pending_send_confirm = Some(pending_send);
                                        app.report_error("Accepted draft file could not be updated", error.to_string());
                                        continue;
                                    }
                                    #[cfg(not(test))]
                                    if let Err(error) = crate::send_recovery_state::retain_reviewed_compose(
                                        &draft.account_id,
                                        &draft.id,
                                        &mxr_core::SendIdentity::for_draft(&draft),
                                        &pending_send.draft_path,
                                    ) {
                                        app.compose.pending_send_confirm = Some(pending_send);
                                        app.report_error("Accepted draft could not be tracked", error.to_string());
                                        continue;
                                    }
                                    if reopen_editor {
                                        app.compose.pending_send_confirm = None;
                                        app.compose.pending_compose = Some(app::ComposeAction::EditDraft {
                                            path: pending_send.draft_path,
                                            account_id: pending_send.account_id,
                                        });
                                    } else {
                                        app.compose.pending_send_confirm = Some(pending_send);
                                    }
                                }
                                Err(error) => app.report_error(
                                    "Could not confirm the accepted draft",
                                    format!(
                                        "The change may already be saved. Reopen the draft to check its current text before continuing. {error}"
                                    ),
                                ),
                            }
                        }
                        AsyncResult::AnalyticsResult { view, result } => {
                            // Drop responses for a view the user already
                            // navigated away from. Avoids the "table
                            // briefly flashes the previous view's data"
                            // race when switching tabs quickly.
                            if app.analytics.view != view {
                                continue;
                            }
                            app.analytics.loading = false;
                            let was_ok = result.is_ok();
                            match *result {
                                Ok(crate::async_result::AnalyticsResultPayload::Storage(rows)) => {
                                    app.analytics.storage_rows = rows;
                                }
                                Ok(
                                    crate::async_result::AnalyticsResultPayload::LargestMessages(
                                        rows,
                                    ),
                                ) => {
                                    app.analytics.largest_message_rows = rows;
                                }
                                Ok(crate::async_result::AnalyticsResultPayload::Stale(rows)) => {
                                    app.analytics.stale_rows = rows;
                                }
                                Ok(crate::async_result::AnalyticsResultPayload::Asymmetry(
                                    rows,
                                )) => {
                                    app.analytics.asymmetry_rows = rows;
                                }
                                Ok(crate::async_result::AnalyticsResultPayload::Decay(rows)) => {
                                    app.analytics.decay_rows = rows;
                                }
                                Ok(crate::async_result::AnalyticsResultPayload::CadenceDrift(
                                    rows,
                                )) => {
                                    app.analytics.cadence_drift_rows = rows;
                                }
                                Ok(crate::async_result::AnalyticsResultPayload::ResponseTime(
                                    summary,
                                )) => {
                                    app.analytics.response_time = Some(summary);
                                }
                                Ok(
                                    crate::async_result::AnalyticsResultPayload::Subscriptions(
                                        rows,
                                    ),
                                ) => {
                                    app.analytics.subscriptions = rows;
                                }
                                Ok(
                                    crate::async_result::AnalyticsResultPayload::SearchAggregation(
                                        rows,
                                    ),
                                ) => {
                                    app.analytics.search_aggregation_rows = rows;
                                }
                                Ok(crate::async_result::AnalyticsResultPayload::Wrapped(
                                    summary,
                                )) => {
                                    app.analytics.wrapped = Some(*summary);
                                }
                                Ok(
                                    crate::async_result::AnalyticsResultPayload::ContactsRefreshed {
                                        rows,
                                    },
                                ) => {
                                    app.status_message =
                                        Some(format!("Contacts refreshed: {rows} rows"));
                                    // Re-fire the active contacts sub-view so the
                                    // user immediately sees the recomputed numbers.
                                    if app.analytics.view == crate::app::AnalyticsView::Contacts {
                                        app.analytics.refresh_pending = true;
                                    }
                                }
                                Err(e) => {
                                    app.analytics.error = Some(e.to_string());
                                    app.report_warn(format!(
                                        "Analytics request failed: {e}"
                                    ));
                                }
                            }
                            if was_ok {
                                app.analytics.mark_refreshed();
                            }
                        }
                        AsyncResult::Assistant { call, result } => {
                            app.apply_assistant_result(call, *result);
                        }
                        other => {
                            let Some(other) = handle_local_io_result(&mut app, other) else {
                                continue;
                            };
                            if let AsyncResult::DaemonEvent(event) = other {
                                handle_daemon_event(&mut app, event);
                            }
                        }
                    }
                }
            }
            _ = tokio::time::sleep(timeout) => {
                app.tick();
                let now = std::time::Instant::now();
                app.tick_connection_state(now);
                app.tick_pending_undo(now);
                app.tick_pending_invite_send(now);
                app.tick_messages_ack(now);
                app.tick_promise_prompts(now);
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

/// From the daemon's "undo: partly done: restored N, M failed and can be
/// retried" refusal, how many are left to retry.
fn undo_retry_count(message: &str) -> Option<u32> {
    let rest = message.strip_prefix("undo: partly done: restored ")?;
    let (_, rest) = rest.split_once(", ")?;
    let (failed, rest) = rest.split_once(' ')?;
    rest.starts_with("failed and can be retried")
        .then(|| failed.parse().ok())
        .flatten()
}

/// How often a sweep checks on its archive job.
const SWEEP_JOB_POLL: std::time::Duration = std::time::Duration::from_millis(300);

pub(crate) fn place_request(fetch: &crate::app::PlaceFetch) -> Request {
    use crate::app::{PlaceFetch, PLACE_PAGE_MESSAGES, PLACE_PAGE_SENDERS};
    match fetch {
        PlaceFetch::Refresh {
            place,
            senders,
            messages_per_sender,
        } => Request::ListPlace {
            place: *place,
            account_id: None,
            sender_email: None,
            limit: *senders,
            offset: 0,
            messages_per_bundle: *messages_per_sender,
            message_offset: 0,
        },
        PlaceFetch::MoreSenders { place, offset } => Request::ListPlace {
            place: *place,
            account_id: None,
            sender_email: None,
            limit: PLACE_PAGE_SENDERS,
            offset: *offset,
            messages_per_bundle: PLACE_PAGE_MESSAGES,
            message_offset: 0,
        },
        PlaceFetch::MoreFromSender {
            place,
            account_id,
            sender_email,
            message_offset,
        } => Request::ListPlace {
            place: *place,
            account_id: Some(account_id.clone()),
            sender_email: Some(sender_email.clone()),
            limit: 1,
            offset: 0,
            messages_per_bundle: PLACE_PAGE_MESSAGES,
            message_offset: *message_offset,
        },
    }
}

pub(crate) fn sweep_request(
    target: &crate::app::SweepTarget,
    dry_run: bool,
    preview_token: Option<String>,
) -> Request {
    Request::SweepPlace {
        place: target.place,
        account_id: target.account_id.clone(),
        sender_email: target.sender_email.clone(),
        dry_run,
        preview_token,
    }
}

/// Run a confirmed sweep over exactly its preview, and wait for the archive
/// job so undo can cover every chunk it archived.
async fn run_sweep(
    bg: &mpsc::UnboundedSender<IpcRequest>,
    confirm: crate::app::PendingSweepConfirm,
) -> Result<(u32, Vec<String>), MxrError> {
    let request = sweep_request(
        &confirm.target,
        false,
        confirm.preview.preview_token.clone(),
    );
    let mut job = match ipc_call(bg, request).await? {
        Response::Ok {
            data: ResponseData::PlaceSwept { job: Some(job), .. },
        } => job,
        Response::Ok {
            data: ResponseData::PlaceSwept { job: None, .. },
        } => return Ok((0, Vec::new())),
        Response::Error { message, .. } => return Err(MxrError::Ipc(message)),
        Response::Ok { .. } => {
            return Err(MxrError::Ipc("unexpected response to SweepPlace".into()))
        }
    };
    while matches!(
        job.status,
        mxr_protocol::JobStatusData::Queued | mxr_protocol::JobStatusData::Running
    ) {
        tokio::time::sleep(SWEEP_JOB_POLL).await;
        job = match ipc_call(
            bg,
            Request::GetJob {
                job_id: job.job_id.clone(),
            },
        )
        .await?
        {
            Response::Ok {
                data: ResponseData::Job { job },
            } => job,
            Response::Error { message, .. } => return Err(MxrError::Ipc(message)),
            Response::Ok { .. } => {
                return Err(MxrError::Ipc("unexpected response to GetJob".into()))
            }
        };
    }
    match job.error {
        // Part of it may have landed: keep its undo reachable by reporting
        // what succeeded rather than failing outright.
        Some(error) if job.progress.succeeded == 0 => Err(MxrError::Ipc(error)),
        _ => Ok((job.progress.succeeded, job.undo_ids)),
    }
}

/// The desk request the TUI sends: every account, the daemon's default
/// lane size. Shared by the runtime and its tests so they cannot drift.
pub(crate) fn desk_request() -> Request {
    Request::GetDesk {
        account_id: None,
        lane_limit: 25,
    }
}

#[cfg(test)]
mod tests;

/// What a done-here answer means for the TUI: the toast in the daemon's
/// handoff words with the undo it offers, or the error when nothing was
/// done. A thread done could not let go keeps its row (the failure
/// refetches Now), and `e` retries it.
pub(crate) fn mode_done_outcome(
    items: &[mxr_protocol::ModeDoneOutcomeData],
    mutation_id: Option<String>,
    undo_unavailable: bool,
) -> (
    Result<app::MutationEffect, MxrError>,
    Option<app::PendingUndo>,
) {
    let done = items.iter().filter(|item| item.error.is_none()).count();
    let mode = items
        .first()
        .map_or(mxr_protocol::ModeKindData::Messages, |item| item.mode);
    let mut copy = app::mode_done_copy(items, mode);
    let undo = mutation_id.map(|daemon_mutation_id| app::PendingUndo {
        action: app::UndoAction::Mutations(vec![daemon_mutation_id]),
        verb_past: "Done".into(),
        count: u32::try_from(done).unwrap_or(u32::MAX),
        applied_at: std::time::Instant::now(),
    });
    if !copy.is_empty() {
        if undo.is_some() {
            copy.push_str(" u to undo");
        } else if undo_unavailable {
            copy.push_str(" Its undo couldn't be saved.");
        }
    }
    let effect = match items.iter().find_map(|item| item.error.as_deref()) {
        Some(error) if done == 0 => Err(MxrError::Ipc(format!("Not done: {error}"))),
        _ => Ok(app::MutationEffect::ModeDone(copy)),
    };
    (effect, undo)
}

/// The list Messages asks for: every account, the picker's filter, the
/// default page size.
pub(crate) fn list_messages_request(filter: mxr_protocol::MessagesFilterData) -> Request {
    Request::ListMessages {
        account_id: None,
        turn: None,
        filter,
        limit: 50,
    }
}

/// Messages' bands and its guide, fetched together so the header, the card
/// and the rows arrive at once. A guide error leaves the rows on screen.
async fn fetch_messages(
    bg: &mpsc::UnboundedSender<IpcRequest>,
    filter: mxr_protocol::MessagesFilterData,
) -> Result<
    (
        mxr_protocol::MessagesData,
        Option<mxr_protocol::ModeGuideData>,
    ),
    MxrError,
> {
    let (messages, guide) = tokio::join!(
        ipc_call(bg, list_messages_request(filter)),
        ipc_call(
            bg,
            Request::GetModeGuide {
                mode: Some(crate::app::MESSAGES_MODE.into()),
            },
        ),
    );
    let messages = match messages {
        Ok(Response::Ok {
            data: ResponseData::Messages { messages },
        }) => messages,
        Ok(Response::Error { message, .. }) => return Err(MxrError::Ipc(message)),
        Err(e) => return Err(e),
        _ => return Err(MxrError::Ipc("unexpected response to ListMessages".into())),
    };
    let guide = match guide {
        Ok(Response::Ok {
            data: ResponseData::ModeGuides { mut guides },
        }) if !guides.is_empty() => Some(guides.remove(0)),
        _ => None,
    };
    Ok((messages, guide))
}

/// What letting go of a digest did: the daemon's line, undo, or why it
/// refused (the cut changed since the preview).
pub(crate) fn updates_let_go_outcome(
    result: &mxr_protocol::UpdatesLetGoData,
) -> (
    Result<app::MutationEffect, MxrError>,
    Option<app::PendingUndo>,
) {
    let done = result
        .items
        .iter()
        .filter(|item| item.error.is_none())
        .count();
    if done == 0 && !result.items.is_empty() {
        let error = result
            .items
            .iter()
            .find_map(|item| item.error.as_deref())
            .unwrap_or("nothing changed");
        return (Err(MxrError::Ipc(format!("Not let go: {error}"))), None);
    }
    let undo = result
        .mutation_id
        .clone()
        .map(|daemon_mutation_id| app::PendingUndo {
            action: app::UndoAction::Mutations(vec![daemon_mutation_id]),
            verb_past: "Let go".into(),
            count: result.message_count,
            applied_at: std::time::Instant::now(),
        });
    let mut copy = crate::ui::sanitize::one_line(&result.line);
    if undo.is_some() {
        copy.push_str(" u to undo");
    } else if result.undo_unavailable {
        copy.push_str(" Its undo couldn't be saved.");
    }
    (Ok(app::MutationEffect::ModeDone(copy)), undo)
}

/// The TUI's pin request names one exact latest message. The daemon reports
/// whether it actually changed; a no-op refreshes the digest without
/// replacing a still-valid undo from an earlier action.
pub(crate) fn updates_pin_outcome(
    message_id: mxr_core::MessageId,
    prior_pinned: bool,
    status: String,
    changed: u32,
) -> (app::MutationEffect, Option<app::PendingUndo>) {
    let undo = (changed > 0).then(|| app::PendingUndo {
        action: app::UndoAction::UpdatesPin {
            message_id,
            pinned: prior_pinned,
        },
        verb_past: if prior_pinned { "Unpinned" } else { "Pinned" }.into(),
        count: 1,
        applied_at: std::time::Instant::now(),
    });
    (app::MutationEffect::RefreshUpdates(status), undo)
}

/// The daemon's dry run of letting go of the digest at `cut`, or of one
/// account's source in it.
async fn preview_let_go(
    bg: &mpsc::UnboundedSender<IpcRequest>,
    cut: Option<chrono::DateTime<chrono::Utc>>,
    source: Option<(mxr_core::id::AccountId, String)>,
) -> Result<mxr_protocol::UpdatesLetGoData, MxrError> {
    let (account_id, source_key) = source.unzip();
    let resp = ipc_call(
        bg,
        Request::LetGoDigest {
            account_id,
            cut,
            source_key,
            selection_token: None,
            dry_run: true,
        },
    )
    .await;
    match resp {
        Ok(Response::Ok {
            data: ResponseData::UpdatesLetGo { result },
        }) => Ok(result),
        Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
        Err(e) => Err(e),
        _ => Err(MxrError::Ipc("unexpected response to LetGoDigest".into())),
    }
}

/// The Updates digest (marking Updates seen) and its guide, together.
async fn fetch_updates(
    bg: &mpsc::UnboundedSender<IpcRequest>,
) -> Result<
    (
        mxr_protocol::UpdatesDigestData,
        Option<mxr_protocol::ModeGuideData>,
    ),
    MxrError,
> {
    let (digest, guide) = tokio::join!(
        ipc_call(
            bg,
            Request::GetUpdatesDigest {
                account_id: None,
                cut: None,
                mark_seen: true,
                expired: false,
            },
        ),
        ipc_call(
            bg,
            Request::GetModeGuide {
                mode: Some(crate::app::UPDATES_MODE.into()),
            },
        ),
    );
    let digest = match digest {
        Ok(Response::Ok {
            data: ResponseData::UpdatesDigest { digest },
        }) => digest,
        Ok(Response::Error { message, .. }) => return Err(MxrError::Ipc(message)),
        Err(e) => return Err(e),
        _ => {
            return Err(MxrError::Ipc(
                "unexpected response to GetUpdatesDigest".into(),
            ))
        }
    };
    let guide = match guide {
        Ok(Response::Ok {
            data: ResponseData::ModeGuides { mut guides },
        }) if !guides.is_empty() => Some(guides.remove(0)),
        _ => None,
    };
    Ok((digest, guide))
}

/// Now and its guide, fetched together so the header, the card and the
/// rows arrive at once. A guide error leaves the rows on screen.
async fn fetch_now(
    bg: &mpsc::UnboundedSender<IpcRequest>,
) -> Result<(mxr_protocol::NowData, Option<mxr_protocol::ModeGuideData>), MxrError> {
    let (now, guide) = tokio::join!(
        ipc_call(bg, Request::GetNow { account_id: None }),
        ipc_call(
            bg,
            Request::GetModeGuide {
                mode: Some(crate::app::NOW_MODE.into()),
            },
        ),
    );
    let now = match now {
        Ok(Response::Ok {
            data: ResponseData::Now { now },
        }) => now,
        Ok(Response::Error { message, .. }) => return Err(MxrError::Ipc(message)),
        Err(e) => return Err(e),
        _ => return Err(MxrError::Ipc("unexpected response to GetNow".into())),
    };
    let guide = match guide {
        Ok(Response::Ok {
            data: ResponseData::ModeGuides { mut guides },
        }) if !guides.is_empty() => Some(guides.remove(0)),
        _ => None,
    };
    Ok((now, guide))
}

/// The runway and To do's guide, fetched together so the header, the card
/// and the rows arrive at once. A guide error leaves the rows on screen.
async fn fetch_todo_runway(
    bg: &mpsc::UnboundedSender<IpcRequest>,
    mark_seen: bool,
) -> Result<
    (
        mxr_protocol::TodoRunwayData,
        Option<mxr_protocol::ModeGuideData>,
    ),
    MxrError,
> {
    // Independent reads: ask for both at once.
    let (runway, guide) = tokio::join!(
        ipc_call(
            bg,
            Request::GetTodoRunway {
                account_id: None,
                mark_seen,
            },
        ),
        ipc_call(
            bg,
            Request::GetModeGuide {
                mode: Some(crate::app::TODO_MODE.into()),
            },
        ),
    );
    let runway = match runway {
        Ok(Response::Ok {
            data: ResponseData::TodoRunway { runway },
        }) => runway,
        Ok(Response::Error { message, .. }) => return Err(MxrError::Ipc(message)),
        Err(e) => return Err(e),
        _ => return Err(MxrError::Ipc("unexpected response to GetTodoRunway".into())),
    };
    let guide = match guide {
        Ok(Response::Ok {
            data: ResponseData::ModeGuides { mut guides },
        }) if !guides.is_empty() => Some(guides.remove(0)),
        _ => None,
    };
    Ok((runway, guide))
}

/// `u` for an Archive change, as the daemon described its reversal.
pub(crate) fn records_undo(change: &mxr_protocol::RecordChangeData) -> Option<app::PendingUndo> {
    if change.dry_run {
        return None;
    }
    let undo = change.undo.clone()?;
    if !matches!(undo.kind.as_str(), "restore" | "dismiss" | "clear_fields") {
        return None;
    }
    let verb_past = match change.action.as_str() {
        "dismiss" => "Took out of Archive",
        "restore" => "Brought back",
        "file" => "Filed",
        _ => "Changed",
    };
    Some(app::PendingUndo {
        verb_past: verb_past.into(),
        count: u32::try_from(undo.record_ids.len()).unwrap_or(u32::MAX),
        action: app::UndoAction::Records(undo),
        applied_at: std::time::Instant::now(),
    })
}

/// One Archive read: the payload `extract` takes out of the answer, or
/// the daemon's error.
async fn records_call<T>(
    bg: &mpsc::UnboundedSender<IpcRequest>,
    request: Request,
    extract: fn(ResponseData) -> Option<T>,
) -> Result<T, MxrError> {
    match ipc_call(bg, request).await {
        Ok(Response::Ok { data }) => {
            extract(data).ok_or_else(|| MxrError::Ipc("unexpected answer from Archive".into()))
        }
        Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
        Err(e) => Err(e),
    }
}

fn change_data(data: ResponseData) -> Option<Box<mxr_protocol::RecordChangeData>> {
    match data {
        ResponseData::RecordChange { change } => Some(Box::new(change)),
        _ => None,
    }
}

/// The rows a list answer brings at once: as many as a ledger page.
const RECORDS_ANSWER_LIST_LIMIT: u32 = 200;

/// Starts whatever the Archive lens asked the runtime for.
/// One daemon request for the trust rung, its answer picked out by
/// `pick`.
async fn trust_call<T>(
    bg: &mpsc::UnboundedSender<IpcRequest>,
    request: Request,
    pick: impl FnOnce(ResponseData) -> Option<T>,
) -> Result<T, MxrError> {
    match ipc_call(bg, request).await {
        Ok(Response::Ok { data }) => {
            pick(data).ok_or_else(|| MxrError::Ipc("unexpected response from the daemon".into()))
        }
        Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
        Err(e) => Err(e),
    }
}

/// The arrivals list a count opened, and Inbox's mode chips for the rows
/// on screen.
fn spawn_trust_fetches(
    app: &mut App,
    bg: &mpsc::UnboundedSender<IpcRequest>,
    queued: &mpsc::UnboundedSender<crate::runtime::AsyncResultTask>,
) {
    if let Some(fetch) = app.mailbox.trust.pending_list.take() {
        let bg = bg.clone();
        let _ = submit_task(queued, async move {
            let result = trust_call(
                &bg,
                Request::ListArrivals {
                    account_id: None,
                    bucket: fetch.bucket,
                    since: Some(fetch.since),
                    until: Some(fetch.until),
                    limit: 200,
                },
                |data| match data {
                    ResponseData::ArrivalList { list } => Some(Box::new(list)),
                    _ => None,
                },
            )
            .await;
            AsyncResult::ArrivalsList(fetch, result)
        });
    }
    app.request_visible_chips();
    if let Some(message_ids) = app.mailbox.trust.pending_chips.take() {
        app.mailbox.trust.chips_in_flight = true;
        let bg = bg.clone();
        let _ = submit_task(queued, async move {
            AsyncResult::ArrivalModes(
                trust_call(
                    &bg,
                    Request::GetArrivalModes { message_ids },
                    |data| match data {
                        ResponseData::ArrivalModes { items } => Some(items),
                        _ => None,
                    },
                )
                .await,
            )
        });
    }
}

fn spawn_records_fetches(
    app: &mut App,
    bg: &mpsc::UnboundedSender<IpcRequest>,
    queued: &mpsc::UnboundedSender<crate::runtime::AsyncResultTask>,
) {
    let page = &mut app.mailbox.records_page;
    if page.pending_refresh && page.showing_subscriptions() {
        let bg = bg.clone();
        let _ = submit_task(queued, async move {
            AsyncResult::RecordSubscriptions(
                records_call(
                    &bg,
                    Request::ListRecordSubscriptions { account_id: None },
                    |data| match data {
                        ResponseData::RecordSubscriptions { subscriptions } => {
                            Some(Box::new(subscriptions))
                        }
                        _ => None,
                    },
                )
                .await,
            )
        });
    }
    if std::mem::take(&mut page.pending_refresh) {
        let bg = bg.clone();
        let filter = page.filter.clone();
        let _ = submit_task(queued, async move {
            let (ledger, guide) = tokio::join!(
                records_call(
                    &bg,
                    Request::ListRecords {
                        account_id: None,
                        filter,
                        limit: 200,
                        offset: 0,
                    },
                    |data| match data {
                        ResponseData::RecordLedger { ledger } => Some(ledger),
                        _ => None,
                    },
                ),
                ipc_call(
                    &bg,
                    Request::GetModeGuide {
                        mode: Some(crate::app::ARCHIVE_MODE.into()),
                    },
                ),
            );
            let guide = match guide {
                Ok(Response::Ok {
                    data: ResponseData::ModeGuides { mut guides },
                }) if !guides.is_empty() => Some(guides.remove(0)),
                _ => None,
            };
            AsyncResult::RecordsLedger(ledger.map(|ledger| Box::new((ledger, guide))))
        });
    }
    if let Some(query) = page.pending_answer.take() {
        let list = page.answer_list;
        let bg = bg.clone();
        let _ = submit_task(queued, async move {
            AsyncResult::RecordsAnswer(
                records_call(
                    &bg,
                    Request::AnswerFromRecords {
                        query,
                        account_id: None,
                        fallback: true,
                        limit: 4,
                        list,
                        offset: 0,
                        list_limit: RECORDS_ANSWER_LIST_LIMIT,
                    },
                    |data| match data {
                        ResponseData::RecordAnswer { answer } => Some(Box::new(answer)),
                        _ => None,
                    },
                )
                .await,
            )
        });
    }
    if let Some(record_id) = page.pending_card.take() {
        let bg = bg.clone();
        let _ = submit_task(queued, async move {
            AsyncResult::RecordCard(
                records_call(&bg, Request::GetRecord { record_id }, |data| match data {
                    ResponseData::Record { record } => Some(Box::new(record)),
                    _ => None,
                })
                .await,
            )
        });
    }
    if let Some(write) = page.pending_export.take() {
        let bg = bg.clone();
        let filter = page.filter.clone();
        let _ = submit_task(queued, async move {
            AsyncResult::RecordsExport(
                records_call(
                    &bg,
                    Request::ExportRecords {
                        account_id: None,
                        filter,
                        attachments_dir: None,
                        dry_run: !write,
                    },
                    |data| match data {
                        ResponseData::RecordExport { export } => Some(Box::new(export)),
                        _ => None,
                    },
                )
                .await,
            )
        });
    }
    if let Some((record_id, edit)) = page.pending_fix_preview.take() {
        let bg = bg.clone();
        let _ = submit_task(queued, async move {
            AsyncResult::RecordFixPreview(
                records_call(
                    &bg,
                    Request::SetRecordField {
                        record_id,
                        edit,
                        apply_to_sender: false,
                        dry_run: true,
                    },
                    change_data,
                )
                .await,
            )
        });
    }
    if let Some(message_id) = page.pending_file_preview.take() {
        let bg = bg.clone();
        let _ = submit_task(queued, async move {
            AsyncResult::RecordFilePreview(
                records_call(
                    &bg,
                    Request::FileRecord {
                        message_id,
                        kind: None,
                        dry_run: true,
                    },
                    change_data,
                )
                .await,
            )
        });
    }
    if let Some(message_id) = page.pending_open.take() {
        let bg = bg.clone();
        let _ = submit_task(queued, async move {
            AsyncResult::RecordEnvelopeOpened(
                records_call(
                    &bg,
                    Request::GetEnvelope { message_id },
                    |data| match data {
                        ResponseData::Envelope { envelope } => Some(envelope),
                        _ => None,
                    },
                )
                .await,
            )
        });
    }
}

/// `u` for a to-do change: put the rows back. A catch-up decision goes
/// back to undecided in the batch; anything else back on the runway.
pub(crate) fn todo_undo(change: &mxr_protocol::TodoChangeData) -> Option<app::PendingUndo> {
    if change.dry_run || change.changed.is_empty() {
        return None;
    }
    let ids: Vec<String> = change.changed.iter().map(|todo| todo.id.clone()).collect();
    let (action, verb_past) = match change.action.as_str() {
        "done" => (app::UndoAction::Todos(ids), "Ticked off"),
        "dismiss" => (app::UndoAction::Todos(ids), "Marked not a to-do"),
        "resolution_accept" => {
            let todo = change.changed.first()?;
            let resolution = todo.resolution.as_ref()?;
            return Some(app::PendingUndo {
                action: app::UndoAction::TodoResolution {
                    todo_id: todo.id.clone(),
                    suggestion_id: resolution.id.clone(),
                },
                verb_past: "Accepted resolution for".into(),
                count: 1,
                applied_at: std::time::Instant::now(),
            });
        }
        "keep" => (app::UndoAction::Catchup(ids), "Kept"),
        "let_go" => (app::UndoAction::Catchup(ids), "Let go of"),
        _ => return None,
    };
    Some(app::PendingUndo {
        action,
        verb_past: verb_past.into(),
        count: u32::try_from(change.changed.len()).unwrap_or(u32::MAX),
        applied_at: std::time::Instant::now(),
    })
}

/// Reading's fetches: the edition, the item under the cursor, an
/// article asked for with `L` or `b`, the two previews and an email's own
/// text for `R`.
fn spawn_reading_fetches(
    app: &mut App,
    bg: &mpsc::UnboundedSender<IpcRequest>,
    queued: &mpsc::UnboundedSender<crate::runtime::AsyncResultTask>,
) {
    if std::mem::take(&mut app.mailbox.reading_page.pending_refresh) {
        let mark_visit = std::mem::take(&mut app.mailbox.reading_page.pending_mark_visit);
        let bg = bg.clone();
        let _ = submit_task(queued, async move {
            AsyncResult::ReadingEdition(fetch_reading_edition(&bg, mark_visit).await.map(Box::new))
        });
    }
    if let Some(key) = app.reading_item_to_load() {
        let bg = bg.clone();
        let _ = submit_task(queued, async move {
            let resp = ipc_call(
                &bg,
                Request::GetReadingItem {
                    item_key: key.clone(),
                },
            )
            .await;
            let result = match resp {
                Ok(Response::Ok {
                    data: ResponseData::ReadingItem { item },
                }) => Ok(Box::new(item)),
                Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                Err(e) => Err(e),
                _ => Err(MxrError::Ipc(
                    "unexpected response to GetReadingItem".into(),
                )),
            };
            AsyncResult::ReadingItem(key, result)
        });
    }
    if let Some((key, refresh)) = app.mailbox.reading_page.pending_fetch.take() {
        let bg = bg.clone();
        let _ = submit_task(queued, async move {
            let resp = ipc_call(
                &bg,
                Request::FetchArticle {
                    item_key: key.clone(),
                    refresh,
                },
            )
            .await;
            let result = match resp {
                Ok(Response::Ok {
                    data: ResponseData::ReadingArticle { fetch },
                }) => Ok(fetch),
                Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                Err(e) => Err(e),
                _ => Err(MxrError::Ipc("unexpected response to FetchArticle".into())),
            };
            AsyncResult::ReadingArticle(key, result)
        });
    }
    if let Some(thread_ids) = app.mailbox.reading_page.pending_let_go_preview.take() {
        let bg = bg.clone();
        let _ = submit_task(queued, async move {
            let resp = ipc_call(
                &bg,
                Request::SetModeDone {
                    thread_ids: thread_ids.clone(),
                    mode: mxr_protocol::ModeKindData::Reading,
                    dry_run: true,
                    todo_ids: Vec::new(),
                    sender: None,
                },
            )
            .await;
            let result = match resp {
                Ok(Response::Ok {
                    data: ResponseData::ModeDone { items, .. },
                }) => Ok(items),
                Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                Err(e) => Err(e),
                _ => Err(MxrError::Ipc("unexpected response to SetModeDone".into())),
            };
            AsyncResult::ReadingLetGoPreview(thread_ids, result)
        });
    }
    if let Some(target) = app.mailbox.reading_page.pending_unsubscribe_preview.take() {
        let bg = bg.clone();
        let _ = submit_task(queued, async move {
            let resp = ipc_call(
                &bg,
                Request::UnsubscribePurge {
                    address: target.sender_email.clone(),
                    account_id: Some(target.account_id.clone()),
                    dry_run: true,
                    archive_on_no_method: false,
                    preview_token: None,
                },
            )
            .await;
            let result = match resp {
                Ok(Response::Ok {
                    data: ResponseData::UnsubscribePurgeResult { result },
                }) => Ok(Box::new(result)),
                Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                Err(e) => Err(e),
                _ => Err(MxrError::Ipc(
                    "unexpected response to UnsubscribePurge".into(),
                )),
            };
            AsyncResult::ReadingUnsubscribePreview(target, result)
        });
    }
    if let Some(message_id) = app.mailbox.reading_page.pending_original.take() {
        let bg = bg.clone();
        let _ = submit_task(queued, async move {
            let resp = ipc_call(&bg, Request::GetBody { message_id }).await;
            let result = match resp {
                Ok(Response::Ok {
                    data: ResponseData::Body { body },
                }) => Ok(mxr_reader::clean(
                    body.text_plain.as_deref(),
                    body.text_html.as_deref(),
                    &mxr_reader::ReaderConfig::default(),
                )
                .content),
                Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                Err(e) => Err(e),
                _ => Err(MxrError::Ipc("unexpected response to GetBody".into())),
            };
            AsyncResult::ReadingOriginal(result)
        });
    }
}

/// The edition and Reading's guide, fetched together so the header, the
/// card and the items arrive at once. A guide error leaves the items.
async fn fetch_reading_edition(
    bg: &mpsc::UnboundedSender<IpcRequest>,
    mark_visit: bool,
) -> Result<
    (
        mxr_protocol::ReadingEditionData,
        Option<mxr_protocol::ModeGuideData>,
    ),
    MxrError,
> {
    let (edition, guide) = tokio::join!(
        ipc_call(
            bg,
            Request::GetReadingEdition {
                account_id: None,
                mark_visit,
            },
        ),
        ipc_call(
            bg,
            Request::GetModeGuide {
                mode: Some(crate::app::READING_MODE.into()),
            },
        ),
    );
    let edition = match edition {
        Ok(Response::Ok {
            data: ResponseData::ReadingEdition { edition },
        }) => edition,
        Ok(Response::Error { message, .. }) => return Err(MxrError::Ipc(message)),
        Err(e) => return Err(e),
        _ => {
            return Err(MxrError::Ipc(
                "unexpected response to GetReadingEdition".into(),
            ))
        }
    };
    let guide = match guide {
        Ok(Response::Ok {
            data: ResponseData::ModeGuides { mut guides },
        }) if !guides.is_empty() => Some(guides.remove(0)),
        _ => None,
    };
    Ok((edition, guide))
}

#[cfg(test)]
mod undo_retry_tests {
    #[test]
    fn a_partly_failed_undo_says_how_many_to_retry() {
        assert_eq!(
            super::undo_retry_count(
                "undo: partly done: restored 3, 2 failed and can be retried with the same undo (x)"
            ),
            Some(2)
        );
        assert_eq!(super::undo_retry_count("undo: window expired"), None);
    }
}

#[cfg(test)]
mod sync_all_protocol_tests {
    use super::require_sync_all_protocol;

    #[test]
    fn global_sync_requires_current_protocol() {
        for version in 4..mxr_protocol::IPC_PROTOCOL_VERSION {
            let error =
                require_sync_all_protocol(version).expect_err("an older daemon cannot sync all");
            assert!(error.to_string().contains(&format!(
                "requires daemon IPC protocol {}",
                mxr_protocol::IPC_PROTOCOL_VERSION
            )));
            assert!(error
                .to_string()
                .contains(&format!("running daemon uses {version}")));
        }
        assert!(require_sync_all_protocol(mxr_protocol::IPC_PROTOCOL_VERSION).is_ok());
    }
}

fn category_preview_json(
    preview: mxr_protocol::RuleCategoriesPreviewData,
    form: mxr_protocol::RuleFormData,
) -> Result<serde_json::Value, MxrError> {
    let mut value = serde_json::to_value(&preview).map_err(|e| MxrError::Ipc(e.to_string()))?;
    value["category"] = serde_json::json!(true);
    value["form"] = serde_json::to_value(form).map_err(|e| MxrError::Ipc(e.to_string()))?;
    value["result"] =
        serde_json::json!({"matches":preview.matches,"complete":true,"notice":preview.notice});
    Ok(value)
}
