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
    daemon_socket_path, ipc_get_auth_session, ipc_start_auth_session, request_account_operation,
    run_account_save_workflow,
};
use crate::accounts_helpers::load_accounts_page_accounts;
use crate::async_result::{AsyncResult, UnsubscribeResultData};
use crate::compose_flow::{
    fetch_reply_context_dedicated, handle_compose_action, handle_compose_editor_status,
};
use crate::daemon_events::{
    apply_all_envelopes_refresh, apply_labels_refresh, apply_thread_summary_loaded,
    format_mutation_failure, handle_daemon_event, mutation_verb_past, restore_mail_list_selection,
};
use crate::editor::{edit_tui_config, open_diagnostics_pane_details, open_tui_log_file};
use crate::ipc::{ipc_call, ipc_call_dedicated, spawn_ipc_worker, IpcRequest};
use crate::local_io::{handle_result as handle_local_io_result, submit_bug_report_write};
use crate::runtime::{
    spawn_replaceable_request_worker, spawn_task_worker, submit_task, ReplaceableRequest,
    ReplaceableRequestKey,
};

const SUMMARY_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(150);

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
        ratatui::restore,
        ratatui::init,
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

    loop {
        tokio::time::sleep(poll_interval).await;

        match ipc_get_auth_session(bg, session_id.clone()).await {
            Ok(updated) => {
                let done = is_terminal(&updated.state);
                let _ = result_tx.send(AsyncResult::AuthSession(updated));
                if done {
                    break;
                }
            }
            Err(e) => {
                let _ = result_tx.send(AsyncResult::AccountOperation(Err(e)));
                break;
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
        } => {
            let mut lines = vec![
                body.trim().to_string(),
                String::new(),
                format!("Model: {model}"),
            ];
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
                        .map(|date| format!(" due {}", date.format("%Y-%m-%d")))
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
        } => format!(
            "{}\n\nHumanizer: {}/100\nRewritten: {}x",
            text.trim(),
            report.score,
            iterations
        ),
        ResponseData::Ack => "Done.".into(),
        other => serde_json::to_string_pretty(other).unwrap_or_else(|_| format!("{other:?}")),
    }
}

pub async fn run() -> anyhow::Result<()> {
    let socket_path = daemon_socket_path();
    let mut client = Client::connect(&socket_path).await?;
    let config = load_config()?;
    let local_state = local_state::load();

    let mut app = App::from_config(&config);
    app.modals.onboarding.seen = local_state.onboarding_seen;
    app.command_palette
        .palette
        .restore_recents_from_labels(&local_state.recent_action_labels);
    if config.accounts.is_empty() {
        app.accounts.page.refresh_pending = true;
    } else {
        app.load(&mut client).await?;
        app.maybe_show_feature_onboarding();
        // Load accounts for sidebar account section
        app.accounts.page.refresh_pending = true;
    }

    let mut terminal = ratatui::init();
    app.set_terminal_image_support(crate::terminal_images::TerminalImageSupport::detect());
    let mut events = Some(EventStream::new());

    // Channels for async results
    let (result_tx, mut result_rx) = mpsc::unbounded_channel::<AsyncResult>();

    // Background IPC worker — also forwards daemon events to result_tx
    let bg = spawn_ipc_worker(socket_path.clone(), result_tx.clone());
    let html_assets =
        crate::terminal_images::spawn_html_image_asset_worker(bg.clone(), result_tx.clone());
    let html_decodes = crate::terminal_images::spawn_html_image_decode_worker(result_tx.clone());
    let replaceable = spawn_replaceable_request_worker(bg.clone(), result_tx.clone());
    let queued = spawn_task_worker(result_tx.clone());
    let local_io = spawn_task_worker(result_tx.clone());

    loop {
        crate::local_io::submit_pending_work(&mut app, &local_io);

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
                    _ => Err(MxrError::Ipc("unexpected response".into())),
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
                        _ => Err(MxrError::Ipc("unexpected response".into())),
                    }
                } else {
                    let purge_resp = ipc_call(
                        &bg,
                        Request::UnsubscribePurge {
                            address: pending.sender_email.clone(),
                            account_id: Some(pending.account_id.clone()),
                            dry_run: false,
                            archive_on_no_method: false,
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
                        _ => Err(MxrError::Ipc("unexpected response".into())),
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
                    _ => Err(MxrError::Ipc("unexpected response".into())),
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
                    _ => Err(MxrError::Ipc("unexpected response".into())),
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
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                AsyncResult::ReplyQueueList(result)
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
                    _ => Err(MxrError::Ipc("unexpected response".into())),
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
                    _ => Err(MxrError::Ipc("unexpected response".into())),
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
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                AsyncResult::DeliveriesList(result)
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
                    _ => Err(MxrError::Ipc("unexpected response".into())),
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
                    _ => Err(MxrError::Ipc("unexpected response".into())),
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
                    _ => Err(MxrError::Ipc("unexpected response".into())),
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
                    },
                )
                .await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::ScreenerQueue { entries },
                    }) => Ok(entries),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
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
                    _ => Err(MxrError::Ipc("unexpected response".into())),
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
            let socket_path = socket_path.clone();
            let captured_id = thread_id.clone();
            let result_tx = result_tx.clone();
            tokio::spawn(async move {
                let resp = tokio::time::timeout(
                    SUMMARY_REQUEST_TIMEOUT,
                    ipc_call_dedicated(
                        &socket_path,
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
                    Ok(Ok(_)) => Err(MxrError::Ipc("unexpected response".into())),
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

        if let Some(rule) = app.rules.pending_dry_run.take() {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let resp = ipc_call(
                    &bg,
                    Request::DryRunRules {
                        rule: Some(rule),
                        all: false,
                        after: None,
                    },
                )
                .await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::RuleDryRun { results },
                    }) => Ok(results),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                AsyncResult::RuleDryRun(result)
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
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                AsyncResult::RuleUpsert(result)
            });
        }

        if app.rules.pending_form_save {
            app.rules.pending_form_save = false;
            let bg = bg.clone();
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
                    _ => Err(MxrError::Ipc("unexpected response".into())),
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
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                AsyncResult::SavedSearchListRefreshed(result)
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

        let platform_dispatch = app.take_pending_platform_dispatch();
        for pending in platform_dispatch {
            let bg = bg.clone();
            let title = pending.title.clone();
            let _ = submit_task(&queued, async move {
                for request in pending.prelude {
                    match ipc_call(&bg, request).await {
                        Ok(Response::Ok { .. }) => {}
                        Ok(Response::Error { message, .. }) => {
                            return AsyncResult::PlatformModalLoaded {
                                title,
                                result: Err(MxrError::Ipc(message)),
                            };
                        }
                        Err(e) => {
                            return AsyncResult::PlatformModalLoaded {
                                title,
                                result: Err(e),
                            };
                        }
                    }
                }
                let resp = ipc_call(&bg, pending.request).await;
                let result: Result<String, MxrError> = match resp {
                    Ok(Response::Ok { data }) => Ok(format_platform_response(&data)),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                };
                AsyncResult::PlatformModalLoaded { title, result }
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
                    _ => Err(MxrError::Ipc("unexpected response".into())),
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
                    _ => Err(MxrError::Ipc("unexpected response".into())),
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
                    _ => Err(MxrError::Ipc("unexpected response".into())),
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
                    _ => Err(MxrError::Ipc("unexpected response".into())),
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
                    _ => Err(MxrError::Ipc("unexpected response".into())),
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
                    _ => Err(MxrError::Ipc("unexpected response".into())),
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
                    return AsyncResult::OwedReplies(Ok(Vec::new()));
                };
                let resp = ipc_call(
                    &bg,
                    Request::ListOwedReplies {
                        account_id,
                        older_than_days: None,
                        within_days: None,
                        limit: 100,
                    },
                )
                .await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::OwedReplies { rows },
                    }) => Ok(rows),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                AsyncResult::OwedReplies(result)
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
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                AsyncResult::CalendarInvites(result)
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
                    _ => Err(MxrError::Ipc("unexpected response".into())),
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
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                AsyncResult::BugReport(result)
            });
        }

        if let Some(pending) = app.mailbox.pending_attachment_action.take() {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let request = match pending.operation {
                    AttachmentOperation::Open => Request::OpenAttachment {
                        message_id: pending.message_id,
                        attachment_id: pending.attachment_id,
                    },
                    AttachmentOperation::Download => Request::DownloadAttachment {
                        message_id: pending.message_id,
                        attachment_id: pending.attachment_id,
                        destination: pending.destination,
                    },
                };
                let resp = ipc_call(&bg, request).await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::AttachmentFile { file },
                    }) => Ok(file),
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
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
                    _ => Err(MxrError::Ipc("unexpected response".into())),
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
                id: mutation_id,
                request: req,
                effect,
                best_effort,
                attempts,
                run_after: _,
            } = queued_mutation;
            let retry =
                (attempts < app::TRANSIENT_MUTATION_MAX_RETRIES).then(|| app::QueuedMutation {
                    id: mutation_id,
                    request: req.clone(),
                    effect: effect.clone(),
                    best_effort,
                    attempts,
                    run_after: None,
                });
            let bg = bg.clone();
            let result_tx_inner = result_tx.clone();
            let _ = submit_task(&queued, async move {
                let verb = mutation_verb_past(&req);
                let resp = ipc_call(&bg, req).await;
                let outcome = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::Ack,
                    }) => Ok(effect),
                    Ok(Response::Ok {
                        data:
                            ResponseData::SendReceipt {
                                local_message_id, ..
                            },
                    }) => Ok(match effect {
                        app::MutationEffect::SentSuccess {
                            status,
                            remind_at,
                            sent_message_id: _,
                        } => app::MutationEffect::SentSuccess {
                            status,
                            remind_at,
                            sent_message_id: Some(local_message_id),
                        },
                        other => other,
                    }),
                    Ok(Response::Ok {
                        data: ResponseData::MutationResult { result },
                    }) if result.succeeded > 0 => {
                        if let Some(daemon_mutation_id) = result.mutation_id.clone() {
                            let _ =
                                result_tx_inner.send(AsyncResult::UndoCaptured(app::PendingUndo {
                                    mutation_id: daemon_mutation_id,
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
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                AsyncResult::MutationResult {
                    id: mutation_id,
                    best_effort,
                    retry: retry.map(Box::new),
                    outcome: Box::new(outcome),
                }
            });
        }
        app.pending_mutation_queue.extend(deferred_mutations);

        // Handle thread export (uses daemon ExportThread which runs mxr-export)
        if let Some(thread_id) = app.mailbox.pending_export_thread.take() {
            let bg = bg.clone();
            let _ = submit_task(&local_io, async move {
                let resp = ipc_call(
                    &bg,
                    Request::ExportThread {
                        thread_id,
                        format: mxr_core::types::ExportFormat::Markdown,
                    },
                )
                .await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::ExportResult { content },
                    }) => {
                        // Write to temp file
                        let filename = format!(
                            "mxr-export-{}.md",
                            chrono::Utc::now().format("%Y%m%d-%H%M%S")
                        );
                        let path = std::env::temp_dir().join(&filename);
                        match tokio::fs::write(&path, &content).await {
                            Ok(()) => Ok(format!("Exported to {}", path.display())),
                            Err(e) => Err(MxrError::Ipc(format!("Write failed: {e}"))),
                        }
                    }
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                AsyncResult::ExportResult(result)
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
                let socket_path = socket_path.clone();
                let result_tx_clone = result_tx.clone();
                let message_id = env_id.clone();
                tokio::spawn(async move {
                    let reply =
                        fetch_reply_context_dedicated(&socket_path, message_id.clone(), false)
                            .await;
                    let reply_all =
                        fetch_reply_context_dedicated(&socket_path, message_id.clone(), true).await;
                    let _ = result_tx_clone.send(AsyncResult::ReplyContextWarmed {
                        message_id,
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
                        AsyncResult::Rules(Ok(rules)) => {
                            app.rules.page.rules = rules;
                            app.rules.page.selected_index = app
                                .rules
                                .page
                                .selected_index
                                .min(app.rules.page.rules.len().saturating_sub(1));
                            app.refresh_selected_rule_panel();
                        }
                        AsyncResult::Rules(Err(e)) => {
                            app.rules.page.status = Some(format!("Rules error: {e}"));
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
                        AsyncResult::RuleDryRun(Ok(results)) => {
                            app.rules.page.dry_run = results;
                        }
                        AsyncResult::RuleDryRun(Err(e)) => {
                            app.rules.page.status = Some(format!("Dry-run error: {e}"));
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
                                            ..
                                        },
                                } => {
                                    app.apply_status_snapshot(
                                        uptime_secs,
                                        daemon_pid,
                                        accounts,
                                        total_messages,
                                        sync_statuses,
                                    );
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
                            app.apply_status_snapshot(
                                snapshot.uptime_secs,
                                snapshot.daemon_pid,
                                snapshot.accounts,
                                snapshot.total_messages,
                                snapshot.sync_statuses,
                            );
                        }
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
                                app.maybe_show_feature_onboarding();
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
                            result: Err(_),
                        } => {
                            if request_id != app.mailbox.thread_request_id {
                                tracing::trace!(request_id, current_id = app.mailbox.thread_request_id, "tui stale thread dropped");
                                continue;
                            }
                            app.resolve_thread_fetch_error(&thread_id);
                        }
                        AsyncResult::MutationResult {
                            id,
                            best_effort,
                            retry,
                            outcome,
                        } => {
                            app.finish_pending_mutation();
                            match *outcome {
                                Ok(effect) => {
                                    // Daemon ack'd the mutation: discard the rollback
                                    // snapshot — there's no longer anything to revert.
                                    let _ = app.mutation_snapshots.take(id);
                                    // The optimistic change is now authoritative on
                                    // both sides; subsequent refreshes will reflect it
                                    // naturally, so stop masking refresh responses for
                                    // this mutation.
                                    app.pending_optimistic.clear(id);
                                    let show_completion_status = app.pending_mutation_count == 0;
                                    app.apply_mutation_completion(effect, show_completion_status);
                                    // In the invites lens, a completed mutation is an
                                    // RSVP — refetch so the row's RSVP status updates.
                                    if app.mailbox.mailbox_view
                                        == crate::app::MailboxView::CalendarInvites
                                    {
                                        app.mailbox.pending_calendar_invites_refresh = true;
                                    }
                                }
                                Err(e) => {
                                    if app.should_retry_mutation_failure(&e) {
                                        if let Some(retry) = retry {
                                            app.schedule_mutation_retry(*retry, &e);
                                            continue;
                                        }
                                    }
                                    app.handle_mutation_failure_result(id, best_effort, &e);
                                }
                            }
                        }
                        AsyncResult::ComposeReady(Ok(data)) => {
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
                            app.status_message = Some(format!("Compose error: {e}"));
                        }
                        AsyncResult::ReplyContextWarmed {
                            message_id,
                            reply,
                            reply_all,
                        } => {
                            let reply_ok = (*reply).ok();
                            let reply_all_ok = (*reply_all).ok();
                            app.compose.reply_context_cache.insert(
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
                                    let preloaded = if deferred.reply_all {
                                        reply_all_ok
                                    } else {
                                        reply_ok
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
                        AsyncResult::OwedReplies(Ok(rows)) => {
                            app.mailbox.owed_page.entries = rows;
                            app.mailbox.selected_index = app
                                .mailbox
                                .selected_index
                                .min(app.mailbox.owed_page.entries.len().saturating_sub(1));
                        }
                        AsyncResult::OwedReplies(Err(e)) => {
                            app.status_message = Some(format!("Owed replies error: {e}"));
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
                        AsyncResult::ConnectionState(state) => {
                            app.set_connection_state(state);
                        }
                        AsyncResult::ReportedError(entry) => {
                            app.push_reported_error(entry);
                        }
                        AsyncResult::UndoCaptured(undo) => {
                            app.set_pending_undo(undo);
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
                                    _ => Err(MxrError::Ipc("unexpected response".into())),
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
                        AsyncResult::PlatformModalLoaded { title, result } => {
                            app.modals.platform.title = title;
                            match result {
                                Ok(body) => app.modals.platform.set_body(body),
                                Err(e) => app.modals.platform.set_error(e.to_string()),
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
            }
        }

        if app.should_quit {
            break;
        }
    }

    ratatui::restore();
    Ok(())
}
#[cfg(test)]
mod tests;
