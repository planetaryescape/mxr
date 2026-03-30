#![cfg_attr(test, allow(clippy::panic, clippy::unwrap_used))]

pub mod action;
pub mod app;
pub mod client;
pub mod desktop_manifest;
pub mod input;
pub mod keybindings;
pub mod local_state;
pub mod terminal_images;
#[cfg(test)]
mod test_fixtures;
pub mod theme;
pub mod ui;

use app::{App, AttachmentOperation, ComposeAction, PendingSend};
use client::Client;
use crossterm::event::EventStream;
use futures::StreamExt;
use mxr_config::{load_config, socket_path as config_socket_path};
use mxr_core::MxrError;
use mxr_protocol::{DaemonEvent, Request, Response, ResponseData};
use ratatui::crossterm::event::Event;
use std::path::Path;
use std::process::Stdio;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{sleep, Duration, Instant};

/// A request sent from the main loop to the background IPC worker.
struct IpcRequest {
    request: Request,
    reply: oneshot::Sender<Result<Response, MxrError>>,
}

/// Runs a single persistent daemon connection in a background task.
/// The main loop sends requests via channel — no new connections per operation.
/// Daemon events (SyncCompleted, LabelCountsUpdated, etc.) are forwarded to result_tx.
fn spawn_ipc_worker(
    socket_path: std::path::PathBuf,
    result_tx: mpsc::UnboundedSender<AsyncResult>,
) -> mpsc::UnboundedSender<IpcRequest> {
    let (tx, mut rx) = mpsc::unbounded_channel::<IpcRequest>();
    tokio::spawn(async move {
        // Create event channel — Client forwards daemon events here
        let (event_tx, mut event_rx) = mpsc::unbounded_channel::<DaemonEvent>();
        let mut client = match connect_ipc_client(&socket_path, event_tx.clone()).await {
            Ok(client) => client,
            Err(_) => return,
        };

        loop {
            tokio::select! {
                req = rx.recv() => {
                    match req {
                        Some(req) => {
                            let mut result = client.raw_request(req.request.clone()).await;
                            if should_reconnect_ipc(&result)
                                && request_supports_retry(&req.request)
                            {
                                match connect_ipc_client(&socket_path, event_tx.clone()).await {
                                    Ok(mut reconnected) => {
                                        let retry = reconnected.raw_request(req.request.clone()).await;
                                        if retry.is_ok() {
                                            client = reconnected;
                                        }
                                        result = retry;
                                    }
                                    Err(error) => {
                                        result = Err(error);
                                    }
                                }
                            }
                            let _ = req.reply.send(result);
                        }
                        None => break,
                    }
                }
                event = event_rx.recv() => {
                    if let Some(event) = event {
                        let _ = result_tx.send(AsyncResult::DaemonEvent(event));
                    }
                }
            }
        }
    });
    tx
}

async fn connect_ipc_client(
    socket_path: &std::path::Path,
    event_tx: mpsc::UnboundedSender<DaemonEvent>,
) -> Result<Client, MxrError> {
    match Client::connect(socket_path).await {
        Ok(client) => Ok(client.with_event_channel(event_tx)),
        Err(error) if should_autostart_daemon(&error) => {
            start_daemon_process(socket_path).await?;
            wait_for_daemon_client(socket_path, START_DAEMON_TIMEOUT)
                .await
                .map(|client| client.with_event_channel(event_tx))
        }
        Err(error) => Err(MxrError::Ipc(error.to_string())),
    }
}

fn should_reconnect_ipc(result: &Result<Response, MxrError>) -> bool {
    match result {
        Err(MxrError::Ipc(message)) => {
            let lower = message.to_lowercase();
            lower.contains("broken pipe")
                || lower.contains("connection closed")
                || lower.contains("connection refused")
                || lower.contains("connection reset")
        }
        _ => false,
    }
}

const START_DAEMON_TIMEOUT: Duration = Duration::from_secs(5);
const START_DAEMON_POLL_INTERVAL: Duration = Duration::from_millis(100);

fn should_autostart_daemon(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::NotFound
    )
}

async fn start_daemon_process(socket_path: &Path) -> Result<(), MxrError> {
    let exe = std::env::current_exe()
        .map_err(|error| MxrError::Ipc(format!("failed to locate mxr binary: {error}")))?;
    std::process::Command::new(exe)
        .arg("daemon")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| {
            MxrError::Ipc(format!(
                "failed to start daemon for {}: {error}",
                socket_path.display()
            ))
        })?;
    Ok(())
}

async fn wait_for_daemon_client(socket_path: &Path, timeout: Duration) -> Result<Client, MxrError> {
    let deadline = Instant::now() + timeout;
    let mut last_error: Option<MxrError> = None;

    loop {
        if Instant::now() >= deadline {
            let detail =
                last_error.unwrap_or_else(|| MxrError::Ipc("daemon did not become ready".into()));
            return Err(MxrError::Ipc(format!(
                "daemon restart did not become ready for {}: {}",
                socket_path.display(),
                detail
            )));
        }

        match Client::connect(socket_path).await {
            Ok(mut client) => match client.raw_request(Request::GetStatus).await {
                Ok(_) => return Ok(client),
                Err(error) => last_error = Some(error),
            },
            Err(error) => last_error = Some(MxrError::Ipc(error.to_string())),
        }

        sleep(START_DAEMON_POLL_INTERVAL).await;
    }
}

fn request_supports_retry(request: &Request) -> bool {
    matches!(
        request,
        Request::ListEnvelopes { .. }
            | Request::ListEnvelopesByIds { .. }
            | Request::GetEnvelope { .. }
            | Request::GetBody { .. }
            | Request::GetHtmlImageAssets { .. }
            | Request::ListBodies { .. }
            | Request::GetThread { .. }
            | Request::ListLabels { .. }
            | Request::ListRules
            | Request::ListAccounts
            | Request::ListAccountsConfig
            | Request::GetRule { .. }
            | Request::GetRuleForm { .. }
            | Request::DryRunRules { .. }
            | Request::ListEvents { .. }
            | Request::GetLogs { .. }
            | Request::GetDoctorReport
            | Request::GenerateBugReport { .. }
            | Request::ListRuleHistory { .. }
            | Request::Search { .. }
            | Request::GetSyncStatus { .. }
            | Request::Count { .. }
            | Request::GetHeaders { .. }
            | Request::ListSavedSearches
            | Request::ListSubscriptions { .. }
            | Request::RunSavedSearch { .. }
            | Request::ListSnoozed
            | Request::PrepareReply { .. }
            | Request::PrepareForward { .. }
            | Request::ListDrafts
            | Request::GetStatus
            | Request::Ping
    )
}

/// Send a request to the IPC worker and get the response.
async fn ipc_call(
    tx: &mpsc::UnboundedSender<IpcRequest>,
    request: Request,
) -> Result<Response, MxrError> {
    let (reply_tx, reply_rx) = oneshot::channel();
    tx.send(IpcRequest {
        request,
        reply: reply_tx,
    })
    .map_err(|_| MxrError::Ipc("IPC worker closed".into()))?;
    reply_rx
        .await
        .map_err(|_| MxrError::Ipc("IPC worker dropped".into()))?
}

fn edit_tui_config(app: &mut App) -> Result<String, MxrError> {
    let config_path = mxr_config::config_file_path();
    let current_config = load_config().map_err(|error| MxrError::Ipc(error.to_string()))?;

    if !config_path.exists() {
        mxr_config::save_config(&current_config)
            .map_err(|error| MxrError::Ipc(error.to_string()))?;
    }

    let editor = mxr_compose::editor::resolve_editor(current_config.general.editor.as_deref());
    let status = std::process::Command::new(&editor)
        .arg(&config_path)
        .status()
        .map_err(|error| MxrError::Ipc(format!("failed to launch editor: {error}")))?;

    if !status.success() {
        return Ok("Config edit cancelled".into());
    }

    let reloaded = load_config().map_err(|error| MxrError::Ipc(error.to_string()))?;
    app.apply_runtime_config(&reloaded);
    app.accounts_page.refresh_pending = true;
    app.pending_status_refresh = true;

    Ok("Config reloaded. Restart daemon for account/provider changes.".into())
}

fn open_tui_log_file() -> Result<String, MxrError> {
    let log_path = mxr_config::data_dir().join("logs").join("mxr.log");
    if !log_path.exists() {
        return Err(MxrError::Ipc(format!(
            "log file not found at {}",
            log_path.display()
        )));
    }

    let editor = load_config()
        .ok()
        .and_then(|config| config.general.editor)
        .map(|editor| mxr_compose::editor::resolve_editor(Some(editor.as_str())))
        .unwrap_or_else(|| mxr_compose::editor::resolve_editor(None));
    let status = std::process::Command::new(&editor)
        .arg(&log_path)
        .status()
        .map_err(|error| MxrError::Ipc(format!("failed to launch editor: {error}")))?;

    if !status.success() {
        return Ok("Log open cancelled".into());
    }

    Ok(format!("Opened logs at {}", log_path.display()))
}

fn open_temp_text_buffer(name: &str, content: &str) -> Result<String, MxrError> {
    let path = std::env::temp_dir().join(format!(
        "mxr-{}-{}.txt",
        name,
        chrono::Utc::now().format("%Y%m%d-%H%M%S")
    ));
    std::fs::write(&path, content)
        .map_err(|error| MxrError::Ipc(format!("failed to write temp file: {error}")))?;

    let editor = load_config()
        .ok()
        .and_then(|config| config.general.editor)
        .map(|editor| mxr_compose::editor::resolve_editor(Some(editor.as_str())))
        .unwrap_or_else(|| mxr_compose::editor::resolve_editor(None));
    let status = std::process::Command::new(&editor)
        .arg(&path)
        .status()
        .map_err(|error| MxrError::Ipc(format!("failed to launch editor: {error}")))?;

    if !status.success() {
        return Ok(format!(
            "Diagnostics detail open cancelled ({})",
            path.display()
        ));
    }

    Ok(format!("Opened diagnostics details at {}", path.display()))
}

fn open_diagnostics_pane_details(
    state: &app::DiagnosticsPageState,
    pane: app::DiagnosticsPaneKind,
) -> Result<String, MxrError> {
    if pane == app::DiagnosticsPaneKind::Logs {
        return open_tui_log_file();
    }

    let name = match pane {
        app::DiagnosticsPaneKind::Status => "doctor",
        app::DiagnosticsPaneKind::Data => "storage",
        app::DiagnosticsPaneKind::Sync => "sync-health",
        app::DiagnosticsPaneKind::Events => "events",
        app::DiagnosticsPaneKind::Logs => "logs",
    };
    let content = crate::ui::diagnostics_page::pane_details_text(state, pane);
    open_temp_text_buffer(name, &content)
}

pub async fn run() -> anyhow::Result<()> {
    let socket_path = daemon_socket_path();
    let mut client = Client::connect(&socket_path).await?;
    let config = load_config()?;
    let local_state = local_state::load();

    let mut app = App::from_config(&config);
    app.onboarding.seen = local_state.onboarding_seen;
    if config.accounts.is_empty() {
        app.accounts_page.refresh_pending = true;
    } else {
        app.load(&mut client).await?;
        app.maybe_show_feature_onboarding();
        // Load accounts for sidebar account section
        app.accounts_page.refresh_pending = true;
    }

    let mut terminal = ratatui::init();
    app.set_terminal_image_support(crate::terminal_images::TerminalImageSupport::detect());
    let mut events = EventStream::new();

    // Channels for async results
    let (result_tx, mut result_rx) = mpsc::unbounded_channel::<AsyncResult>();

    // Background IPC worker — also forwards daemon events to result_tx
    let bg = spawn_ipc_worker(socket_path, result_tx.clone());

    loop {
        if app.pending_local_state_save {
            app.pending_local_state_save = false;
            let state = local_state::TuiLocalState {
                onboarding_seen: app.onboarding.seen,
            };
            if let Err(error) = local_state::save(&state) {
                app.status_message = Some(format!("Could not save TUI state: {error}"));
            }
        }

        if app.pending_config_edit {
            app.pending_config_edit = false;
            ratatui::restore();
            let result = edit_tui_config(&mut app);
            terminal = ratatui::init();
            match result {
                Ok(message) => {
                    app.status_message = Some(message);
                }
                Err(error) => {
                    app.error_modal = Some(app::ErrorModalState::new(
                        "Config Reload Failed",
                        format!(
                            "Config could not be reloaded after editing.\n\n{error}\n\nFix the file and run Edit Config again."
                        ),
                    ));
                    app.status_message = Some(format!("Config reload failed: {error}"));
                }
            }
        }
        if app.pending_log_open {
            app.pending_log_open = false;
            ratatui::restore();
            let result = open_tui_log_file();
            terminal = ratatui::init();
            match result {
                Ok(message) => {
                    app.status_message = Some(message);
                }
                Err(error) => {
                    app.error_modal = Some(app::ErrorModalState::new(
                        "Open Logs Failed",
                        format!(
                            "The log file could not be opened.\n\n{error}\n\nCheck that the daemon has created the log file and try again."
                        ),
                    ));
                    app.status_message = Some(format!("Open logs failed: {error}"));
                }
            }
        }
        if let Some(pane) = app.pending_diagnostics_details.take() {
            ratatui::restore();
            let result = open_diagnostics_pane_details(&app.diagnostics_page, pane);
            terminal = ratatui::init();
            match result {
                Ok(message) => {
                    app.status_message = Some(message);
                }
                Err(error) => {
                    app.error_modal = Some(app::ErrorModalState::new(
                        "Diagnostics Open Failed",
                        format!(
                            "The diagnostics source could not be opened.\n\n{error}\n\nTry refresh first, then open details again."
                        ),
                    ));
                    app.status_message = Some(format!("Open diagnostics failed: {error}"));
                }
            }
        }

        // Batch any queued body fetches. Current message fetches and window prefetches
        // share the same path so all state transitions stay consistent.
        if !app.queued_body_fetches.is_empty() {
            let ids = std::mem::take(&mut app.queued_body_fetches);
            let bg = bg.clone();
            let tx = result_tx.clone();
            tokio::spawn(async move {
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
                        data: ResponseData::Bodies { bodies },
                    }) => Ok(bodies),
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                let _ = tx.send(AsyncResult::Bodies { requested, result });
            });
        }

        if !app.queued_html_image_asset_fetches.is_empty() {
            let ids = std::mem::take(&mut app.queued_html_image_asset_fetches);
            for message_id in ids {
                app.in_flight_html_image_asset_requests
                    .insert(message_id.clone());
                let bg = bg.clone();
                let tx = result_tx.clone();
                let allow_remote = app.remote_content_enabled;
                tokio::spawn(async move {
                    let resp = ipc_call(
                        &bg,
                        Request::GetHtmlImageAssets {
                            message_id: message_id.clone(),
                            allow_remote,
                        },
                    )
                    .await;
                    let result = match resp {
                        Ok(Response::Ok {
                            data: ResponseData::HtmlImageAssets { assets, .. },
                        }) => Ok(assets),
                        Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                        Err(error) => Err(error),
                        _ => Err(MxrError::Ipc("unexpected response".into())),
                    };
                    let _ = tx.send(AsyncResult::HtmlImageAssets {
                        message_id,
                        allow_remote,
                        result,
                    });
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
                    crate::terminal_images::spawn_image_decode(key, path, result_tx.clone());
                }
            }
        }

        if let Some(thread_id) = app.pending_thread_fetch.take() {
            app.in_flight_thread_fetch = Some(thread_id.clone());
            let bg = bg.clone();
            let tx = result_tx.clone();
            tokio::spawn(async move {
                let resp = ipc_call(
                    &bg,
                    Request::GetThread {
                        thread_id: thread_id.clone(),
                    },
                )
                .await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::Thread { thread, messages },
                    }) => Ok((thread, messages)),
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                let _ = tx.send(AsyncResult::Thread { thread_id, result });
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
        if let Some(pending) = app.pending_search.take() {
            let bg = bg.clone();
            let tx = result_tx.clone();
            tokio::spawn(async move {
                let query = pending.query.clone();
                let target = pending.target;
                let append = pending.append;
                let session_id = pending.session_id;
                let results = match ipc_call(
                    &bg,
                    Request::Search {
                        query,
                        limit: pending.limit,
                        offset: pending.offset,
                        mode: Some(pending.mode),
                        sort: Some(pending.sort),
                        explain: false,
                    },
                )
                .await
                {
                    Ok(Response::Ok {
                        data:
                            ResponseData::SearchResults {
                                results, has_more, ..
                            },
                    }) => {
                        let mut scores = std::collections::HashMap::new();
                        let message_ids = results
                            .into_iter()
                            .map(|result| {
                                scores.insert(result.message_id.clone(), result.score);
                                result.message_id
                            })
                            .collect::<Vec<_>>();
                        if message_ids.is_empty() {
                            Ok(SearchResultData {
                                envelopes: Vec::new(),
                                scores,
                                has_more,
                            })
                        } else {
                            match ipc_call(&bg, Request::ListEnvelopesByIds { message_ids }).await {
                                Ok(Response::Ok {
                                    data: ResponseData::Envelopes { envelopes },
                                }) => Ok(SearchResultData {
                                    envelopes,
                                    scores,
                                    has_more,
                                }),
                                Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                                Err(e) => Err(e),
                                _ => Err(MxrError::Ipc("unexpected response".into())),
                            }
                        }
                    }
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                let _ = tx.send(AsyncResult::Search {
                    target,
                    append,
                    session_id,
                    result: results,
                });
            });
        }

        if let Some(pending) = app.pending_search_count.take() {
            let bg = bg.clone();
            let tx = result_tx.clone();
            tokio::spawn(async move {
                let session_id = pending.session_id;
                let result = match ipc_call(
                    &bg,
                    Request::Count {
                        query: pending.query,
                        mode: Some(pending.mode),
                    },
                )
                .await
                {
                    Ok(Response::Ok {
                        data: ResponseData::Count { count },
                    }) => Ok(count),
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(error) => Err(error),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                let _ = tx.send(AsyncResult::SearchCount { session_id, result });
            });
        }

        if let Some(pending) = app.pending_unsubscribe_action.take() {
            let bg = bg.clone();
            let tx = result_tx.clone();
            tokio::spawn(async move {
                let unsubscribe_resp = ipc_call(
                    &bg,
                    Request::Unsubscribe {
                        message_id: pending.message_id.clone(),
                    },
                )
                .await;
                let unsubscribe_result = match unsubscribe_resp {
                    Ok(Response::Ok {
                        data: ResponseData::Ack,
                    }) => Ok(()),
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(error) => Err(error),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };

                let result = match unsubscribe_result {
                    Ok(()) if pending.archive_message_ids.is_empty() => Ok(UnsubscribeResultData {
                        archived_ids: Vec::new(),
                        message: format!("Unsubscribed from {}", pending.sender_email),
                    }),
                    Ok(()) => {
                        let archived_count = pending.archive_message_ids.len();
                        let archive_resp = ipc_call(
                            &bg,
                            Request::Mutation(mxr_protocol::MutationCommand::Archive {
                                message_ids: pending.archive_message_ids.clone(),
                            }),
                        )
                        .await;
                        match archive_resp {
                            Ok(Response::Ok {
                                data: ResponseData::Ack,
                            }) => Ok(UnsubscribeResultData {
                                archived_ids: pending.archive_message_ids,
                                message: format!(
                                    "Unsubscribed and archived {} messages from {}",
                                    archived_count, pending.sender_email
                                ),
                            }),
                            Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                            Err(error) => Err(error),
                            _ => Err(MxrError::Ipc("unexpected response".into())),
                        }
                    }
                    Err(error) => Err(error),
                };
                let _ = tx.send(AsyncResult::Unsubscribe(result));
            });
        }

        if app.rules_page.refresh_pending {
            app.rules_page.refresh_pending = false;
            let bg = bg.clone();
            let tx = result_tx.clone();
            tokio::spawn(async move {
                let resp = ipc_call(&bg, Request::ListRules).await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::Rules { rules },
                    }) => Ok(rules),
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                let _ = tx.send(AsyncResult::Rules(result));
            });
        }

        if let Some(rule) = app.pending_rule_detail.take() {
            let bg = bg.clone();
            let tx = result_tx.clone();
            tokio::spawn(async move {
                let resp = ipc_call(&bg, Request::GetRule { rule }).await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::RuleData { rule },
                    }) => Ok(rule),
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                let _ = tx.send(AsyncResult::RuleDetail(result));
            });
        }

        if let Some(rule) = app.pending_rule_history.take() {
            let bg = bg.clone();
            let tx = result_tx.clone();
            tokio::spawn(async move {
                let resp = ipc_call(
                    &bg,
                    Request::ListRuleHistory {
                        rule: Some(rule),
                        limit: 20,
                    },
                )
                .await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::RuleHistory { entries },
                    }) => Ok(entries),
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                let _ = tx.send(AsyncResult::RuleHistory(result));
            });
        }

        if let Some(rule) = app.pending_rule_dry_run.take() {
            let bg = bg.clone();
            let tx = result_tx.clone();
            tokio::spawn(async move {
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
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                let _ = tx.send(AsyncResult::RuleDryRun(result));
            });
        }

        if let Some(rule) = app.pending_rule_form_load.take() {
            let bg = bg.clone();
            let tx = result_tx.clone();
            tokio::spawn(async move {
                let resp = ipc_call(&bg, Request::GetRuleForm { rule }).await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::RuleFormData { form },
                    }) => Ok(form),
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                let _ = tx.send(AsyncResult::RuleForm(result));
            });
        }

        if let Some(rule) = app.pending_rule_delete.take() {
            let bg = bg.clone();
            let tx = result_tx.clone();
            tokio::spawn(async move {
                let resp = ipc_call(&bg, Request::DeleteRule { rule }).await;
                let result = match resp {
                    Ok(Response::Ok { .. }) => Ok(()),
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                };
                let _ = tx.send(AsyncResult::RuleDeleted(result));
            });
        }

        if let Some(rule) = app.pending_rule_upsert.take() {
            let bg = bg.clone();
            let tx = result_tx.clone();
            tokio::spawn(async move {
                let resp = ipc_call(&bg, Request::UpsertRule { rule }).await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::RuleData { rule },
                    }) => Ok(rule),
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                let _ = tx.send(AsyncResult::RuleUpsert(result));
            });
        }

        if app.pending_rule_form_save {
            app.pending_rule_form_save = false;
            let bg = bg.clone();
            let tx = result_tx.clone();
            let existing_rule = app.rules_page.form.existing_rule.clone();
            let name = app.rules_page.form.name.clone();
            let condition = app.rules_page.form.condition.clone();
            let action = app.rules_page.form.action.clone();
            let priority = app.rules_page.form.priority.parse::<i32>().unwrap_or(100);
            let enabled = app.rules_page.form.enabled;
            tokio::spawn(async move {
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
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                let _ = tx.send(AsyncResult::RuleUpsert(result));
            });
        }

        if app.diagnostics_page.refresh_pending {
            app.diagnostics_page.refresh_pending = false;
            app.pending_status_refresh = false;
            app.diagnostics_page.pending_requests = 4;
            for request in [
                Request::GetStatus,
                Request::GetDoctorReport,
                Request::ListEvents {
                    limit: 20,
                    level: None,
                    category: None,
                },
                Request::GetLogs {
                    limit: 50,
                    level: None,
                },
            ] {
                let bg = bg.clone();
                let tx = result_tx.clone();
                tokio::spawn(async move {
                    let resp = ipc_call(&bg, request).await;
                    let _ = tx.send(AsyncResult::Diagnostics(Box::new(resp)));
                });
            }
        }

        if app.pending_status_refresh {
            app.pending_status_refresh = false;
            let bg = bg.clone();
            let tx = result_tx.clone();
            tokio::spawn(async move {
                let resp = ipc_call(&bg, Request::GetStatus).await;
                let result = match resp {
                    Ok(Response::Ok {
                        data:
                            ResponseData::Status {
                                uptime_secs,
                                daemon_pid,
                                accounts,
                                total_messages,
                                sync_statuses,
                                ..
                            },
                    }) => Ok(StatusSnapshot {
                        uptime_secs,
                        daemon_pid,
                        accounts,
                        total_messages,
                        sync_statuses,
                    }),
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                let _ = tx.send(AsyncResult::Status(result));
            });
        }

        if app.accounts_page.refresh_pending {
            app.accounts_page.refresh_pending = false;
            let bg = bg.clone();
            let tx = result_tx.clone();
            tokio::spawn(async move {
                let result = load_accounts_page_accounts(&bg).await;
                let _ = tx.send(AsyncResult::Accounts(result));
            });
        }

        if app.pending_labels_refresh {
            app.pending_labels_refresh = false;
            let bg = bg.clone();
            let tx = result_tx.clone();
            tokio::spawn(async move {
                let resp = ipc_call(&bg, Request::ListLabels { account_id: None }).await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::Labels { labels },
                    }) => Ok(labels),
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                let _ = tx.send(AsyncResult::Labels(result));
            });
        }

        if app.pending_all_envelopes_refresh {
            app.pending_all_envelopes_refresh = false;
            let bg = bg.clone();
            let tx = result_tx.clone();
            tokio::spawn(async move {
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
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                let _ = tx.send(AsyncResult::AllEnvelopes(result));
            });
        }

        if app.pending_subscriptions_refresh {
            app.pending_subscriptions_refresh = false;
            let bg = bg.clone();
            let tx = result_tx.clone();
            tokio::spawn(async move {
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
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                let _ = tx.send(AsyncResult::Subscriptions(result));
            });
        }

        if let Some(account) = app.pending_account_save.take() {
            let bg = bg.clone();
            let tx = result_tx.clone();
            tokio::spawn(async move {
                let result = run_account_save_workflow(&bg, account).await;
                let _ = tx.send(AsyncResult::AccountOperation(result));
            });
        }

        if let Some(account) = app.pending_account_test.take() {
            let bg = bg.clone();
            let tx = result_tx.clone();
            tokio::spawn(async move {
                let result =
                    request_account_operation(&bg, Request::TestAccountConfig { account }).await;
                let _ = tx.send(AsyncResult::AccountOperation(result));
            });
        }

        if let Some((account, reauthorize)) = app.pending_account_authorize.take() {
            let bg = bg.clone();
            let tx = result_tx.clone();
            tokio::spawn(async move {
                let result = request_account_operation(
                    &bg,
                    Request::AuthorizeAccountConfig {
                        account,
                        reauthorize,
                    },
                )
                .await;
                let _ = tx.send(AsyncResult::AccountOperation(result));
            });
        }

        if let Some(key) = app.pending_account_set_default.take() {
            let bg = bg.clone();
            let tx = result_tx.clone();
            tokio::spawn(async move {
                let result =
                    request_account_operation(&bg, Request::SetDefaultAccount { key }).await;
                let _ = tx.send(AsyncResult::AccountOperation(result));
            });
        }

        if app.pending_bug_report {
            app.pending_bug_report = false;
            let bg = bg.clone();
            let tx = result_tx.clone();
            tokio::spawn(async move {
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
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                let _ = tx.send(AsyncResult::BugReport(result));
            });
        }

        if let Some(pending) = app.pending_attachment_action.take() {
            let bg = bg.clone();
            let tx = result_tx.clone();
            tokio::spawn(async move {
                let request = match pending.operation {
                    AttachmentOperation::Open => Request::OpenAttachment {
                        message_id: pending.message_id,
                        attachment_id: pending.attachment_id,
                    },
                    AttachmentOperation::Download => Request::DownloadAttachment {
                        message_id: pending.message_id,
                        attachment_id: pending.attachment_id,
                    },
                };
                let resp = ipc_call(&bg, request).await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::AttachmentFile { file },
                    }) => Ok(file),
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                let _ = tx.send(AsyncResult::AttachmentFile {
                    operation: pending.operation,
                    result,
                });
            });
        }

        // Spawn non-blocking label envelope fetch
        if let Some(label_id) = app.pending_label_fetch.take() {
            let bg = bg.clone();
            let tx = result_tx.clone();
            tokio::spawn(async move {
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
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                let _ = tx.send(AsyncResult::LabelEnvelopes(envelopes));
            });
        }

        // Drain pending mutations
        for (req, effect) in app.pending_mutation_queue.drain(..) {
            let bg = bg.clone();
            let tx = result_tx.clone();
            tokio::spawn(async move {
                let resp = ipc_call(&bg, req).await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::Ack,
                    }) => Ok(effect),
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                let _ = tx.send(AsyncResult::MutationResult(result));
            });
        }

        // Handle thread export (uses daemon ExportThread which runs mxr-export)
        if let Some(thread_id) = app.pending_export_thread.take() {
            let bg = bg.clone();
            let tx = result_tx.clone();
            tokio::spawn(async move {
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
                        match std::fs::write(&path, &content) {
                            Ok(()) => Ok(format!("Exported to {}", path.display())),
                            Err(e) => Err(MxrError::Ipc(format!("Write failed: {e}"))),
                        }
                    }
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                let _ = tx.send(AsyncResult::ExportResult(result));
            });
        }

        // Handle compose actions
        if let Some(compose_action) = app.pending_compose.take() {
            let bg = bg.clone();
            let tx = result_tx.clone();
            tokio::spawn(async move {
                let result = handle_compose_action(&bg, compose_action).await;
                let _ = tx.send(AsyncResult::ComposeReady(result));
            });
        }

        tokio::select! {
            event = events.next() => {
                if let Some(Ok(Event::Key(key))) = event {
                    if let Some(action) = app.handle_key(key) {
                        app.apply(action);
                    }
                }
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
                                if session_id != app.search_page.session_id {
                                    continue;
                                }

                                if append {
                                    app.search_page.results.extend(results.envelopes);
                                    app.search_page.scores.extend(results.scores);
                                } else {
                                    app.search_page.results = results.envelopes;
                                    app.search_page.scores = results.scores;
                                    app.search_page.selected_index = 0;
                                    app.search_page.scroll_offset = 0;
                                }

                                app.search_page.has_more = results.has_more;
                                app.search_page.loading_more = false;
                                app.search_page.ui_status = app::SearchUiStatus::Loaded;
                                app.search_page.session_active =
                                    !app.search_page.query.is_empty()
                                        || !app.search_page.results.is_empty();

                                if app.search_page.load_to_end {
                                    if app.search_page.has_more {
                                        app.load_more_search_results();
                                    } else {
                                        app.search_page.load_to_end = false;
                                        if app.search_row_count() > 0 {
                                            app.search_page.selected_index =
                                                app.search_row_count() - 1;
                                            app.ensure_search_visible();
                                            app.auto_preview_search();
                                        }
                                    }
                                } else {
                                    app.search_page.selected_index = app
                                        .search_page
                                        .selected_index
                                        .min(app.search_row_count().saturating_sub(1));
                                    if app.screen == app::Screen::Search {
                                        app.ensure_search_visible();
                                        if app.search_page.result_selected {
                                            app.auto_preview_search();
                                        } else {
                                            app.clear_message_view_state();
                                        }
                                    }
                                }
                            }
                            app::SearchTarget::Mailbox => {
                                if session_id != app.mailbox_search_session_id {
                                    continue;
                                }
                                app.envelopes = results.envelopes;
                                app.selected_index = 0;
                                app.scroll_offset = 0;
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
                                    if session_id != app.search_page.session_id {
                                        continue;
                                    }
                                    app.search_page.loading_more = false;
                                    app.search_page.load_to_end = false;
                                    app.search_page.count_pending = false;
                                    app.search_page.total_count = None;
                                    app.search_page.ui_status = app::SearchUiStatus::Error;
                                }
                                app::SearchTarget::Mailbox => {
                                    if session_id != app.mailbox_search_session_id {
                                        continue;
                                    }
                                    app.envelopes = app.all_envelopes.clone();
                                }
                            }
                            app.status_message = Some(format!("Search failed: {error}"));
                        }
                        AsyncResult::SearchCount {
                            session_id,
                            result: Ok(count),
                        } => {
                            if session_id != app.search_page.session_id {
                                continue;
                            }
                            app.search_page.total_count = Some(count);
                            app.search_page.count_pending = false;
                            if app.search_page.ui_status != app::SearchUiStatus::Error {
                                app.search_page.ui_status = app::SearchUiStatus::Loaded;
                            }
                        }
                        AsyncResult::SearchCount {
                            session_id,
                            result: Err(error),
                        } => {
                            if session_id != app.search_page.session_id {
                                continue;
                            }
                            app.search_page.count_pending = false;
                            if app.search_page.results.is_empty()
                                && matches!(app.search_page.ui_status, app::SearchUiStatus::Searching)
                            {
                                app.search_page.ui_status = app::SearchUiStatus::Error;
                            }
                            app.status_message = Some(format!("Search count failed: {error}"));
                        }
                        AsyncResult::Rules(Ok(rules)) => {
                            app.rules_page.rules = rules;
                            app.rules_page.selected_index = app
                                .rules_page
                                .selected_index
                                .min(app.rules_page.rules.len().saturating_sub(1));
                            app.refresh_selected_rule_panel();
                        }
                        AsyncResult::Rules(Err(e)) => {
                            app.rules_page.status = Some(format!("Rules error: {e}"));
                        }
                        AsyncResult::RuleDetail(Ok(rule)) => {
                            app.rules_page.detail = Some(rule);
                            app.rules_page.panel = app::RulesPanel::Details;
                        }
                        AsyncResult::RuleDetail(Err(e)) => {
                            app.rules_page.status = Some(format!("Rule error: {e}"));
                        }
                        AsyncResult::RuleHistory(Ok(entries)) => {
                            app.rules_page.history = entries;
                        }
                        AsyncResult::RuleHistory(Err(e)) => {
                            app.rules_page.status = Some(format!("History error: {e}"));
                        }
                        AsyncResult::RuleDryRun(Ok(results)) => {
                            app.rules_page.dry_run = results;
                        }
                        AsyncResult::RuleDryRun(Err(e)) => {
                            app.rules_page.status = Some(format!("Dry-run error: {e}"));
                        }
                        AsyncResult::RuleForm(Ok(form)) => {
                            app.rules_page.form.visible = true;
                            app.rules_page.form.existing_rule = form.id;
                            app.rules_page.form.name = form.name;
                            app.rules_page.form.condition = form.condition;
                            app.rules_page.form.action = form.action;
                            app.rules_page.form.priority = form.priority.to_string();
                            app.rules_page.form.enabled = form.enabled;
                            app.rules_page.form.active_field = 0;
                            app.sync_rule_form_editors();
                            app.rules_page.panel = app::RulesPanel::Form;
                        }
                        AsyncResult::RuleForm(Err(e)) => {
                            app.rules_page.status = Some(format!("Form error: {e}"));
                        }
                        AsyncResult::RuleDeleted(Ok(())) => {
                            app.rules_page.status = Some("Rule deleted".into());
                            app.rules_page.refresh_pending = true;
                        }
                        AsyncResult::RuleDeleted(Err(e)) => {
                            app.rules_page.status = Some(format!("Delete error: {e}"));
                        }
                        AsyncResult::RuleUpsert(Ok(rule)) => {
                            app.rules_page.detail = Some(rule.clone());
                            app.rules_page.form.visible = false;
                            app.rules_page.panel = app::RulesPanel::Details;
                            app.rules_page.status = Some("Rule saved".into());
                            app.rules_page.refresh_pending = true;
                        }
                        AsyncResult::RuleUpsert(Err(e)) => {
                            app.rules_page.status = Some(format!("Save error: {e}"));
                        }
                        AsyncResult::Diagnostics(result) => {
                            app.diagnostics_page.pending_requests =
                                app.diagnostics_page.pending_requests.saturating_sub(1);
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
                                    app.diagnostics_page.doctor = Some(report);
                                }
                                Response::Ok {
                                    data: ResponseData::EventLogEntries { entries },
                                } => {
                                    app.diagnostics_page.events = entries;
                                }
                                Response::Ok {
                                    data: ResponseData::LogLines { lines },
                                } => {
                                    app.diagnostics_page.logs = lines;
                                }
                                Response::Error { message } => {
                                    app.diagnostics_page.status = Some(message);
                                }
                                _ => {}
                                },
                                Err(e) => {
                                    app.diagnostics_page.status =
                                        Some(format!("Diagnostics error: {e}"));
                                }
                            }
                        }
                        AsyncResult::Status(Ok(snapshot)) => {
                            app.apply_status_snapshot(
                                snapshot.uptime_secs,
                                snapshot.daemon_pid,
                                snapshot.accounts,
                                snapshot.total_messages,
                                snapshot.sync_statuses,
                            );
                        }
                        AsyncResult::Status(Err(e)) => {
                            app.status_message = Some(format!("Status refresh failed: {e}"));
                        }
                        AsyncResult::Accounts(Ok(accounts)) => {
                            app.accounts_page.accounts = accounts;
                            app.accounts_page.selected_index = app
                                .accounts_page
                                .selected_index
                                .min(app.accounts_page.accounts.len().saturating_sub(1));
                            if app.accounts_page.accounts.is_empty() {
                                app.accounts_page.onboarding_required = true;
                            } else {
                                app.accounts_page.onboarding_required = false;
                                app.accounts_page.onboarding_modal_open = false;
                                app.maybe_show_feature_onboarding();
                            }
                        }
                        AsyncResult::Accounts(Err(e)) => {
                            app.accounts_page.status = Some(format!("Accounts error: {e}"));
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
                            app.status_message =
                                Some(format!("Mailbox refresh failed: {e}"));
                        }
                        AsyncResult::AccountOperation(Ok(result)) => {
                            let was_switch = app.pending_account_switch;
                            app.pending_account_switch = false;
                            app.apply_account_operation_result(result);
                            if was_switch {
                                // Clear stale data from previous account
                                app.viewing_envelope = None;
                                app.envelopes.clear();
                                app.all_envelopes.clear();
                                app.search_page.results.clear();
                                app.subscriptions_page.entries.clear();
                                app.active_label = None;
                                app.pending_active_label = None;
                                app.pending_label_fetch = None;
                                app.selected_index = 0;
                                app.scroll_offset = 0;
                                // Trigger full refresh for new account
                                app.pending_labels_refresh = true;
                                app.pending_all_envelopes_refresh = true;
                                app.pending_subscriptions_refresh = true;
                                app.pending_status_refresh = true;
                                app.desired_system_mailbox = Some("INBOX".into());
                                app.status_message = Some("Account switched".into());
                            }
                        }
                        AsyncResult::AccountOperation(Err(e)) => {
                            app.accounts_page.operation_in_flight = false;
                            app.accounts_page.throbber = Default::default();
                            app.accounts_page.status = Some(format!("Account error: {e}"));
                            app.error_modal = Some(app::ErrorModalState::new(
                                "Account Operation Failed",
                                format!("The account test or save request failed.\n\n{e}"),
                            ));
                        }
                        AsyncResult::BugReport(Ok(content)) => {
                            let filename = format!(
                                "mxr-bug-report-{}.md",
                                chrono::Utc::now().format("%Y%m%d-%H%M%S")
                            );
                            let path = std::env::temp_dir().join(filename);
                            match std::fs::write(&path, &content) {
                                Ok(()) => {
                                    app.diagnostics_page.status =
                                        Some(format!("Bug report saved to {}", path.display()));
                                }
                                Err(e) => {
                                    app.diagnostics_page.status =
                                        Some(format!("Bug report write failed: {e}"));
                                }
                            }
                        }
                        AsyncResult::BugReport(Err(e)) => {
                            app.diagnostics_page.status = Some(format!("Bug report error: {e}"));
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
                            app.attachment_panel.status = Some(message.clone());
                            app.status_message = Some(message);
                        }
                        AsyncResult::AttachmentFile {
                            result: Err(e), ..
                        } => {
                            let message = format!("Attachment error: {e}");
                            app.attachment_panel.status = Some(message.clone());
                            app.status_message = Some(message);
                        }
                        AsyncResult::LabelEnvelopes(Ok(envelopes)) => {
                            let selected_id =
                                app.selected_mail_row().map(|row| row.representative.id);
                            app.envelopes = envelopes;
                            // Only update active_label when this is a user-initiated
                            // label switch (pending_active_label was set). For
                            // refresh-only fetches triggered by sync or mutations,
                            // pending_active_label is None — preserve current label.
                            if app.pending_active_label.is_some() {
                                app.active_label = app.pending_active_label.take();
                            }
                            restore_mail_list_selection(&mut app, selected_id);
                            app.queue_body_window();
                        }
                        AsyncResult::LabelEnvelopes(Err(e)) => {
                            app.pending_active_label = None;
                            app.status_message = Some(format!("Label filter failed: {e}"));
                        }
                        AsyncResult::Bodies { requested, result: Ok(bodies) } => {
                            let mut returned = std::collections::HashSet::new();
                            for body in bodies {
                                returned.insert(body.message_id.clone());
                                app.resolve_body_success(body);
                            }
                            for message_id in requested {
                                if !returned.contains(&message_id) {
                                    app.resolve_body_fetch_error(
                                        &message_id,
                                        "body not available".into(),
                                    );
                                }
                            }
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
                            result: Ok((thread, messages)),
                        } => {
                            app.resolve_thread_success(thread, messages);
                            let _ = thread_id;
                        }
                        AsyncResult::Thread {
                            thread_id,
                            result: Err(_),
                        } => {
                            app.resolve_thread_fetch_error(&thread_id);
                        }
                        AsyncResult::MutationResult(Ok(effect)) => {
                            app.finish_pending_mutation();
                            let show_completion_status = app.pending_mutation_count == 0;
                            match effect {
                                app::MutationEffect::RemoveFromList(id) => {
                                    app.apply_removed_message_ids(std::slice::from_ref(&id));
                                    if show_completion_status {
                                        app.status_message = Some("Done".into());
                                    }
                                    app.pending_subscriptions_refresh = true;
                                }
                                app::MutationEffect::RemoveFromListMany(ids) => {
                                    app.apply_removed_message_ids(&ids);
                                    if show_completion_status {
                                        app.status_message = Some("Done".into());
                                    }
                                    app.pending_subscriptions_refresh = true;
                                }
                                app::MutationEffect::UpdateFlags { message_id, flags } => {
                                    app.apply_local_flags(&message_id, flags);
                                    if show_completion_status {
                                        app.status_message = Some("Done".into());
                                    }
                                }
                                app::MutationEffect::UpdateFlagsMany { updates } => {
                                    app.apply_local_flags_many(&updates);
                                    if show_completion_status {
                                        app.status_message = Some("Done".into());
                                    }
                                }
                                app::MutationEffect::RefreshList => {
                                    if let Some(label_id) = app.active_label.clone() {
                                        app.pending_label_fetch = Some(label_id);
                                    }
                                    app.pending_subscriptions_refresh = true;
                                    if show_completion_status {
                                        app.status_message = Some("Synced".into());
                                    }
                                }
                                app::MutationEffect::ModifyLabels {
                                    message_ids,
                                    add,
                                    remove,
                                    status,
                                } => {
                                    app.apply_local_label_refs(&message_ids, &add, &remove);
                                    if show_completion_status {
                                        app.status_message = Some(status);
                                    }
                                }
                                app::MutationEffect::StatusOnly(msg) => {
                                    if show_completion_status {
                                        app.status_message = Some(msg);
                                    }
                                }
                            }
                        }
                        AsyncResult::MutationResult(Err(e)) => {
                            app.finish_pending_mutation();
                            app.refresh_mailbox_after_mutation_failure();
                            app.show_mutation_failure(&e);
                        }
                        AsyncResult::ComposeReady(Ok(data)) => {
                            // Restore terminal, spawn editor, then re-init terminal
                            ratatui::restore();
                            let editor = mxr_compose::editor::resolve_editor(None);
                            let status = std::process::Command::new(&editor)
                                .arg(format!("+{}", data.cursor_line))
                                .arg(&data.draft_path)
                                .status();
                            terminal = ratatui::init();
                            match status {
                                Ok(s) if s.success() => {
                                    match pending_send_from_edited_draft(&data) {
                                        Ok(Some(pending)) => {
                                            app.pending_send_confirm = Some(pending);
                                        }
                                        Ok(None) => {}
                                        Err(message) => {
                                            app.status_message = Some(message);
                                        }
                                    }
                                }
                                Ok(_) => {
                                    // Editor exited abnormally — user probably :q! to discard
                                    app.status_message = Some("Draft discarded".into());
                                    let _ = std::fs::remove_file(&data.draft_path);
                                }
                                Err(e) => {
                                    app.status_message =
                                        Some(format!("Failed to launch editor: {e}"));
                                }
                            }
                        }
                        AsyncResult::ComposeReady(Err(e)) => {
                            app.status_message = Some(format!("Compose error: {e}"));
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
                            app.pending_subscriptions_refresh = true;
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
                        AsyncResult::DaemonEvent(event) => handle_daemon_event(&mut app, event),
                    }
                }
            }
            _ = tokio::time::sleep(timeout) => {
                app.tick();
            }
        }

        if app.should_quit {
            break;
        }
    }

    ratatui::restore();
    Ok(())
}

pub(crate) enum AsyncResult {
    Search {
        target: app::SearchTarget,
        append: bool,
        session_id: u64,
        result: Result<SearchResultData, MxrError>,
    },
    SearchCount {
        session_id: u64,
        result: Result<u32, MxrError>,
    },
    Rules(Result<Vec<serde_json::Value>, MxrError>),
    RuleDetail(Result<serde_json::Value, MxrError>),
    RuleHistory(Result<Vec<serde_json::Value>, MxrError>),
    RuleDryRun(Result<Vec<serde_json::Value>, MxrError>),
    RuleForm(Result<mxr_protocol::RuleFormData, MxrError>),
    RuleDeleted(Result<(), MxrError>),
    RuleUpsert(Result<serde_json::Value, MxrError>),
    Diagnostics(Box<Result<Response, MxrError>>),
    Status(Result<StatusSnapshot, MxrError>),
    Accounts(Result<Vec<mxr_protocol::AccountSummaryData>, MxrError>),
    Labels(Result<Vec<mxr_core::Label>, MxrError>),
    AllEnvelopes(Result<Vec<mxr_core::Envelope>, MxrError>),
    Subscriptions(Result<Vec<mxr_core::types::SubscriptionSummary>, MxrError>),
    AccountOperation(Result<mxr_protocol::AccountOperationResult, MxrError>),
    BugReport(Result<String, MxrError>),
    AttachmentFile {
        operation: AttachmentOperation,
        result: Result<mxr_protocol::AttachmentFile, MxrError>,
    },
    LabelEnvelopes(Result<Vec<mxr_core::Envelope>, MxrError>),
    Bodies {
        requested: Vec<mxr_core::MessageId>,
        result: Result<Vec<mxr_core::MessageBody>, MxrError>,
    },
    HtmlImageAssets {
        message_id: mxr_core::MessageId,
        allow_remote: bool,
        result: Result<Vec<mxr_core::types::HtmlImageAsset>, MxrError>,
    },
    HtmlImageDecoded {
        key: crate::terminal_images::HtmlImageKey,
        result: Result<image::DynamicImage, MxrError>,
    },
    HtmlImageResized {
        key: crate::terminal_images::HtmlImageKey,
        result: Result<ratatui_image::thread::ResizeResponse, MxrError>,
    },
    Thread {
        thread_id: mxr_core::ThreadId,
        result: Result<(mxr_core::Thread, Vec<mxr_core::Envelope>), MxrError>,
    },
    MutationResult(Result<app::MutationEffect, MxrError>),
    ComposeReady(Result<ComposeReadyData, MxrError>),
    ExportResult(Result<String, MxrError>),
    Unsubscribe(Result<UnsubscribeResultData, MxrError>),
    DaemonEvent(DaemonEvent),
}

pub(crate) struct ComposeReadyData {
    draft_path: std::path::PathBuf,
    cursor_line: usize,
    initial_content: String,
}

pub(crate) struct SearchResultData {
    envelopes: Vec<mxr_core::types::Envelope>,
    scores: std::collections::HashMap<mxr_core::MessageId, f32>,
    has_more: bool,
}

pub(crate) struct StatusSnapshot {
    uptime_secs: u64,
    daemon_pid: Option<u32>,
    accounts: Vec<String>,
    total_messages: u32,
    sync_statuses: Vec<mxr_protocol::AccountSyncStatus>,
}

pub(crate) struct UnsubscribeResultData {
    archived_ids: Vec<mxr_core::MessageId>,
    message: String,
}

async fn handle_compose_action(
    bg: &mpsc::UnboundedSender<IpcRequest>,
    action: ComposeAction,
) -> Result<ComposeReadyData, MxrError> {
    let from = get_account_email(bg).await?;

    let kind = match action {
        ComposeAction::EditDraft(path) => {
            // Re-edit existing draft — skip creating a new file
            let cursor_line = 1;
            return Ok(ComposeReadyData {
                draft_path: path.clone(),
                cursor_line,
                initial_content: std::fs::read_to_string(&path)
                    .map_err(|e| MxrError::Ipc(e.to_string()))?,
            });
        }
        ComposeAction::New => mxr_compose::ComposeKind::New,
        ComposeAction::NewWithTo(to) => mxr_compose::ComposeKind::NewWithTo { to },
        ComposeAction::Reply { message_id } => {
            let resp = ipc_call(
                bg,
                Request::PrepareReply {
                    message_id,
                    reply_all: false,
                },
            )
            .await?;
            match resp {
                Response::Ok {
                    data: ResponseData::ReplyContext { context },
                } => mxr_compose::ComposeKind::Reply {
                    in_reply_to: context.in_reply_to,
                    references: context.references,
                    to: context.reply_to,
                    cc: context.cc,
                    subject: context.subject,
                    thread_context: context.thread_context,
                },
                Response::Error { message } => return Err(MxrError::Ipc(message)),
                _ => return Err(MxrError::Ipc("unexpected response".into())),
            }
        }
        ComposeAction::ReplyAll { message_id } => {
            let resp = ipc_call(
                bg,
                Request::PrepareReply {
                    message_id,
                    reply_all: true,
                },
            )
            .await?;
            match resp {
                Response::Ok {
                    data: ResponseData::ReplyContext { context },
                } => mxr_compose::ComposeKind::Reply {
                    in_reply_to: context.in_reply_to,
                    references: context.references,
                    to: context.reply_to,
                    cc: context.cc,
                    subject: context.subject,
                    thread_context: context.thread_context,
                },
                Response::Error { message } => return Err(MxrError::Ipc(message)),
                _ => return Err(MxrError::Ipc("unexpected response".into())),
            }
        }
        ComposeAction::Forward { message_id } => {
            let resp = ipc_call(bg, Request::PrepareForward { message_id }).await?;
            match resp {
                Response::Ok {
                    data: ResponseData::ForwardContext { context },
                } => mxr_compose::ComposeKind::Forward {
                    subject: context.subject,
                    original_context: context.forwarded_content,
                },
                Response::Error { message } => return Err(MxrError::Ipc(message)),
                _ => return Err(MxrError::Ipc("unexpected response".into())),
            }
        }
    };

    let (path, cursor_line) =
        mxr_compose::create_draft_file(kind, &from).map_err(|e| MxrError::Ipc(e.to_string()))?;

    Ok(ComposeReadyData {
        draft_path: path.clone(),
        cursor_line,
        initial_content: std::fs::read_to_string(&path)
            .map_err(|e| MxrError::Ipc(e.to_string()))?,
    })
}

async fn get_account_email(bg: &mpsc::UnboundedSender<IpcRequest>) -> Result<String, MxrError> {
    let resp = ipc_call(bg, Request::ListAccounts).await?;
    match resp {
        Response::Ok {
            data: ResponseData::Accounts { mut accounts },
        } => {
            if let Some(index) = accounts.iter().position(|account| account.is_default) {
                Ok(accounts.remove(index).email)
            } else {
                accounts
                    .into_iter()
                    .next()
                    .map(|account| account.email)
                    .ok_or_else(|| MxrError::Ipc("No runtime account configured".into()))
            }
        }
        Response::Error { message } => Err(MxrError::Ipc(message)),
        _ => Err(MxrError::Ipc("Unexpected account response".into())),
    }
}

fn pending_send_from_edited_draft(data: &ComposeReadyData) -> Result<Option<PendingSend>, String> {
    let content = std::fs::read_to_string(&data.draft_path)
        .map_err(|e| format!("Failed to read draft: {e}"))?;
    let unchanged = content == data.initial_content;

    let (fm, body) = mxr_compose::frontmatter::parse_compose_file(&content)
        .map_err(|e| format!("Parse error: {e}"))?;
    let issues = mxr_compose::validate_draft(&fm, &body);
    let has_errors = issues.iter().any(|issue| issue.is_error());
    if has_errors {
        let msgs: Vec<String> = issues.iter().map(|issue| issue.to_string()).collect();
        return Err(format!("Draft errors: {}", msgs.join("; ")));
    }

    Ok(Some(PendingSend {
        fm,
        body,
        draft_path: data.draft_path.clone(),
        allow_send: !unchanged,
    }))
}

fn daemon_socket_path() -> std::path::PathBuf {
    config_socket_path()
}

async fn request_account_operation(
    bg: &mpsc::UnboundedSender<IpcRequest>,
    request: Request,
) -> Result<mxr_protocol::AccountOperationResult, MxrError> {
    let resp = ipc_call(bg, request).await;
    match resp {
        Ok(Response::Ok {
            data: ResponseData::AccountOperation { result },
        }) => Ok(result),
        Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
        Err(e) => Err(e),
        _ => Err(MxrError::Ipc("unexpected response".into())),
    }
}

async fn run_account_save_workflow(
    bg: &mpsc::UnboundedSender<IpcRequest>,
    account: mxr_protocol::AccountConfigData,
) -> Result<mxr_protocol::AccountOperationResult, MxrError> {
    let mut result = if matches!(
        account.sync,
        Some(mxr_protocol::AccountSyncConfigData::Gmail { .. })
    ) {
        request_account_operation(
            bg,
            Request::AuthorizeAccountConfig {
                account: account.clone(),
                reauthorize: false,
            },
        )
        .await?
    } else {
        empty_account_operation_result()
    };

    if result.auth.as_ref().is_some_and(|step| !step.ok) {
        return Ok(result);
    }

    let save_result = request_account_operation(
        bg,
        Request::UpsertAccountConfig {
            account: account.clone(),
        },
    )
    .await?;
    merge_account_operation_result(&mut result, save_result);

    if result.save.as_ref().is_some_and(|step| !step.ok) {
        return Ok(result);
    }

    let test_result = request_account_operation(bg, Request::TestAccountConfig { account }).await?;
    merge_account_operation_result(&mut result, test_result);

    Ok(result)
}

fn empty_account_operation_result() -> mxr_protocol::AccountOperationResult {
    mxr_protocol::AccountOperationResult {
        ok: true,
        summary: String::new(),
        save: None,
        auth: None,
        sync: None,
        send: None,
    }
}

fn merge_account_operation_result(
    base: &mut mxr_protocol::AccountOperationResult,
    next: mxr_protocol::AccountOperationResult,
) {
    base.ok &= next.ok;
    if !next.summary.is_empty() {
        if base.summary.is_empty() {
            base.summary = next.summary;
        } else {
            base.summary = format!("{} | {}", base.summary, next.summary);
        }
    }
    if next.save.is_some() {
        base.save = next.save;
    }
    if next.auth.is_some() {
        base.auth = next.auth;
    }
    if next.sync.is_some() {
        base.sync = next.sync;
    }
    if next.send.is_some() {
        base.send = next.send;
    }
}

fn handle_daemon_event(app: &mut App, event: DaemonEvent) {
    match event {
        DaemonEvent::SyncCompleted {
            messages_synced, ..
        } => {
            app.pending_labels_refresh = true;
            app.pending_all_envelopes_refresh = true;
            app.pending_subscriptions_refresh = true;
            app.pending_status_refresh = true;
            if let Some(label_id) = app.active_label.clone() {
                app.pending_label_fetch = Some(label_id);
            }
            if messages_synced > 0 {
                app.status_message = Some(format!("Synced {messages_synced} messages"));
            }
        }
        DaemonEvent::LabelCountsUpdated { counts } => {
            let selected_sidebar = app.selected_sidebar_key();
            for count in &counts {
                if let Some(label) = app
                    .labels
                    .iter_mut()
                    .find(|label| label.id == count.label_id)
                {
                    label.unread_count = count.unread_count;
                    label.total_count = count.total_count;
                }
            }
            app.restore_sidebar_selection(selected_sidebar);
        }
        DaemonEvent::SyncError { account_id, error } => {
            app.error_modal = Some(app::ErrorModalState::new(
                "Sync Failed",
                format!("Account: {account_id}\n\n{error}"),
            ));
            app.status_message = Some(format!("Sync error: {error}"));
            app.pending_status_refresh = true;
        }
        _ => {}
    }
}

fn apply_all_envelopes_refresh(app: &mut App, envelopes: Vec<mxr_core::Envelope>) {
    let selected_id = (app.active_label.is_none()
        && app.pending_active_label.is_none()
        && !app.search_active
        && app.mailbox_view == app::MailboxView::Messages)
        .then(|| app.selected_mail_row().map(|row| row.representative.id))
        .flatten();
    app.all_envelopes = envelopes;
    if app.active_label.is_none() && app.pending_active_label.is_none() && !app.search_active {
        app.envelopes = app
            .all_envelopes
            .iter()
            .filter(|envelope| !envelope.flags.contains(mxr_core::MessageFlags::TRASH))
            .cloned()
            .collect();
        if app.mailbox_view == app::MailboxView::Messages {
            restore_mail_list_selection(app, selected_id);
        } else {
            app.selected_index = app
                .selected_index
                .min(app.subscriptions_page.entries.len().saturating_sub(1));
        }
        app.queue_body_window();
    }
}

fn apply_labels_refresh(app: &mut App, mut labels: Vec<mxr_core::Label>) {
    let selected_sidebar = app.selected_sidebar_key();
    let mut preserved_label_ids = std::collections::HashSet::new();
    if let Some(app::SidebarSelectionKey::Label(label_id)) = selected_sidebar.as_ref() {
        preserved_label_ids.insert(label_id.clone());
    }
    if let Some(label_id) = app.pending_active_label.as_ref() {
        preserved_label_ids.insert(label_id.clone());
    }
    if let Some(label_id) = app.active_label.as_ref() {
        preserved_label_ids.insert(label_id.clone());
    }

    for label_id in preserved_label_ids {
        if labels.iter().any(|label| label.id == label_id) {
            continue;
        }
        if let Some(existing) = app
            .labels
            .iter()
            .find(|label| label.id == label_id)
            .cloned()
        {
            labels.push(mxr_core::Label {
                unread_count: 0,
                total_count: 0,
                ..existing
            });
        }
    }

    app.labels = labels;
    app.restore_sidebar_selection(selected_sidebar);
    app.resolve_desired_system_mailbox();
}

fn restore_mail_list_selection(app: &mut App, selected_id: Option<mxr_core::MessageId>) {
    let row_count = app.mail_list_rows().len();
    if row_count == 0 {
        app.selected_index = 0;
        app.scroll_offset = 0;
        return;
    }

    if let Some(id) = selected_id {
        if let Some(position) = app
            .mail_list_rows()
            .iter()
            .position(|row| row.representative.id == id)
        {
            app.selected_index = position;
        } else {
            app.selected_index = app.selected_index.min(row_count.saturating_sub(1));
        }
    } else {
        app.selected_index = 0;
    }

    let visible_height = app.visible_height.max(1);
    if app.selected_index < app.scroll_offset {
        app.scroll_offset = app.selected_index;
    } else if app.selected_index >= app.scroll_offset + visible_height {
        app.scroll_offset = app.selected_index + 1 - visible_height;
    }
}

async fn load_accounts_page_accounts(
    bg: &mpsc::UnboundedSender<IpcRequest>,
) -> Result<Vec<mxr_protocol::AccountSummaryData>, MxrError> {
    match ipc_call(bg, Request::ListAccounts).await {
        Ok(Response::Ok {
            data: ResponseData::Accounts { accounts },
        }) if !accounts.is_empty() => Ok(accounts),
        Ok(Response::Ok {
            data: ResponseData::Accounts { .. },
        })
        | Ok(Response::Error { .. })
        | Err(_) => load_config_account_summaries(bg).await,
        Ok(_) => Err(MxrError::Ipc("unexpected response".into())),
    }
}

async fn load_config_account_summaries(
    bg: &mpsc::UnboundedSender<IpcRequest>,
) -> Result<Vec<mxr_protocol::AccountSummaryData>, MxrError> {
    let resp = ipc_call(bg, Request::ListAccountsConfig).await?;
    match resp {
        Response::Ok {
            data: ResponseData::AccountsConfig { accounts },
        } => Ok(accounts
            .into_iter()
            .map(account_config_to_summary)
            .collect()),
        Response::Error { message } => Err(MxrError::Ipc(message)),
        _ => Err(MxrError::Ipc("unexpected response".into())),
    }
}

fn account_config_to_summary(
    account: mxr_protocol::AccountConfigData,
) -> mxr_protocol::AccountSummaryData {
    let provider_kind = account
        .sync
        .as_ref()
        .map(account_sync_kind_label)
        .or_else(|| account.send.as_ref().map(account_send_kind_label))
        .unwrap_or_else(|| "unknown".to_string());
    let account_id = mxr_core::AccountId::from_provider_id(&provider_kind, &account.email);

    mxr_protocol::AccountSummaryData {
        account_id,
        key: Some(account.key),
        name: account.name,
        email: account.email,
        provider_kind,
        sync_kind: account.sync.as_ref().map(account_sync_kind_label),
        send_kind: account.send.as_ref().map(account_send_kind_label),
        enabled: true,
        is_default: account.is_default,
        source: mxr_protocol::AccountSourceData::Config,
        editable: mxr_protocol::AccountEditModeData::Full,
        sync: account.sync,
        send: account.send,
    }
}

fn account_sync_kind_label(sync: &mxr_protocol::AccountSyncConfigData) -> String {
    match sync {
        mxr_protocol::AccountSyncConfigData::Gmail { .. } => "gmail".to_string(),
        mxr_protocol::AccountSyncConfigData::Imap { .. } => "imap".to_string(),
    }
}

fn account_send_kind_label(send: &mxr_protocol::AccountSendConfigData) -> String {
    match send {
        mxr_protocol::AccountSendConfigData::Gmail => "gmail".to_string(),
        mxr_protocol::AccountSendConfigData::Smtp { .. } => "smtp".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::action::Action;
    use super::app::{
        ActivePane, App, BodySource, BodyViewMetadata, BodyViewState, LayoutMode, MutationEffect,
        PendingSearchRequest, Screen, SearchPane, SearchTarget, SidebarItem, SEARCH_PAGE_SIZE,
    };
    use super::input::InputHandler;
    use super::ui::command_palette::default_commands;
    use super::ui::command_palette::CommandPalette;
    use super::ui::search_bar::SearchBar;
    use super::ui::status_bar;
    use super::{
        app::MailListMode, apply_all_envelopes_refresh, handle_daemon_event,
        pending_send_from_edited_draft, ComposeReadyData, PendingSend,
    };
    use crate::test_fixtures::TestEnvelopeBuilder;
    use mxr_config::RenderConfig;
    use mxr_core::id::*;
    use mxr_core::types::*;
    use mxr_core::MxrError;
    use mxr_protocol::{DaemonEvent, LabelCount, MutationCommand, Request};
    use mxr_test_support::render_to_string;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn make_test_envelopes(count: usize) -> Vec<Envelope> {
        (0..count)
            .map(|i| {
                TestEnvelopeBuilder::new()
                    .provider_id(format!("fake-{}", i))
                    .with_from_address(&format!("User {}", i), &format!("user{}@example.com", i))
                    .to(vec![])
                    .subject(format!("Subject {}", i))
                    .message_id_header(None)
                    .flags(if i % 2 == 0 {
                        MessageFlags::READ
                    } else {
                        MessageFlags::empty()
                    })
                    .snippet(format!("Snippet {}", i))
                    .size_bytes(1000)
                    .build()
            })
            .collect()
    }

    fn make_unsubscribe_envelope(
        account_id: AccountId,
        sender_email: &str,
        unsub: UnsubscribeMethod,
    ) -> Envelope {
        TestEnvelopeBuilder::new()
            .account_id(account_id)
            .provider_id("unsub-fixture")
            .with_from_address("Newsletter", sender_email)
            .to(vec![])
            .subject("Newsletter")
            .message_id_header(None)
            .snippet("newsletter")
            .size_bytes(42)
            .unsubscribe(unsub)
            .build()
    }

    #[test]
    fn input_j_moves_down() {
        let mut h = InputHandler::new();
        assert_eq!(
            h.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE)),
            Some(Action::MoveDown)
        );
    }

    #[test]
    fn input_k_moves_up() {
        let mut h = InputHandler::new();
        assert_eq!(
            h.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE)),
            Some(Action::MoveUp)
        );
    }

    #[test]
    fn input_gg_jumps_top() {
        let mut h = InputHandler::new();
        assert_eq!(
            h.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE)),
            None
        );
        assert_eq!(
            h.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE)),
            Some(Action::JumpTop)
        );
    }

    #[test]
    fn input_zz_centers() {
        let mut h = InputHandler::new();
        assert_eq!(
            h.handle_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE)),
            None
        );
        assert_eq!(
            h.handle_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE)),
            Some(Action::CenterCurrent)
        );
    }

    #[test]
    fn input_enter_opens() {
        let mut h = InputHandler::new();
        assert_eq!(
            h.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Some(Action::OpenSelected)
        );
    }

    #[test]
    fn input_o_opens() {
        let mut h = InputHandler::new();
        assert_eq!(
            h.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE)),
            Some(Action::OpenSelected)
        );
    }

    #[test]
    fn input_escape_back() {
        let mut h = InputHandler::new();
        assert_eq!(
            h.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            Some(Action::Back)
        );
    }

    #[test]
    fn input_q_quits() {
        let mut h = InputHandler::new();
        assert_eq!(
            h.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)),
            Some(Action::QuitView)
        );
    }

    #[test]
    fn input_hml_viewport() {
        let mut h = InputHandler::new();
        assert_eq!(
            h.handle_key(KeyEvent::new(KeyCode::Char('H'), KeyModifiers::SHIFT)),
            Some(Action::ViewportTop)
        );
        assert_eq!(
            h.handle_key(KeyEvent::new(KeyCode::Char('M'), KeyModifiers::SHIFT)),
            Some(Action::ViewportMiddle)
        );
        assert_eq!(
            h.handle_key(KeyEvent::new(KeyCode::Char('L'), KeyModifiers::SHIFT)),
            Some(Action::ViewportBottom)
        );
    }

    #[test]
    fn input_ctrl_du_page() {
        let mut h = InputHandler::new();
        assert_eq!(
            h.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)),
            Some(Action::PageDown)
        );
        assert_eq!(
            h.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL)),
            Some(Action::PageUp)
        );
    }

    #[test]
    fn app_move_down() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(5);
        app.apply(Action::MoveDown);
        assert_eq!(app.selected_index, 1);
    }

    #[test]
    fn app_move_up_at_zero() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(5);
        app.apply(Action::MoveUp);
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn app_jump_top() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(10);
        app.selected_index = 5;
        app.apply(Action::JumpTop);
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn app_switch_pane() {
        let mut app = App::new();
        assert_eq!(app.active_pane, ActivePane::MailList);
        app.apply(Action::SwitchPane);
        assert_eq!(app.active_pane, ActivePane::Sidebar);
        app.apply(Action::SwitchPane);
        assert_eq!(app.active_pane, ActivePane::MailList);
    }

    #[test]
    fn app_quit() {
        let mut app = App::new();
        app.apply(Action::QuitView);
        assert!(app.should_quit);
    }

    #[test]
    fn app_new_uses_default_reader_mode() {
        let app = App::new();
        assert!(app.reader_mode);
    }

    #[test]
    fn app_from_render_config_respects_reader_mode() {
        let config = RenderConfig {
            reader_mode: false,
            ..Default::default()
        };
        let app = App::from_render_config(&config);
        assert!(!app.reader_mode);
    }

    #[test]
    fn apply_runtime_config_updates_tui_settings() {
        let mut app = App::new();
        let mut config = mxr_config::MxrConfig::default();
        config.render.reader_mode = false;
        config.snooze.morning_hour = 7;
        config.appearance.theme = "light".into();

        app.apply_runtime_config(&config);

        assert!(!app.reader_mode);
        assert_eq!(app.snooze_config.morning_hour, 7);
        assert_eq!(
            app.theme.selection_fg,
            crate::theme::Theme::light().selection_fg
        );
    }

    #[test]
    fn edit_config_action_sets_pending_flag() {
        let mut app = App::new();

        app.apply(Action::EditConfig);

        assert!(app.pending_config_edit);
        assert_eq!(
            app.status_message.as_deref(),
            Some("Opening config in editor...")
        );
    }

    #[test]
    fn open_logs_action_sets_pending_flag() {
        let mut app = App::new();

        app.apply(Action::OpenLogs);

        assert!(app.pending_log_open);
        assert_eq!(
            app.status_message.as_deref(),
            Some("Opening log file in editor...")
        );
    }

    #[test]
    fn app_move_down_bounds() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(3);
        app.apply(Action::MoveDown);
        app.apply(Action::MoveDown);
        app.apply(Action::MoveDown);
        assert_eq!(app.selected_index, 2);
    }

    #[test]
    fn layout_mode_switching() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(3);
        assert_eq!(app.layout_mode, LayoutMode::TwoPane);
        app.apply(Action::OpenMessageView);
        assert_eq!(app.layout_mode, LayoutMode::ThreePane);
        app.apply(Action::CloseMessageView);
        assert_eq!(app.layout_mode, LayoutMode::TwoPane);
    }

    #[test]
    fn command_palette_toggle() {
        let mut p = CommandPalette::default();
        assert!(!p.visible);
        p.toggle(crate::action::UiContext::MailboxList);
        assert!(p.visible);
        p.toggle(crate::action::UiContext::MailboxList);
        assert!(!p.visible);
    }

    #[test]
    fn command_palette_fuzzy_filter() {
        let mut p = CommandPalette::default();
        p.toggle(crate::action::UiContext::MailboxList);
        p.on_char('i');
        p.on_char('n');
        p.on_char('b');
        let labels: Vec<&str> = p
            .filtered
            .iter()
            .map(|&i| p.commands[i].label.as_str())
            .collect();
        assert!(labels.contains(&"Go to Inbox"));
    }

    #[test]
    fn command_palette_shortcut_filter_finds_edit_config() {
        let mut p = CommandPalette::default();
        p.toggle(crate::action::UiContext::MailboxList);
        p.on_char('g');
        p.on_char('c');
        let labels: Vec<&str> = p
            .filtered
            .iter()
            .map(|&i| p.commands[i].label.as_str())
            .collect();
        assert!(labels.contains(&"Edit Config"));
    }

    #[test]
    fn unsubscribe_opens_confirm_modal_and_scopes_archive_to_sender_and_account() {
        let mut app = App::new();
        let account_id = AccountId::new();
        let other_account_id = AccountId::new();
        let target = make_unsubscribe_envelope(
            account_id.clone(),
            "news@example.com",
            UnsubscribeMethod::HttpLink {
                url: "https://example.com/unsub".into(),
            },
        );
        let same_sender_same_account = make_unsubscribe_envelope(
            account_id.clone(),
            "news@example.com",
            UnsubscribeMethod::None,
        );
        let same_sender_other_account = make_unsubscribe_envelope(
            other_account_id,
            "news@example.com",
            UnsubscribeMethod::None,
        );
        let different_sender_same_account =
            make_unsubscribe_envelope(account_id, "other@example.com", UnsubscribeMethod::None);

        app.envelopes = vec![target.clone()];
        app.all_envelopes = vec![
            target.clone(),
            same_sender_same_account.clone(),
            same_sender_other_account,
            different_sender_same_account,
        ];

        app.apply(Action::Unsubscribe);

        let pending = app
            .pending_unsubscribe_confirm
            .as_ref()
            .expect("unsubscribe modal should open");
        assert_eq!(pending.sender_email, "news@example.com");
        assert_eq!(pending.method_label, "browser link");
        assert_eq!(pending.archive_message_ids.len(), 2);
        assert!(pending.archive_message_ids.contains(&target.id));
        assert!(pending
            .archive_message_ids
            .contains(&same_sender_same_account.id));
    }

    #[test]
    fn unsubscribe_without_method_sets_status_error() {
        let mut app = App::new();
        let env = make_unsubscribe_envelope(
            AccountId::new(),
            "news@example.com",
            UnsubscribeMethod::None,
        );
        app.envelopes = vec![env];

        app.apply(Action::Unsubscribe);

        assert!(app.pending_unsubscribe_confirm.is_none());
        assert_eq!(
            app.status_message.as_deref(),
            Some("No unsubscribe option found for this message")
        );
    }

    #[test]
    fn unsubscribe_confirm_archive_populates_pending_action() {
        let mut app = App::new();
        let env = make_unsubscribe_envelope(
            AccountId::new(),
            "news@example.com",
            UnsubscribeMethod::OneClick {
                url: "https://example.com/one-click".into(),
            },
        );
        app.envelopes = vec![env.clone()];
        app.all_envelopes = vec![env.clone()];
        app.apply(Action::Unsubscribe);
        app.apply(Action::ConfirmUnsubscribeAndArchiveSender);

        let pending = app
            .pending_unsubscribe_action
            .as_ref()
            .expect("unsubscribe action should be queued");
        assert_eq!(pending.message_id, env.id);
        assert_eq!(pending.archive_message_ids.len(), 1);
        assert_eq!(pending.sender_email, "news@example.com");
    }

    #[test]
    fn search_input_lifecycle() {
        let mut bar = SearchBar::default();
        bar.activate();
        assert!(bar.active);
        bar.on_char('h');
        bar.on_char('e');
        bar.on_char('l');
        bar.on_char('l');
        bar.on_char('o');
        assert_eq!(bar.query, "hello");
        let q = bar.submit();
        assert_eq!(q, "hello");
        assert!(!bar.active);
    }

    #[test]
    fn search_bar_cycles_modes() {
        let mut bar = SearchBar::default();
        assert_eq!(bar.mode, mxr_core::SearchMode::Lexical);
        bar.cycle_mode();
        assert_eq!(bar.mode, mxr_core::SearchMode::Hybrid);
        bar.cycle_mode();
        assert_eq!(bar.mode, mxr_core::SearchMode::Semantic);
        bar.cycle_mode();
        assert_eq!(bar.mode, mxr_core::SearchMode::Lexical);
    }

    #[test]
    fn reopening_active_search_preserves_query() {
        let mut app = App::new();
        app.search_active = true;
        app.search_bar.query = "deploy".to_string();
        app.search_bar.cursor_pos = 0;

        app.apply(Action::OpenMailboxFilter);

        assert!(app.search_bar.active);
        assert_eq!(app.search_bar.query, "deploy");
        assert_eq!(app.search_bar.cursor_pos, "deploy".len());
    }

    #[test]
    fn g_prefix_navigation() {
        let mut h = InputHandler::new();
        assert_eq!(
            h.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE)),
            None
        );
        assert_eq!(
            h.handle_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE)),
            Some(Action::GoToInbox)
        );
        assert_eq!(
            h.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE)),
            None
        );
        assert_eq!(
            h.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE)),
            Some(Action::GoToStarred)
        );
    }

    #[test]
    fn status_bar_sync_formats() {
        assert_eq!(
            status_bar::format_sync_status(12, Some("synced 2m ago")),
            "[INBOX] 12 unread | synced 2m ago"
        );
        assert_eq!(
            status_bar::format_sync_status(0, None),
            "[INBOX] 0 unread | not synced"
        );
    }

    fn make_test_labels() -> Vec<Label> {
        crate::test_fixtures::test_system_labels(&AccountId::new())
    }

    // --- Navigation tests ---

    #[test]
    fn threepane_l_loads_new_message() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(5);
        app.all_envelopes = app.envelopes.clone();
        // Open first message
        app.apply(Action::OpenSelected);
        assert_eq!(app.layout_mode, LayoutMode::ThreePane);
        let first_id = app.viewing_envelope.as_ref().unwrap().id.clone();
        // Move focus back to mail list
        app.active_pane = ActivePane::MailList;
        // Navigate to second message
        app.apply(Action::MoveDown);
        // Press l (which triggers OpenSelected)
        app.apply(Action::OpenSelected);
        let second_id = app.viewing_envelope.as_ref().unwrap().id.clone();
        assert_ne!(
            first_id, second_id,
            "l should load the new message, not stay on old one"
        );
        assert_eq!(app.selected_index, 1);
    }

    #[test]
    fn threepane_jk_auto_preview() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(5);
        app.all_envelopes = app.envelopes.clone();
        // Open first message to enter ThreePane
        app.apply(Action::OpenSelected);
        assert_eq!(app.layout_mode, LayoutMode::ThreePane);
        let first_id = app.viewing_envelope.as_ref().unwrap().id.clone();
        // Move focus back to mail list
        app.active_pane = ActivePane::MailList;
        // Move down — should auto-preview
        app.apply(Action::MoveDown);
        let preview_id = app.viewing_envelope.as_ref().unwrap().id.clone();
        assert_ne!(first_id, preview_id, "j/k should auto-preview in ThreePane");
        // Body should be loaded from cache (or None if not cached in test)
        // No async fetch needed — bodies are inline with envelopes
    }

    #[test]
    fn twopane_jk_no_auto_preview() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(5);
        app.all_envelopes = app.envelopes.clone();
        // Don't open message — stay in TwoPane
        assert_eq!(app.layout_mode, LayoutMode::TwoPane);
        app.apply(Action::MoveDown);
        assert!(
            app.viewing_envelope.is_none(),
            "j/k should not auto-preview in TwoPane"
        );
        // No body fetch triggered in TwoPane mode
    }

    // --- Back navigation tests ---

    #[test]
    fn back_in_message_view_closes_preview_pane() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(3);
        app.all_envelopes = app.envelopes.clone();
        app.apply(Action::OpenSelected);
        assert_eq!(app.active_pane, ActivePane::MessageView);
        assert_eq!(app.layout_mode, LayoutMode::ThreePane);
        app.apply(Action::Back);
        assert_eq!(app.active_pane, ActivePane::MailList);
        assert_eq!(app.layout_mode, LayoutMode::TwoPane);
        assert!(app.viewing_envelope.is_none());
    }

    #[test]
    fn back_in_mail_list_clears_label_filter() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(5);
        app.all_envelopes = app.envelopes.clone();
        app.labels = make_test_labels();
        let inbox_id = app
            .labels
            .iter()
            .find(|l| l.name == "INBOX")
            .unwrap()
            .id
            .clone();
        // Simulate label filter active
        app.active_label = Some(inbox_id);
        app.envelopes = vec![app.envelopes[0].clone()]; // Filtered down
                                                        // Esc should clear filter
        app.apply(Action::Back);
        assert!(app.active_label.is_none(), "Esc should clear label filter");
        assert_eq!(app.envelopes.len(), 5, "Should restore all envelopes");
    }

    #[test]
    fn back_in_mail_list_closes_threepane_when_no_filter() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(3);
        app.all_envelopes = app.envelopes.clone();
        app.apply(Action::OpenSelected); // ThreePane
        app.active_pane = ActivePane::MailList; // Move back
                                                // No filter active — Esc should close ThreePane
        app.apply(Action::Back);
        assert_eq!(app.layout_mode, LayoutMode::TwoPane);
    }

    // --- Sidebar tests ---

    #[test]
    fn sidebar_system_labels_before_user_labels() {
        let mut app = App::new();
        app.labels = make_test_labels();
        let ordered = app.ordered_visible_labels();
        // System labels should come first
        let first_user_idx = ordered.iter().position(|l| l.kind == LabelKind::User);
        let last_system_idx = ordered.iter().rposition(|l| l.kind == LabelKind::System);
        if let (Some(first_user), Some(last_system)) = (first_user_idx, last_system_idx) {
            assert!(
                last_system < first_user,
                "All system labels should come before user labels"
            );
        }
    }

    #[test]
    fn sidebar_system_labels_in_correct_order() {
        let mut app = App::new();
        app.labels = make_test_labels();
        let ordered = app.ordered_visible_labels();
        let system_names: Vec<&str> = ordered
            .iter()
            .filter(|l| l.kind == LabelKind::System)
            .map(|l| l.name.as_str())
            .collect();
        // INBOX should be first, then STARRED, SENT, etc.
        assert_eq!(system_names[0], "INBOX");
        assert_eq!(system_names[1], "STARRED");
        assert_eq!(system_names[2], "SENT");
        assert_eq!(system_names[3], "DRAFT");
        assert_eq!(system_names[4], "ARCHIVE");
    }

    #[test]
    fn sidebar_items_put_inbox_before_all_mail() {
        let mut app = App::new();
        app.labels = make_test_labels();

        let items = app.sidebar_items();
        let all_mail_index = items
            .iter()
            .position(|item| matches!(item, SidebarItem::AllMail))
            .unwrap();

        assert!(matches!(
            items.first(),
            Some(SidebarItem::Label(label)) if label.name == "INBOX"
        ));
        assert!(all_mail_index > 0);
    }

    #[test]
    fn sidebar_hidden_labels_not_shown() {
        let mut app = App::new();
        app.labels = make_test_labels();
        let ordered = app.ordered_visible_labels();
        let names: Vec<&str> = ordered.iter().map(|l| l.name.as_str()).collect();
        assert!(
            !names.contains(&"CATEGORY_UPDATES"),
            "Gmail categories should be hidden"
        );
    }

    #[test]
    fn sidebar_empty_system_labels_hidden_except_primary() {
        let mut app = App::new();
        app.labels = make_test_labels();
        let ordered = app.ordered_visible_labels();
        let names: Vec<&str> = ordered.iter().map(|l| l.name.as_str()).collect();
        // CHAT has 0 total, 0 unread — should be hidden
        assert!(
            !names.contains(&"CHAT"),
            "Empty non-primary system labels should be hidden"
        );
        // DRAFT has 0 total but is primary — should be shown
        assert!(
            names.contains(&"DRAFT"),
            "Primary system labels shown even if empty"
        );
        assert!(
            names.contains(&"ARCHIVE"),
            "Archive should be shown as a primary system label even if empty"
        );
        // IMPORTANT has 5 total — should be shown (non-primary but non-empty)
        assert!(
            names.contains(&"IMPORTANT"),
            "Non-empty system labels should be shown"
        );
    }

    #[test]
    fn sidebar_user_labels_alphabetical() {
        let mut app = App::new();
        app.labels = make_test_labels();
        let ordered = app.ordered_visible_labels();
        let user_names: Vec<&str> = ordered
            .iter()
            .filter(|l| l.kind == LabelKind::User)
            .map(|l| l.name.as_str())
            .collect();
        // Personal < Work alphabetically
        assert_eq!(user_names, vec!["Personal", "Work"]);
    }

    // --- GoTo navigation tests ---

    #[test]
    fn goto_inbox_sets_active_label() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(5);
        app.all_envelopes = app.envelopes.clone();
        app.labels = make_test_labels();
        app.apply(Action::GoToInbox);
        let label = app.labels.iter().find(|l| l.name == "INBOX").unwrap();
        assert!(
            app.active_label.is_none(),
            "GoToInbox should wait for fetch success before swapping active label"
        );
        assert_eq!(app.pending_active_label.as_ref().unwrap(), &label.id);
        assert!(
            app.pending_label_fetch.is_some(),
            "Should trigger label fetch"
        );
    }

    #[test]
    fn goto_inbox_without_labels_records_desired_mailbox() {
        let mut app = App::new();
        app.apply(Action::GoToInbox);
        assert_eq!(app.desired_system_mailbox.as_deref(), Some("INBOX"));
        assert!(app.pending_label_fetch.is_none());
        assert!(app.pending_active_label.is_none());
    }

    #[test]
    fn labels_refresh_resolves_desired_inbox() {
        let mut app = App::new();
        app.desired_system_mailbox = Some("INBOX".into());
        app.labels = make_test_labels();

        app.resolve_desired_system_mailbox();

        let inbox_id = app
            .labels
            .iter()
            .find(|label| label.name == "INBOX")
            .unwrap()
            .id
            .clone();
        assert_eq!(app.pending_active_label.as_ref(), Some(&inbox_id));
        assert_eq!(app.pending_label_fetch.as_ref(), Some(&inbox_id));
        assert!(app.active_label.is_none());
    }

    #[test]
    fn sync_completed_requests_live_refresh_even_without_active_label() {
        let mut app = App::new();

        handle_daemon_event(
            &mut app,
            DaemonEvent::SyncCompleted {
                account_id: AccountId::new(),
                messages_synced: 5,
            },
        );

        assert!(app.pending_labels_refresh);
        assert!(app.pending_all_envelopes_refresh);
        assert!(app.pending_status_refresh);
        assert!(app.pending_label_fetch.is_none());
        assert_eq!(app.status_message.as_deref(), Some("Synced 5 messages"));
    }

    #[test]
    fn status_bar_uses_label_counts_instead_of_loaded_window() {
        let mut app = App::new();
        let mut envelopes = make_test_envelopes(5);
        if let Some(first) = envelopes.first_mut() {
            first.flags.remove(MessageFlags::READ);
            first.flags.insert(MessageFlags::STARRED);
        }
        app.envelopes = envelopes.clone();
        app.all_envelopes = envelopes;
        app.labels = make_test_labels();
        let inbox = app
            .labels
            .iter()
            .find(|label| label.name == "INBOX")
            .unwrap()
            .id
            .clone();
        app.active_label = Some(inbox);
        app.last_sync_status = Some("synced just now".into());

        let state = app.status_bar_state();

        assert_eq!(state.mailbox_name, "INBOX");
        assert_eq!(state.total_count, 10);
        assert_eq!(state.unread_count, 3);
        assert_eq!(state.starred_count, 2);
        assert_eq!(state.sync_status.as_deref(), Some("synced just now"));
    }

    #[test]
    fn all_envelopes_refresh_updates_visible_all_mail() {
        let mut app = App::new();
        let envelopes = make_test_envelopes(4);
        app.active_label = None;
        app.search_active = false;

        apply_all_envelopes_refresh(&mut app, envelopes.clone());

        assert_eq!(app.all_envelopes.len(), 4);
        assert_eq!(app.envelopes.len(), 4);
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn all_envelopes_refresh_preserves_selection_when_possible() {
        let mut app = App::new();
        app.visible_height = 3;
        app.mail_list_mode = MailListMode::Messages;
        let initial = make_test_envelopes(4);
        app.all_envelopes = initial.clone();
        app.envelopes = initial.clone();
        app.selected_index = 2;
        app.scroll_offset = 1;

        let mut refreshed = initial.clone();
        refreshed.push(make_test_envelopes(1).remove(0));

        apply_all_envelopes_refresh(&mut app, refreshed);

        assert_eq!(app.selected_index, 2);
        assert_eq!(app.envelopes[app.selected_index].id, initial[2].id);
        assert_eq!(app.scroll_offset, 1);
    }

    #[test]
    fn all_envelopes_refresh_preserves_selected_message_when_rows_shift() {
        let mut app = App::new();
        app.mail_list_mode = MailListMode::Messages;
        let initial = make_test_envelopes(4);
        let selected_id = initial[2].id.clone();
        app.all_envelopes = initial.clone();
        app.envelopes = initial;
        app.selected_index = 2;

        let mut refreshed = make_test_envelopes(1);
        refreshed.extend(app.envelopes.clone());

        apply_all_envelopes_refresh(&mut app, refreshed);

        assert_eq!(app.envelopes[app.selected_index].id, selected_id);
    }

    #[test]
    fn all_envelopes_refresh_preserves_pending_label_view() {
        let mut app = App::new();
        let labels = make_test_labels();
        let inbox_id = labels
            .iter()
            .find(|label| label.name == "INBOX")
            .unwrap()
            .id
            .clone();
        let initial = make_test_envelopes(2);
        let refreshed = make_test_envelopes(5);
        app.labels = labels;
        app.envelopes = initial.clone();
        app.all_envelopes = initial;
        app.pending_active_label = Some(inbox_id);

        apply_all_envelopes_refresh(&mut app, refreshed.clone());

        assert_eq!(app.all_envelopes.len(), refreshed.len());
        assert_eq!(app.all_envelopes[0].id, refreshed[0].id);
        assert_eq!(app.envelopes.len(), 2);
    }

    #[test]
    fn label_counts_refresh_can_follow_empty_boot() {
        let mut app = App::new();
        app.desired_system_mailbox = Some("INBOX".into());

        handle_daemon_event(
            &mut app,
            DaemonEvent::SyncCompleted {
                account_id: AccountId::new(),
                messages_synced: 0,
            },
        );

        assert!(app.pending_labels_refresh);
        assert!(app.pending_all_envelopes_refresh);
        assert_eq!(app.desired_system_mailbox.as_deref(), Some("INBOX"));
    }

    #[test]
    fn clear_filter_restores_all_envelopes() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(10);
        app.all_envelopes = app.envelopes.clone();
        app.labels = make_test_labels();
        let inbox_id = app
            .labels
            .iter()
            .find(|l| l.name == "INBOX")
            .unwrap()
            .id
            .clone();
        app.active_label = Some(inbox_id);
        app.envelopes = vec![app.envelopes[0].clone()]; // Simulate filtered
        app.selected_index = 0;
        app.apply(Action::ClearFilter);
        assert!(app.active_label.is_none());
        assert_eq!(app.envelopes.len(), 10, "Should restore full list");
    }

    // --- Mutation effect tests ---

    #[test]
    fn archive_removes_from_list() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(5);
        app.all_envelopes = app.envelopes.clone();
        let removed_id = app.envelopes[0].id.clone();
        app.apply(Action::Archive);
        assert_eq!(app.pending_mutation_queue.len(), 1);
        assert_eq!(app.envelopes.len(), 4);
        assert!(!app
            .envelopes
            .iter()
            .any(|envelope| envelope.id == removed_id));
    }

    #[test]
    fn star_updates_flags_in_place() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(3);
        app.all_envelopes = app.envelopes.clone();
        // First envelope is READ (even index), not starred
        assert!(!app.envelopes[0].flags.contains(MessageFlags::STARRED));
        app.apply(Action::Star);
        assert_eq!(app.pending_mutation_queue.len(), 1);
        assert_eq!(app.pending_mutation_count, 1);
        assert!(app.envelopes[0].flags.contains(MessageFlags::STARRED));
    }

    #[test]
    fn bulk_mark_read_applies_flags_when_confirmed() {
        let mut app = App::new();
        let mut envelopes = make_test_envelopes(3);
        for envelope in &mut envelopes {
            envelope.flags.remove(MessageFlags::READ);
        }
        app.envelopes = envelopes.clone();
        app.all_envelopes = envelopes.clone();
        app.selected_set = envelopes
            .iter()
            .map(|envelope| envelope.id.clone())
            .collect();

        app.apply(Action::MarkRead);
        assert!(app.pending_mutation_queue.is_empty());
        match app.pending_bulk_confirm.as_ref() {
            Some(confirm) => match &confirm.request {
                Request::Mutation(MutationCommand::SetRead { message_ids, read }) => {
                    assert!(*read);
                    assert_eq!(message_ids.len(), 3);
                }
                other => panic!("Expected SetRead bulk request, got {other:?}"),
            },
            None => panic!("Expected pending bulk confirmation"),
        }
        assert!(app
            .envelopes
            .iter()
            .all(|envelope| !envelope.flags.contains(MessageFlags::READ)));

        app.apply(Action::OpenSelected);

        assert_eq!(app.pending_mutation_queue.len(), 1);
        assert_eq!(app.pending_mutation_count, 1);
        assert!(app.pending_bulk_confirm.is_none());
        assert!(app
            .envelopes
            .iter()
            .all(|envelope| envelope.flags.contains(MessageFlags::READ)));
        assert_eq!(
            app.pending_mutation_status.as_deref(),
            Some("Marking 3 messages as read...")
        );
    }

    #[test]
    fn status_bar_shows_pending_mutation_indicator_after_other_actions() {
        let mut app = App::new();
        let mut envelopes = make_test_envelopes(2);
        for envelope in &mut envelopes {
            envelope.flags.remove(MessageFlags::READ);
        }
        app.envelopes = envelopes.clone();
        app.all_envelopes = envelopes;

        app.apply(Action::MarkRead);
        app.apply(Action::MoveDown);

        let state = app.status_bar_state();
        assert_eq!(state.pending_mutation_count, 1);
        assert_eq!(
            state.pending_mutation_status.as_deref(),
            Some("Marking 1 message as read...")
        );
    }

    #[test]
    fn mark_read_and_archive_removes_message_optimistically_and_queues_mutation() {
        let mut app = App::new();
        let mut envelopes = make_test_envelopes(1);
        envelopes[0].flags.remove(MessageFlags::READ);
        app.envelopes = envelopes.clone();
        app.all_envelopes = envelopes;
        let message_id = app.envelopes[0].id.clone();

        app.apply(Action::MarkReadAndArchive);

        assert!(app.envelopes.is_empty());
        assert!(app.all_envelopes.is_empty());
        assert_eq!(app.pending_mutation_queue.len(), 1);
        match &app.pending_mutation_queue[0].0 {
            Request::Mutation(MutationCommand::ReadAndArchive { message_ids }) => {
                assert_eq!(message_ids, &vec![message_id]);
            }
            other => panic!("expected read-and-archive mutation, got {other:?}"),
        }
    }

    #[test]
    fn bulk_mark_read_and_archive_removes_messages_when_confirmed() {
        let mut app = App::new();
        let mut envelopes = make_test_envelopes(3);
        for envelope in &mut envelopes {
            envelope.flags.remove(MessageFlags::READ);
        }
        app.envelopes = envelopes.clone();
        app.all_envelopes = envelopes.clone();
        app.selected_set = envelopes
            .iter()
            .map(|envelope| envelope.id.clone())
            .collect();

        app.apply(Action::MarkReadAndArchive);
        match app.pending_bulk_confirm.as_ref() {
            Some(confirm) => match &confirm.request {
                Request::Mutation(MutationCommand::ReadAndArchive { message_ids }) => {
                    assert_eq!(message_ids.len(), 3);
                }
                other => panic!("Expected ReadAndArchive bulk request, got {other:?}"),
            },
            None => panic!("Expected pending bulk confirmation"),
        }
        assert_eq!(app.envelopes.len(), 3);

        app.apply(Action::OpenSelected);

        assert!(app.pending_bulk_confirm.is_none());
        assert_eq!(app.pending_mutation_queue.len(), 1);
        assert_eq!(app.pending_mutation_count, 1);
        assert!(app.envelopes.is_empty());
        assert!(app.all_envelopes.is_empty());
        assert_eq!(
            app.pending_mutation_status.as_deref(),
            Some("Marking 3 messages as read and archiving...")
        );
    }

    #[test]
    fn mutation_failure_opens_error_modal_and_refreshes_mailbox() {
        let mut app = App::new();

        app.show_mutation_failure(&MxrError::Ipc("boom".into()));
        app.refresh_mailbox_after_mutation_failure();

        assert_eq!(
            app.error_modal.as_ref().map(|modal| modal.title.as_str()),
            Some("Mutation Failed")
        );
        assert_eq!(
            app.error_modal
                .as_ref()
                .map(|modal| modal.detail.contains("boom")),
            Some(true)
        );
        assert!(app.pending_labels_refresh);
        assert!(app.pending_all_envelopes_refresh);
        assert!(app.pending_status_refresh);
        assert!(app.pending_subscriptions_refresh);
    }

    #[test]
    fn mutation_failure_reloads_pending_label_fetch() {
        let mut app = App::new();
        let inbox_id = LabelId::new();
        app.pending_active_label = Some(inbox_id.clone());

        app.refresh_mailbox_after_mutation_failure();

        assert_eq!(app.pending_label_fetch.as_ref(), Some(&inbox_id));
    }

    #[test]
    fn archive_viewing_message_effect() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(3);
        app.all_envelopes = app.envelopes.clone();
        // Open first message
        app.apply(Action::OpenSelected);
        let viewing_id = app
            .viewing_envelope
            .as_ref()
            .expect("open selected should populate viewing envelope")
            .id
            .clone();
        // The pending_mutation_queue is empty — Archive wasn't pressed yet
        // Press archive while viewing
        app.apply(Action::Archive);
        let (_, effect) = app.pending_mutation_queue.remove(0);
        // Verify the effect targets the viewing envelope
        match &effect {
            MutationEffect::RemoveFromList(id) => {
                assert_eq!(*id, viewing_id);
            }
            _ => panic!("Expected RemoveFromList"),
        }
    }

    #[test]
    fn archive_keeps_reader_open_and_selects_next_message() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(3);
        app.all_envelopes = app.envelopes.clone();

        app.apply(Action::OpenSelected);
        let removed_id = app.viewing_envelope.as_ref().unwrap().id.clone();
        let next_id = app.envelopes[1].id.clone();

        app.apply_removed_message_ids(&[removed_id]);

        assert_eq!(app.layout_mode, LayoutMode::ThreePane);
        assert_eq!(app.selected_index, 0);
        assert_eq!(app.active_pane, ActivePane::MessageView);
        assert_eq!(
            app.viewing_envelope
                .as_ref()
                .map(|envelope| envelope.id.clone()),
            Some(next_id)
        );
    }

    #[test]
    fn archive_keeps_mail_list_focus_when_reader_was_visible() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(3);
        app.all_envelopes = app.envelopes.clone();

        app.apply(Action::OpenSelected);
        app.active_pane = ActivePane::MailList;
        let removed_id = app.viewing_envelope.as_ref().unwrap().id.clone();
        let next_id = app.envelopes[1].id.clone();

        app.apply_removed_message_ids(&[removed_id]);

        assert_eq!(app.layout_mode, LayoutMode::ThreePane);
        assert_eq!(app.active_pane, ActivePane::MailList);
        assert_eq!(
            app.viewing_envelope
                .as_ref()
                .map(|envelope| envelope.id.clone()),
            Some(next_id)
        );
    }

    #[test]
    fn archive_last_visible_message_closes_reader() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(1);
        app.all_envelopes = app.envelopes.clone();

        app.apply(Action::OpenSelected);
        let removed_id = app.viewing_envelope.as_ref().unwrap().id.clone();

        app.apply_removed_message_ids(&[removed_id]);

        assert_eq!(app.layout_mode, LayoutMode::TwoPane);
        assert_eq!(app.active_pane, ActivePane::MailList);
        assert!(app.viewing_envelope.is_none());
        assert!(app.envelopes.is_empty());
    }

    // --- Mail list title tests ---

    #[test]
    fn mail_list_title_shows_message_count() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(5);
        app.all_envelopes = app.envelopes.clone();
        let title = app.mail_list_title();
        assert!(title.contains("5"), "Title should show message count");
        assert!(
            title.contains("Threads"),
            "Default title should say Threads"
        );
    }

    #[test]
    fn mail_list_title_shows_label_name() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(5);
        app.all_envelopes = app.envelopes.clone();
        app.labels = make_test_labels();
        let inbox_id = app
            .labels
            .iter()
            .find(|l| l.name == "INBOX")
            .unwrap()
            .id
            .clone();
        app.active_label = Some(inbox_id);
        let title = app.mail_list_title();
        assert!(
            title.contains("Inbox"),
            "Title should show humanized label name"
        );
    }

    #[test]
    fn mail_list_title_shows_search_query() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(5);
        app.all_envelopes = app.envelopes.clone();
        app.search_active = true;
        app.search_bar.query = "deployment".to_string();
        let title = app.mail_list_title();
        assert!(
            title.contains("deployment"),
            "Title should show search query"
        );
        assert!(title.contains("Search"), "Title should indicate search");
    }

    #[test]
    fn message_view_body_display() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(3);
        app.all_envelopes = app.envelopes.clone();
        app.apply(Action::OpenMessageView);
        assert_eq!(app.layout_mode, LayoutMode::ThreePane);
        app.body_view_state = BodyViewState::Ready {
            raw: "Hello".into(),
            rendered: "Hello".into(),
            source: BodySource::Plain,
            metadata: BodyViewMetadata::default(),
        };
        assert_eq!(app.body_view_state.display_text(), Some("Hello"));
        app.apply(Action::CloseMessageView);
        assert!(matches!(app.body_view_state, BodyViewState::Empty { .. }));
    }

    #[test]
    fn close_message_view_preserves_reader_mode() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(1);
        app.all_envelopes = app.envelopes.clone();
        app.apply(Action::OpenMessageView);

        app.apply(Action::CloseMessageView);

        assert!(app.reader_mode);
    }

    #[test]
    fn open_selected_populates_visible_thread_messages() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(3);
        let shared_thread = ThreadId::new();
        app.envelopes[0].thread_id = shared_thread.clone();
        app.envelopes[1].thread_id = shared_thread;
        app.envelopes[0].date = chrono::Utc::now() - chrono::Duration::minutes(5);
        app.envelopes[1].date = chrono::Utc::now();
        app.all_envelopes = app.envelopes.clone();

        app.apply(Action::OpenSelected);

        assert_eq!(app.viewed_thread_messages.len(), 2);
        assert_eq!(app.viewed_thread_messages[0].id, app.envelopes[0].id);
        assert_eq!(app.viewed_thread_messages[1].id, app.envelopes[1].id);
    }

    #[test]
    fn mail_list_defaults_to_threads() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(3);
        let shared_thread = ThreadId::new();
        app.envelopes[0].thread_id = shared_thread.clone();
        app.envelopes[1].thread_id = shared_thread;
        app.all_envelopes = app.envelopes.clone();

        assert_eq!(app.mail_list_rows().len(), 2);
        assert_eq!(
            app.selected_mail_row().map(|row| row.message_count),
            Some(2)
        );
    }

    #[test]
    fn open_thread_focuses_latest_unread_message() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(3);
        let shared_thread = ThreadId::new();
        app.envelopes[0].thread_id = shared_thread.clone();
        app.envelopes[1].thread_id = shared_thread;
        app.envelopes[0].date = chrono::Utc::now() - chrono::Duration::minutes(10);
        app.envelopes[1].date = chrono::Utc::now();
        app.envelopes[0].flags = MessageFlags::READ;
        app.envelopes[1].flags = MessageFlags::empty();
        app.all_envelopes = app.envelopes.clone();

        app.apply(Action::OpenSelected);

        assert_eq!(app.thread_selected_index, 1);
        assert_eq!(
            app.focused_thread_envelope().map(|env| env.id.clone()),
            Some(app.envelopes[1].id.clone())
        );
    }

    #[test]
    fn open_selected_marks_unread_message_read_after_dwell() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(1);
        app.envelopes[0].flags = MessageFlags::empty();
        app.all_envelopes = app.envelopes.clone();

        app.apply(Action::OpenSelected);

        assert!(!app.envelopes[0].flags.contains(MessageFlags::READ));
        assert!(!app.all_envelopes[0].flags.contains(MessageFlags::READ));
        assert!(!app.viewed_thread_messages[0]
            .flags
            .contains(MessageFlags::READ));
        assert!(!app
            .viewing_envelope
            .as_ref()
            .unwrap()
            .flags
            .contains(MessageFlags::READ));
        assert!(app.pending_mutation_queue.is_empty());

        app.expire_pending_preview_read_for_tests();
        app.tick();

        assert!(app.envelopes[0].flags.contains(MessageFlags::READ));
        assert!(app.all_envelopes[0].flags.contains(MessageFlags::READ));
        assert!(app.viewed_thread_messages[0]
            .flags
            .contains(MessageFlags::READ));
        assert!(app
            .viewing_envelope
            .as_ref()
            .unwrap()
            .flags
            .contains(MessageFlags::READ));
        assert_eq!(app.pending_mutation_queue.len(), 1);
        match &app.pending_mutation_queue[0].0 {
            Request::Mutation(MutationCommand::SetRead { message_ids, read }) => {
                assert!(*read);
                assert_eq!(message_ids, &vec![app.envelopes[0].id.clone()]);
            }
            other => panic!("expected set-read mutation, got {other:?}"),
        }
    }

    #[test]
    fn open_selected_on_read_message_does_not_queue_read_mutation() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(1);
        app.envelopes[0].flags = MessageFlags::READ;
        app.all_envelopes = app.envelopes.clone();

        app.apply(Action::OpenSelected);
        app.expire_pending_preview_read_for_tests();
        app.tick();

        assert!(app.pending_mutation_queue.is_empty());
    }

    #[test]
    fn reopening_same_message_does_not_queue_duplicate_read_mutation() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(1);
        app.envelopes[0].flags = MessageFlags::empty();
        app.all_envelopes = app.envelopes.clone();

        app.apply(Action::OpenSelected);
        app.apply(Action::OpenSelected);

        assert!(app.pending_mutation_queue.is_empty());
        app.expire_pending_preview_read_for_tests();
        app.tick();
        assert_eq!(app.pending_mutation_queue.len(), 1);
    }

    #[test]
    fn single_message_view_uses_jk_to_scroll() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(1);
        app.all_envelopes = app.envelopes.clone();

        app.apply(Action::OpenSelected);

        assert_eq!(app.active_pane, ActivePane::MessageView);
        assert_eq!(app.viewed_thread_messages.len(), 1);
        assert_eq!(app.thread_selected_index, 0);
        assert_eq!(app.message_scroll_offset, 0);

        let _ = app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(app.thread_selected_index, 0);
        assert_eq!(app.message_scroll_offset, 1);

        let _ = app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.thread_selected_index, 0);
        assert_eq!(app.message_scroll_offset, 2);

        let _ = app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
        assert_eq!(app.thread_selected_index, 0);
        assert_eq!(app.message_scroll_offset, 1);

        let _ = app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(app.thread_selected_index, 0);
        assert_eq!(app.message_scroll_offset, 0);
    }

    #[test]
    fn thread_move_down_changes_reply_target() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(2);
        let shared_thread = ThreadId::new();
        app.envelopes[0].thread_id = shared_thread.clone();
        app.envelopes[1].thread_id = shared_thread;
        app.envelopes[0].date = chrono::Utc::now() - chrono::Duration::minutes(5);
        app.envelopes[1].date = chrono::Utc::now();
        app.envelopes[0].flags = MessageFlags::empty();
        app.envelopes[1].flags = MessageFlags::READ;
        app.all_envelopes = app.envelopes.clone();

        app.apply(Action::OpenSelected);
        assert_eq!(
            app.focused_thread_envelope().map(|env| env.id.clone()),
            Some(app.envelopes[0].id.clone())
        );

        let _ = app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));

        assert_eq!(
            app.focused_thread_envelope().map(|env| env.id.clone()),
            Some(app.envelopes[1].id.clone())
        );
        app.apply(Action::Reply);
        assert_eq!(
            app.pending_compose,
            Some(super::app::ComposeAction::Reply {
                message_id: app.envelopes[1].id.clone()
            })
        );
    }

    #[test]
    fn thread_focus_change_marks_newly_focused_unread_message_read_after_dwell() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(2);
        let shared_thread = ThreadId::new();
        app.envelopes[0].thread_id = shared_thread.clone();
        app.envelopes[1].thread_id = shared_thread;
        app.envelopes[0].date = chrono::Utc::now() - chrono::Duration::minutes(5);
        app.envelopes[1].date = chrono::Utc::now();
        app.envelopes[0].flags = MessageFlags::empty();
        app.envelopes[1].flags = MessageFlags::empty();
        app.all_envelopes = app.envelopes.clone();

        app.apply(Action::OpenSelected);
        assert_eq!(app.thread_selected_index, 1);
        assert!(app.pending_mutation_queue.is_empty());

        let _ = app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));

        assert_eq!(app.thread_selected_index, 0);
        assert!(!app.viewed_thread_messages[0]
            .flags
            .contains(MessageFlags::READ));
        assert!(app.pending_mutation_queue.is_empty());

        app.expire_pending_preview_read_for_tests();
        app.tick();

        assert!(app.viewed_thread_messages[0]
            .flags
            .contains(MessageFlags::READ));
        assert!(app
            .viewing_envelope
            .as_ref()
            .unwrap()
            .flags
            .contains(MessageFlags::READ));
        assert_eq!(app.pending_mutation_queue.len(), 1);
        match &app.pending_mutation_queue[0].0 {
            Request::Mutation(MutationCommand::SetRead { message_ids, read }) => {
                assert!(*read);
                assert_eq!(message_ids, &vec![app.envelopes[0].id.clone()]);
            }
            other => panic!("expected set-read mutation, got {other:?}"),
        }
    }

    #[test]
    fn preview_navigation_only_marks_message_read_after_settling() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(2);
        app.envelopes[0].flags = MessageFlags::empty();
        app.envelopes[1].flags = MessageFlags::empty();
        app.envelopes[0].thread_id = ThreadId::new();
        app.envelopes[1].thread_id = ThreadId::new();
        app.envelopes[0].date = chrono::Utc::now() - chrono::Duration::minutes(1);
        app.envelopes[1].date = chrono::Utc::now();
        app.all_envelopes = app.envelopes.clone();

        app.apply(Action::OpenSelected);
        app.apply(Action::MoveDown);

        assert!(!app.envelopes[0].flags.contains(MessageFlags::READ));
        assert!(!app.envelopes[1].flags.contains(MessageFlags::READ));
        assert!(app.pending_mutation_queue.is_empty());

        app.expire_pending_preview_read_for_tests();
        app.tick();

        assert!(!app.envelopes[0].flags.contains(MessageFlags::READ));
        assert!(app.envelopes[1].flags.contains(MessageFlags::READ));
        assert_eq!(app.pending_mutation_queue.len(), 1);
        match &app.pending_mutation_queue[0].0 {
            Request::Mutation(MutationCommand::SetRead { message_ids, read }) => {
                assert!(*read);
                assert_eq!(message_ids, &vec![app.envelopes[1].id.clone()]);
            }
            other => panic!("expected set-read mutation, got {other:?}"),
        }
    }

    #[test]
    fn help_action_toggles_modal_state() {
        let mut app = App::new();

        app.apply(Action::Help);
        assert!(app.help_modal_open);
        assert!(app.help_query.is_empty());
        assert_eq!(app.help_selected, 0);

        app.help_query = "config".into();
        app.help_selected = 3;
        app.apply(Action::Help);
        assert!(!app.help_modal_open);
        assert!(app.help_query.is_empty());
        assert_eq!(app.help_selected, 0);
    }

    #[test]
    fn help_modal_typing_enters_search_mode_and_backspace_clears_it() {
        let mut app = App::new();
        app.apply(Action::Help);

        let action = app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
        assert!(action.is_none());
        assert_eq!(app.help_query, "g");
        assert_eq!(app.help_selected, 0);

        let action = app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
        assert!(action.is_none());
        assert_eq!(app.help_query, "gc");

        let action = app.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert!(action.is_none());
        assert_eq!(app.help_query, "g");

        let action = app.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert!(action.is_none());
        assert!(app.help_query.is_empty());
        assert_eq!(app.help_selected, 0);
    }

    #[test]
    fn help_modal_o_types_instead_of_reopening_onboarding() {
        let mut app = App::new();
        app.apply(Action::Help);

        let action = app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
        assert!(action.is_none());
        assert_eq!(app.help_query, "o");
        assert!(!app.onboarding.visible);
    }

    #[test]
    fn account_form_validation_points_to_first_invalid_field() {
        let mut app = App::new();
        app.screen = Screen::Accounts;
        app.accounts_page.form.visible = true;
        app.accounts_page.form.mode = super::app::AccountFormMode::ImapSmtp;
        app.accounts_page.form.key = "work".into();
        app.accounts_page.form.email = "me@example.com".into();
        app.accounts_page.form.imap_port = "993".into();
        app.accounts_page.form.smtp_host = "smtp.example.com".into();
        app.accounts_page.form.smtp_port = "587".into();
        app.accounts_page.form.smtp_auth_required = false;

        app.apply(Action::TestAccountForm);

        assert_eq!(app.accounts_page.form.active_field, 4);
        assert!(!app.accounts_page.operation_in_flight);
        assert!(app.pending_account_test.is_none());
        let result = app.accounts_page.form.last_result.as_ref().unwrap();
        assert!(result.summary.contains("Account form has problems."));
        assert_eq!(
            result.sync.as_ref().unwrap().detail,
            "IMAP host is required. IMAP auth is enabled, so IMAP password or IMAP pass ref is required."
        );
    }

    #[test]
    fn smtp_only_form_test_allows_no_auth_and_marks_operation_pending() {
        let mut app = App::new();
        app.screen = Screen::Accounts;
        app.accounts_page.form.visible = true;
        app.accounts_page.form.mode = super::app::AccountFormMode::SmtpOnly;
        app.accounts_page.form.key = "relay".into();
        app.accounts_page.form.email = "relay@example.com".into();
        app.accounts_page.form.smtp_host = "smtp.example.com".into();
        app.accounts_page.form.smtp_port = "25".into();
        app.accounts_page.form.smtp_auth_required = false;
        app.accounts_page.form.last_result = Some(mxr_protocol::AccountOperationResult {
            ok: false,
            summary: "stale".into(),
            save: None,
            auth: None,
            sync: None,
            send: None,
        });

        app.apply(Action::TestAccountForm);

        assert!(app.accounts_page.operation_in_flight);
        assert!(app.accounts_page.form.last_result.is_none());
        let pending = app.pending_account_test.take().unwrap();
        match pending.send.unwrap() {
            mxr_protocol::AccountSendConfigData::Smtp {
                auth_required,
                username,
                password_ref,
                ..
            } => {
                assert!(!auth_required);
                assert!(username.is_empty());
                assert!(password_ref.is_empty());
            }
            other => panic!("expected smtp config, got {other:?}"),
        }
    }

    #[test]
    fn auth_required_form_generates_secret_refs_from_account_key() {
        let mut app = App::new();
        app.screen = Screen::Accounts;
        app.accounts_page.form.visible = true;
        app.accounts_page.form.is_new_account = true;
        app.accounts_page.form.mode = super::app::AccountFormMode::ImapSmtp;
        app.accounts_page.form.key = "work".into();
        app.accounts_page.form.email = "me@example.com".into();
        app.accounts_page.form.imap_host = "imap.example.com".into();
        app.accounts_page.form.imap_port = "993".into();
        app.accounts_page.form.imap_password = "imap-secret".into();
        app.accounts_page.form.smtp_host = "smtp.example.com".into();
        app.accounts_page.form.smtp_port = "587".into();
        app.accounts_page.form.smtp_password = "smtp-secret".into();

        app.apply(Action::TestAccountForm);

        let pending = app.pending_account_test.take().unwrap();
        match pending.sync.unwrap() {
            mxr_protocol::AccountSyncConfigData::Imap { password_ref, .. } => {
                assert_eq!(password_ref, "mxr/work-imap");
            }
            other => panic!("expected imap config, got {other:?}"),
        }
        match pending.send.unwrap() {
            mxr_protocol::AccountSendConfigData::Smtp { password_ref, .. } => {
                assert_eq!(password_ref, "mxr/work-smtp");
            }
            other => panic!("expected smtp config, got {other:?}"),
        }
    }

    #[test]
    fn failed_account_operation_opens_details_modal() {
        let mut app = App::new();
        let result = mxr_protocol::AccountOperationResult {
            ok: false,
            summary: "Account 'consulting' test failed.".into(),
            save: None,
            auth: None,
            sync: Some(mxr_protocol::AccountOperationStep {
                ok: false,
                detail: "IMAP server returned a NAMESPACE response in an unsupported format during folder discovery. This looks like a server compatibility issue, not necessarily a bad username or password.".into(),
            }),
            send: Some(mxr_protocol::AccountOperationStep {
                ok: true,
                detail: "SMTP send ok".into(),
            }),
        };

        app.apply_account_operation_result(result);

        let modal = app.error_modal.as_ref().unwrap();
        assert_eq!(modal.title, "Account Test Failed");
        assert!(modal.detail.contains("NAMESPACE response"));
        assert!(modal.detail.contains("compatibility issue"));
    }

    #[test]
    fn account_form_o_reopens_result_details_modal() {
        let mut app = App::new();
        app.screen = Screen::Accounts;
        app.accounts_page.form.visible = true;
        app.accounts_page.form.last_result = Some(mxr_protocol::AccountOperationResult {
            ok: false,
            summary: "Account 'consulting' test failed.".into(),
            save: None,
            auth: None,
            sync: Some(mxr_protocol::AccountOperationStep {
                ok: false,
                detail: "IMAP server returned a response mxr could not parse.".into(),
            }),
            send: None,
        });

        let action = app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));

        assert!(action.is_none());
        assert_eq!(
            app.error_modal.as_ref().map(|modal| modal.title.as_str()),
            Some("Account Test Failed")
        );
    }

    #[test]
    fn error_modal_supports_scrolling_keys() {
        let mut app = App::new();
        app.error_modal = Some(super::app::ErrorModalState::new(
            "Account Test Failed",
            "line1\nline2\nline3\nline4\nline5",
        ));

        let action = app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert!(action.is_none());
        assert_eq!(app.error_modal.as_ref().unwrap().scroll_offset, 1);

        let action = app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
        assert!(action.is_none());
        assert_eq!(app.error_modal.as_ref().unwrap().scroll_offset, 9);

        let action = app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
        assert!(action.is_none());
        assert_eq!(app.error_modal.as_ref().unwrap().scroll_offset, 8);

        let action = app.handle_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE));
        assert!(action.is_none());
        assert_eq!(app.error_modal.as_ref().unwrap().scroll_offset, 0);
    }

    #[test]
    fn closing_new_account_form_preserves_draft_and_resume_restores_it() {
        let mut app = App::new();
        app.screen = Screen::Accounts;
        app.accounts_page.form.visible = true;
        app.accounts_page.form.is_new_account = true;
        app.accounts_page.form.key = "draft".into();
        app.accounts_page.form.email = "draft@example.com".into();
        app.accounts_page.form.smtp_host = "smtp.example.com".into();

        let action = app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(action.is_none());
        assert!(!app.accounts_page.form.visible);
        assert_eq!(
            app.accounts_page.new_account_draft.as_ref().unwrap().key,
            "draft"
        );

        app.apply(Action::OpenAccountFormNew);
        assert!(app.accounts_page.resume_new_account_draft_prompt_open);
        assert!(!app.accounts_page.form.visible);

        let action = app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(action.is_none());
        assert!(app.accounts_page.form.visible);
        assert_eq!(app.accounts_page.form.key, "draft");
        assert_eq!(app.accounts_page.form.email, "draft@example.com");
        assert!(app.accounts_page.new_account_draft.is_none());
    }

    #[test]
    fn new_account_draft_prompt_can_start_fresh_form() {
        let mut app = App::new();
        app.screen = Screen::Accounts;
        app.accounts_page.form.visible = true;
        app.accounts_page.form.is_new_account = true;
        app.accounts_page.form.key = "draft".into();
        app.accounts_page.form.email = "draft@example.com".into();

        let action = app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(action.is_none());
        assert_eq!(
            app.accounts_page
                .new_account_draft
                .as_ref()
                .map(|draft| draft.email.as_str()),
            Some("draft@example.com")
        );

        app.apply(Action::OpenAccountFormNew);
        assert!(app.accounts_page.resume_new_account_draft_prompt_open);

        let action = app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        assert!(action.is_none());
        assert!(app.accounts_page.form.visible);
        assert!(app.accounts_page.form.is_new_account);
        assert!(app.accounts_page.form.key.is_empty());
        assert!(app.accounts_page.new_account_draft.is_none());
        assert!(!app.accounts_page.resume_new_account_draft_prompt_open);
    }

    #[test]
    fn leaving_accounts_screen_preserves_new_account_draft() {
        let mut app = App::new();
        app.screen = Screen::Accounts;
        app.accounts_page.form.visible = true;
        app.accounts_page.form.is_new_account = true;
        app.accounts_page.form.key = "draft".into();
        app.accounts_page.form.email = "draft@example.com".into();

        app.apply(Action::OpenMailboxScreen);

        assert_eq!(app.screen, Screen::Mailbox);
        assert!(!app.accounts_page.form.visible);
        assert_eq!(
            app.accounts_page.new_account_draft.as_ref().unwrap().email,
            "draft@example.com"
        );
    }

    #[test]
    fn open_search_screen_activates_dedicated_search_workspace() {
        let mut app = App::new();
        app.apply(Action::OpenSearchScreen);
        assert_eq!(app.screen, Screen::Search);
        assert!(app.search_page.editing);
    }

    #[test]
    fn search_screen_typing_updates_results_and_queues_search() {
        let mut app = App::new();
        let mut envelopes = make_test_envelopes(2);
        envelopes[0].subject = "crates.io release".into();
        envelopes[0].snippet = "mxr publish".into();
        envelopes[1].subject = "support request".into();
        envelopes[1].snippet = "billing".into();
        app.envelopes = envelopes.clone();
        app.all_envelopes = envelopes;

        app.apply(Action::OpenSearchScreen);
        app.search_page.query.clear();
        app.search_page.results = app.all_envelopes.clone();

        for ch in "crate".chars() {
            let action = app.handle_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
            assert!(action.is_none());
        }

        assert_eq!(app.search_page.query, "crate");
        assert!(app.search_page.results.is_empty());
        assert!(!app.search_page.loading_more);
        assert!(!app.search_page.count_pending);
        assert_eq!(
            app.search_page.ui_status,
            crate::app::SearchUiStatus::Debouncing
        );
        assert_eq!(
            app.pending_search_debounce,
            Some(crate::app::PendingSearchDebounce {
                query: "crate".into(),
                mode: mxr_core::SearchMode::Lexical,
                session_id: app.search_page.session_id,
                due_at: app
                    .pending_search_debounce
                    .as_ref()
                    .map(|pending| pending.due_at)
                    .expect("debounce timer should be set"),
            })
        );
        assert!(app.pending_search.is_none());
        assert!(app.pending_search_count.is_none());
    }

    #[test]
    fn open_search_screen_preserves_existing_search_session() {
        let mut app = App::new();
        let results = make_test_envelopes(2);
        app.search_bar.query = "stale overlay".into();
        app.search_page.query = "deploy".into();
        app.search_page.results = results.clone();
        app.search_page.session_active = true;
        app.search_page.selected_index = 1;
        app.search_page.result_selected = true;
        app.search_page.active_pane = SearchPane::Preview;
        app.viewing_envelope = Some(results[1].clone());

        app.apply(Action::OpenRulesScreen);
        app.apply(Action::OpenSearchScreen);

        assert_eq!(app.screen, Screen::Search);
        assert_eq!(app.search_page.query, "deploy");
        assert_eq!(app.search_page.results.len(), 2);
        assert_eq!(app.search_page.selected_index, 1);
        assert_eq!(app.search_page.active_pane, SearchPane::Preview);
        assert_eq!(
            app.viewing_envelope.as_ref().map(|env| env.id.clone()),
            Some(results[1].id.clone())
        );
        assert!(app.pending_search.is_none());
    }

    #[test]
    fn slash_opens_global_search_and_ctrl_f_opens_mailbox_filter() {
        let mut app = App::new();

        let action = app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        assert_eq!(action, Some(Action::OpenGlobalSearch));
        app.apply(action.expect("slash should map to search"));
        assert_eq!(app.screen, Screen::Search);
        assert!(app.search_page.editing);

        app.apply(Action::OpenMailboxScreen);
        let action = app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL));
        assert_eq!(action, Some(Action::OpenMailboxFilter));
    }

    #[test]
    fn search_results_accept_gg_and_g_navigation() {
        let mut app = App::new();
        app.apply(Action::OpenSearchScreen);
        app.search_page.editing = false;
        app.search_page.results = make_test_envelopes(3);
        app.search_page.selected_index = 2;

        let action = app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
        assert!(action.is_none());
        let action = app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
        assert_eq!(action, Some(Action::JumpTop));
        app.apply(action.unwrap());
        assert_eq!(app.search_page.selected_index, 0);

        let action = app.handle_key(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT));
        assert_eq!(action, Some(Action::JumpBottom));
        app.apply(action.unwrap());
        assert_eq!(app.search_page.selected_index, 2);
    }

    #[test]
    fn open_search_screen_without_session_clears_stale_preview_and_query() {
        let mut app = App::new();
        let envelope = make_test_envelopes(1).remove(0);
        app.search_bar.query = "mailbox quick filter".into();
        app.viewing_envelope = Some(envelope.clone());
        app.viewed_thread_messages = vec![envelope];
        app.search_page.query = "stale".into();
        app.search_page.session_active = false;
        app.search_page.results.clear();

        app.apply(Action::OpenSearchScreen);

        assert_eq!(app.screen, Screen::Search);
        assert!(app.search_page.editing);
        assert!(app.search_page.query.is_empty());
        assert!(app.viewing_envelope.is_none());
        assert!(app.viewed_thread_messages.is_empty());
        assert_eq!(app.search_page.ui_status, crate::app::SearchUiStatus::Idle);
    }

    #[test]
    fn non_mail_screens_ignore_label_shortcut() {
        let mut app = App::new();

        for screen in [Screen::Rules, Screen::Accounts, Screen::Diagnostics] {
            app.screen = screen;
            app.label_picker.close();
            let action = app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
            assert!(action.is_none(), "unexpected action on {screen:?}");
            assert!(
                !app.label_picker.visible,
                "label picker opened on {screen:?}"
            );
        }
    }

    #[test]
    fn rules_navigation_refreshes_selected_panel_request() {
        let mut app = App::new();
        app.screen = Screen::Rules;
        app.rules_page.rules = vec![
            serde_json::json!({"id": "rule-1", "name": "One"}),
            serde_json::json!({"id": "rule-2", "name": "Two"}),
        ];
        app.rules_page.panel = crate::app::RulesPanel::History;

        assert!(app
            .handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
            .is_none());
        assert_eq!(app.rules_page.selected_index, 1);
        assert_eq!(app.pending_rule_history.as_deref(), Some("rule-2"));

        app.rules_page.panel = crate::app::RulesPanel::DryRun;
        assert!(app
            .handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE))
            .is_none());
        assert_eq!(app.rules_page.selected_index, 0);
        assert_eq!(app.pending_rule_dry_run.as_deref(), Some("rule-1"));
    }

    #[test]
    fn search_open_selected_keeps_search_screen_and_focuses_preview() {
        let mut app = App::new();
        let results = make_test_envelopes(2);
        app.screen = Screen::Search;
        app.search_page.query = "deploy".into();
        app.search_page.results = results.clone();
        app.search_page.session_active = true;
        app.search_page.selected_index = 1;

        app.apply(Action::OpenSelected);

        assert_eq!(app.screen, Screen::Search);
        assert_eq!(app.search_page.active_pane, SearchPane::Preview);
        assert_eq!(
            app.viewing_envelope.as_ref().map(|env| env.id.clone()),
            Some(results[1].id.clone())
        );
    }

    #[test]
    fn search_results_do_not_collapse_to_threads() {
        let mut app = App::new();
        let mut results = make_test_envelopes(3);
        let thread_id = ThreadId::new();
        for envelope in &mut results {
            envelope.thread_id = thread_id.clone();
        }
        app.mail_list_mode = MailListMode::Threads;
        app.screen = Screen::Search;
        app.search_page.query = "deploy".into();
        app.search_page.results = results.clone();
        app.search_page.session_active = true;
        app.search_page.selected_index = 1;

        assert_eq!(app.search_row_count(), 3);
        assert_eq!(
            app.selected_search_envelope().map(|env| env.id.clone()),
            Some(results[1].id.clone())
        );
    }

    #[test]
    fn search_jump_bottom_loads_remaining_pages() {
        let mut app = App::new();
        app.screen = Screen::Search;
        app.search_page.query = "deploy".into();
        app.search_page.results = make_test_envelopes(3);
        app.search_page.session_active = true;
        app.search_page.has_more = true;
        app.search_page.loading_more = false;
        app.search_page.session_id = 9;

        app.apply(Action::JumpBottom);

        assert!(app.search_page.load_to_end);
        assert!(app.search_page.loading_more);
        assert_eq!(
            app.pending_search,
            Some(PendingSearchRequest {
                query: "deploy".into(),
                mode: mxr_core::SearchMode::Lexical,
                sort: mxr_core::SortOrder::DateDesc,
                limit: SEARCH_PAGE_SIZE,
                offset: 3,
                target: SearchTarget::SearchPage,
                append: true,
                session_id: 9,
            })
        );
    }

    #[test]
    fn search_jump_bottom_uses_search_results_viewport_height() {
        let mut app = App::new();
        app.screen = Screen::Search;
        app.search_page.query = "deploy".into();
        app.search_page.results = make_test_envelopes(15);
        app.search_page.session_active = true;

        let _ = render_to_string(120, 20, |frame| app.draw(frame));

        app.apply(Action::JumpBottom);

        assert_eq!(app.visible_height, 10);
        assert_eq!(app.search_page.selected_index, 14);
        assert_eq!(app.search_page.scroll_offset, 5);
    }

    #[test]
    fn search_escape_routes_back_to_inbox() {
        let mut app = App::new();
        app.screen = Screen::Search;
        app.search_page.session_active = true;
        app.search_page.query = "deploy".into();
        app.search_page.results = make_test_envelopes(2);
        app.search_page.active_pane = SearchPane::Results;

        let action = app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

        assert_eq!(action, Some(Action::OpenMailboxScreen));
    }

    #[test]
    fn open_rules_screen_marks_refresh_pending() {
        let mut app = App::new();
        app.apply(Action::OpenRulesScreen);
        assert_eq!(app.screen, Screen::Rules);
        assert!(app.rules_page.refresh_pending);
    }

    #[test]
    fn open_diagnostics_screen_marks_refresh_pending() {
        let mut app = App::new();
        app.apply(Action::OpenDiagnosticsScreen);
        assert_eq!(app.screen, Screen::Diagnostics);
        assert!(app.diagnostics_page.refresh_pending);
    }

    #[test]
    fn open_accounts_screen_marks_refresh_pending() {
        let mut app = App::new();
        app.apply(Action::OpenAccountsScreen);
        assert_eq!(app.screen, Screen::Accounts);
        assert!(app.accounts_page.refresh_pending);
    }

    #[test]
    fn new_account_form_opens_from_accounts_screen() {
        let mut app = App::new();
        app.apply(Action::OpenAccountsScreen);
        app.apply(Action::OpenAccountFormNew);

        assert_eq!(app.screen, Screen::Accounts);
        assert!(app.accounts_page.form.visible);
        assert_eq!(
            app.accounts_page.form.mode,
            crate::app::AccountFormMode::Gmail
        );
    }

    #[test]
    fn app_from_empty_config_enters_account_onboarding() {
        let config = mxr_config::MxrConfig::default();
        let app = App::from_config(&config);

        // Onboarding modal shows on whatever page the user is on (mailbox by default)
        assert_eq!(app.screen, Screen::Mailbox);
        assert!(app.accounts_page.onboarding_required);
        assert!(app.accounts_page.onboarding_modal_open);
    }

    #[test]
    fn onboarding_confirm_opens_new_account_form() {
        let config = mxr_config::MxrConfig::default();
        let mut app = App::from_config(&config);

        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert_eq!(app.screen, Screen::Accounts);
        assert!(app.accounts_page.form.visible);
        assert!(!app.accounts_page.onboarding_modal_open);
    }

    #[test]
    fn onboarding_q_quits() {
        let config = mxr_config::MxrConfig::default();
        let mut app = App::from_config(&config);

        let action = app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));

        assert_eq!(action, Some(Action::QuitView));
    }

    #[test]
    fn onboarding_blocks_mailbox_screen_until_account_exists() {
        let config = mxr_config::MxrConfig::default();
        let mut app = App::from_config(&config);

        app.apply(Action::OpenMailboxScreen);

        assert_eq!(app.screen, Screen::Accounts);
        assert!(app.accounts_page.onboarding_required);
    }

    #[test]
    fn account_form_h_and_l_switch_modes_from_any_field() {
        let mut app = App::new();
        app.apply(Action::OpenAccountFormNew);
        app.accounts_page.form.active_field = 2;

        app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
        assert_eq!(
            app.accounts_page.form.mode,
            crate::app::AccountFormMode::ImapSmtp
        );

        app.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
        assert_eq!(
            app.accounts_page.form.mode,
            crate::app::AccountFormMode::Gmail
        );
    }

    #[test]
    fn account_form_tab_on_mode_cycles_modes() {
        let mut app = App::new();
        app.apply(Action::OpenAccountFormNew);
        app.accounts_page.form.active_field = 0;

        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(
            app.accounts_page.form.mode,
            crate::app::AccountFormMode::ImapSmtp
        );

        app.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
        assert_eq!(
            app.accounts_page.form.mode,
            crate::app::AccountFormMode::Gmail
        );
    }

    #[test]
    fn account_form_mode_switch_with_input_requires_confirmation() {
        let mut app = App::new();
        app.apply(Action::OpenAccountFormNew);
        app.accounts_page.form.key = "work".into();

        app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));

        assert_eq!(
            app.accounts_page.form.mode,
            crate::app::AccountFormMode::Gmail
        );
        assert_eq!(
            app.accounts_page.form.pending_mode_switch,
            Some(crate::app::AccountFormMode::ImapSmtp)
        );
    }

    #[test]
    fn account_form_mode_switch_confirmation_applies_mode_change() {
        let mut app = App::new();
        app.apply(Action::OpenAccountFormNew);
        app.accounts_page.form.key = "work".into();

        app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert_eq!(
            app.accounts_page.form.mode,
            crate::app::AccountFormMode::ImapSmtp
        );
        assert!(app.accounts_page.form.pending_mode_switch.is_none());
    }

    #[test]
    fn account_form_mode_switch_confirmation_cancel_keeps_mode() {
        let mut app = App::new();
        app.apply(Action::OpenAccountFormNew);
        app.accounts_page.form.key = "work".into();

        app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));

        assert_eq!(
            app.accounts_page.form.mode,
            crate::app::AccountFormMode::Gmail
        );
        assert!(app.accounts_page.form.pending_mode_switch.is_none());
    }

    #[test]
    fn flattened_sidebar_navigation_reaches_saved_searches() {
        let mut app = App::new();
        app.labels = vec![Label {
            id: LabelId::new(),
            account_id: AccountId::new(),
            provider_id: "inbox".into(),
            name: "INBOX".into(),
            kind: LabelKind::System,
            color: None,
            unread_count: 1,
            total_count: 3,
        }];
        app.saved_searches = vec![SavedSearch {
            id: SavedSearchId::new(),
            account_id: None,
            name: "Unread".into(),
            query: "is:unread".into(),
            search_mode: SearchMode::Lexical,
            sort: SortOrder::DateDesc,
            icon: None,
            position: 0,
            created_at: chrono::Utc::now(),
        }];
        app.active_pane = ActivePane::Sidebar;

        let _ = app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        let _ = app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        let _ = app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));

        assert!(matches!(
            app.selected_sidebar_item(),
            Some(super::app::SidebarItem::SavedSearch(_))
        ));
    }

    #[test]
    fn toggle_select_advances_cursor_and_updates_preview() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(2);
        app.all_envelopes = app.envelopes.clone();
        app.apply(Action::OpenSelected);
        app.active_pane = ActivePane::MailList;

        app.apply(Action::ToggleSelect);

        assert_eq!(app.selected_index, 1);
        assert_eq!(
            app.viewing_envelope.as_ref().map(|env| env.id.clone()),
            Some(app.envelopes[1].id.clone())
        );
        assert!(matches!(
            app.body_view_state,
            BodyViewState::Loading { ref preview }
                if preview.as_deref() == Some("Snippet 1")
        ));
    }

    #[test]
    fn toggle_select_in_message_view_keeps_current_message_visible() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(2);
        app.all_envelopes = app.envelopes.clone();
        app.apply(Action::OpenSelected);

        let original_id = app.viewing_envelope.as_ref().unwrap().id.clone();
        app.apply(Action::ToggleSelect);

        assert_eq!(app.selected_index, 0);
        assert_eq!(
            app.viewing_envelope.as_ref().map(|env| env.id.clone()),
            Some(original_id.clone())
        );
        assert!(app.selected_set.contains(&original_id));
    }

    #[test]
    fn label_count_updates_preserve_sidebar_selection_identity() {
        let mut app = App::new();
        app.labels = make_test_labels();

        let selected_index = app
            .sidebar_items()
            .iter()
            .position(|item| matches!(item, super::app::SidebarItem::Label(label) if label.name == "Work"))
            .unwrap();
        app.sidebar_selected = selected_index;

        handle_daemon_event(
            &mut app,
            DaemonEvent::LabelCountsUpdated {
                counts: vec![
                    LabelCount {
                        label_id: LabelId::from_provider_id("test", "STARRED"),
                        unread_count: 0,
                        total_count: 0,
                    },
                    LabelCount {
                        label_id: LabelId::from_provider_id("test", "SENT"),
                        unread_count: 0,
                        total_count: 0,
                    },
                ],
            },
        );

        assert!(matches!(
            app.selected_sidebar_item(),
            Some(super::app::SidebarItem::Label(label)) if label.name == "Work"
        ));
    }

    #[test]
    fn labels_refresh_preserves_active_label_context_when_label_becomes_empty() {
        let mut app = App::new();
        app.labels = make_test_labels();
        let work = app
            .labels
            .iter()
            .find(|label| label.name == "Work")
            .unwrap()
            .clone();
        app.active_label = Some(work.id.clone());
        app.sidebar_selected = app
            .sidebar_items()
            .iter()
            .position(
                |item| matches!(item, super::app::SidebarItem::Label(label) if label.id == work.id),
            )
            .unwrap();

        let refreshed = app
            .labels
            .iter()
            .filter(|label| label.id != work.id)
            .cloned()
            .collect();

        super::apply_labels_refresh(&mut app, refreshed);

        let preserved = app.labels.iter().find(|label| label.id == work.id).unwrap();
        assert_eq!(preserved.unread_count, 0);
        assert_eq!(preserved.total_count, 0);
        assert_eq!(app.active_label.as_ref(), Some(&work.id));
        assert!(matches!(
            app.selected_sidebar_item(),
            Some(super::app::SidebarItem::Label(label)) if label.id == work.id
        ));
        assert_eq!(app.status_bar_state().mailbox_name, "Work");
    }

    #[test]
    fn opening_search_result_keeps_search_workspace_open() {
        let mut app = App::new();
        app.screen = Screen::Search;
        app.search_page.results = make_test_envelopes(2);
        app.search_page.selected_index = 1;

        app.apply(Action::OpenSelected);

        assert_eq!(app.screen, Screen::Search);
        assert_eq!(app.search_page.active_pane, SearchPane::Preview);
        assert_eq!(
            app.viewing_envelope.as_ref().map(|env| env.id.clone()),
            Some(app.search_page.results[1].id.clone())
        );
    }

    #[test]
    fn attachment_list_opens_modal_for_current_message() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(1);
        app.all_envelopes = app.envelopes.clone();
        let env = app.envelopes[0].clone();
        app.body_cache.insert(
            env.id.clone(),
            MessageBody {
                message_id: env.id.clone(),
                text_plain: Some("hello".into()),
                text_html: None,
                attachments: vec![AttachmentMeta {
                    id: AttachmentId::new(),
                    message_id: env.id.clone(),
                    filename: "report.pdf".into(),
                    mime_type: "application/pdf".into(),
                    disposition: mxr_core::types::AttachmentDisposition::Attachment,
                    content_id: None,
                    content_location: None,
                    size_bytes: 1024,
                    local_path: None,
                    provider_id: "att-1".into(),
                }],
                fetched_at: chrono::Utc::now(),
                metadata: Default::default(),
            },
        );

        app.apply(Action::OpenSelected);
        app.apply(Action::AttachmentList);

        assert!(app.attachment_panel.visible);
        assert_eq!(app.attachment_panel.attachments.len(), 1);
        assert_eq!(app.attachment_panel.attachments[0].filename, "report.pdf");
    }

    #[test]
    fn search_preview_attachment_key_opens_modal() {
        let mut app = App::new();
        let mut results = make_test_envelopes(1);
        results[0].has_attachments = true;
        let env = results[0].clone();
        app.screen = Screen::Search;
        app.search_page.results = results;
        app.search_page.session_active = true;
        app.search_page.active_pane = SearchPane::Preview;
        app.viewed_thread_messages = vec![env.clone()];
        app.viewing_envelope = Some(env.clone());
        app.body_cache.insert(
            env.id.clone(),
            MessageBody {
                message_id: env.id.clone(),
                text_plain: Some("hello".into()),
                text_html: None,
                attachments: vec![AttachmentMeta {
                    id: AttachmentId::new(),
                    message_id: env.id.clone(),
                    filename: "report.pdf".into(),
                    mime_type: "application/pdf".into(),
                    disposition: mxr_core::types::AttachmentDisposition::Attachment,
                    content_id: None,
                    content_location: None,
                    size_bytes: 1024,
                    local_path: None,
                    provider_id: "att-1".into(),
                }],
                fetched_at: chrono::Utc::now(),
                metadata: Default::default(),
            },
        );

        let action = app.handle_key(KeyEvent::new(KeyCode::Char('A'), KeyModifiers::SHIFT));
        assert_eq!(action, Some(Action::AttachmentList));

        app.apply(Action::AttachmentList);

        assert!(app.attachment_panel.visible);
        assert_eq!(app.attachment_panel.attachments.len(), 1);
        assert_eq!(app.attachment_panel.attachments[0].filename, "report.pdf");
    }

    #[test]
    fn search_preview_toggle_select_keeps_current_message_visible() {
        let mut app = App::new();
        let results = make_test_envelopes(2);
        let env = results[0].clone();
        app.screen = Screen::Search;
        app.search_page.results = results;
        app.search_page.session_active = true;
        app.search_page.active_pane = SearchPane::Preview;
        app.viewed_thread_messages = vec![env.clone()];
        app.viewing_envelope = Some(env.clone());

        let action = app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        assert_eq!(action, Some(Action::ToggleSelect));

        app.apply(Action::ToggleSelect);

        assert_eq!(app.search_page.selected_index, 0);
        assert_eq!(
            app.viewing_envelope
                .as_ref()
                .map(|current| current.id.clone()),
            Some(env.id.clone())
        );
        assert!(app.selected_set.contains(&env.id));
    }

    #[test]
    fn unchanged_editor_result_disables_send_actions() {
        let temp = std::env::temp_dir().join(format!(
            "mxr-compose-test-{}-{}.md",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let content = "---\nto: a@example.com\ncc: \"\"\nbcc: \"\"\nsubject: Hello\nfrom: me@example.com\nattach: []\n---\n\nBody\n";
        std::fs::write(&temp, content).unwrap();

        let pending = pending_send_from_edited_draft(&ComposeReadyData {
            draft_path: temp.clone(),
            cursor_line: 1,
            initial_content: content.to_string(),
        })
        .unwrap()
        .expect("pending send should exist");

        assert!(!pending.allow_send);

        let _ = std::fs::remove_file(temp);
    }

    #[test]
    fn send_key_is_ignored_for_unchanged_draft_confirmation() {
        let mut app = App::new();
        app.pending_send_confirm = Some(PendingSend {
            fm: mxr_compose::frontmatter::ComposeFrontmatter {
                to: "a@example.com".into(),
                cc: String::new(),
                bcc: String::new(),
                subject: "Hello".into(),
                from: "me@example.com".into(),
                in_reply_to: None,
                references: vec![],
                attach: vec![],
            },
            body: "Body".into(),
            draft_path: std::path::PathBuf::from("/tmp/draft.md"),
            allow_send: false,
        });

        let _ = app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));

        assert_eq!(
            app.pending_send_confirm
                .as_ref()
                .map(|pending| pending.allow_send),
            Some(false)
        );
        assert!(app.pending_mutation_queue.is_empty());
    }

    #[test]
    fn mail_list_l_opens_label_picker_not_message() {
        let mut app = App::new();
        app.active_pane = ActivePane::MailList;

        let action = app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));

        assert_eq!(action, Some(Action::ApplyLabel));
    }

    #[test]
    fn input_gc_opens_config_editor() {
        let mut h = InputHandler::new();

        assert_eq!(
            h.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE)),
            None
        );
        assert_eq!(
            h.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE)),
            Some(Action::EditConfig)
        );
    }

    #[test]
    fn input_g_shift_l_opens_logs() {
        let mut h = InputHandler::new();

        assert_eq!(
            h.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE)),
            None
        );
        assert_eq!(
            h.handle_key(KeyEvent::new(KeyCode::Char('L'), KeyModifiers::SHIFT)),
            Some(Action::OpenLogs)
        );
    }

    #[test]
    fn input_m_marks_read_and_archives() {
        let mut app = App::new();

        let action = app.handle_key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE));

        assert_eq!(action, Some(Action::MarkReadAndArchive));
    }

    #[test]
    fn reconnect_detection_treats_connection_refused_as_recoverable() {
        let result = Err(MxrError::Ipc(
            "IPC error: Connection refused (os error 61)".into(),
        ));

        assert!(crate::should_reconnect_ipc(&result));
    }

    #[test]
    fn autostart_detection_handles_refused_and_missing_socket() {
        let refused = std::io::Error::from(std::io::ErrorKind::ConnectionRefused);
        let missing = std::io::Error::from(std::io::ErrorKind::NotFound);
        let other = std::io::Error::from(std::io::ErrorKind::PermissionDenied);

        assert!(crate::should_autostart_daemon(&refused));
        assert!(crate::should_autostart_daemon(&missing));
        assert!(!crate::should_autostart_daemon(&other));
    }

    #[test]
    fn diagnostics_shift_l_opens_logs() {
        let mut app = App::new();
        app.screen = Screen::Diagnostics;

        let action = app.handle_key(KeyEvent::new(KeyCode::Char('L'), KeyModifiers::SHIFT));

        assert_eq!(action, Some(Action::OpenLogs));
    }

    #[test]
    fn diagnostics_tab_cycles_selected_pane() {
        let mut app = App::new();
        app.screen = Screen::Diagnostics;

        let action = app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));

        assert!(action.is_none());
        assert_eq!(
            app.diagnostics_page.selected_pane,
            crate::app::DiagnosticsPaneKind::Data
        );
    }

    #[test]
    fn diagnostics_enter_toggles_fullscreen_for_selected_pane() {
        let mut app = App::new();
        app.screen = Screen::Diagnostics;
        app.diagnostics_page.selected_pane = crate::app::DiagnosticsPaneKind::Logs;

        assert!(app
            .handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .is_none());
        assert_eq!(
            app.diagnostics_page.fullscreen_pane,
            Some(crate::app::DiagnosticsPaneKind::Logs)
        );
        assert!(app
            .handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .is_none());
        assert_eq!(app.diagnostics_page.fullscreen_pane, None);
    }

    #[test]
    fn diagnostics_d_opens_selected_pane_details() {
        let mut app = App::new();
        app.screen = Screen::Diagnostics;
        app.diagnostics_page.selected_pane = crate::app::DiagnosticsPaneKind::Events;

        let action = app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));

        assert_eq!(action, Some(Action::OpenDiagnosticsPaneDetails));
    }

    #[test]
    fn back_clears_selection_before_other_mail_list_back_behavior() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(2);
        app.all_envelopes = app.envelopes.clone();
        app.selected_set.insert(app.envelopes[0].id.clone());

        app.apply(Action::Back);

        assert!(app.selected_set.is_empty());
        assert_eq!(app.status_message.as_deref(), Some("Selection cleared"));
    }

    #[test]
    fn bulk_archive_requires_confirmation_before_queueing() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(3);
        app.all_envelopes = app.envelopes.clone();
        app.selected_set = app.envelopes.iter().map(|env| env.id.clone()).collect();

        app.apply(Action::Archive);

        assert!(app.pending_mutation_queue.is_empty());
        match app.pending_bulk_confirm.as_ref() {
            Some(confirm) => match &confirm.request {
                Request::Mutation(MutationCommand::Archive { message_ids }) => {
                    assert_eq!(message_ids.len(), 3);
                }
                other => panic!("Expected Archive bulk request, got {other:?}"),
            },
            None => panic!("Expected pending bulk confirmation"),
        }
    }

    #[test]
    fn confirming_bulk_archive_queues_mutation_and_clears_selection() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(3);
        app.all_envelopes = app.envelopes.clone();
        app.selected_set = app.envelopes.iter().map(|env| env.id.clone()).collect();
        app.apply(Action::Archive);

        app.apply(Action::OpenSelected);

        assert!(app.pending_bulk_confirm.is_none());
        assert_eq!(app.pending_mutation_queue.len(), 1);
        assert!(app.selected_set.is_empty());
    }

    #[test]
    fn command_palette_includes_major_mail_actions() {
        let labels: Vec<String> = default_commands()
            .into_iter()
            .map(|cmd| cmd.label)
            .collect();
        assert!(labels.contains(&"Reply".to_string()));
        assert!(labels.contains(&"Reply All".to_string()));
        assert!(labels.contains(&"Archive".to_string()));
        assert!(labels.contains(&"Delete".to_string()));
        assert!(labels.contains(&"Apply Label".to_string()));
        assert!(labels.contains(&"Snooze".to_string()));
        assert!(labels.contains(&"Clear Selection".to_string()));
        assert!(labels.contains(&"Open Accounts Page".to_string()));
        assert!(labels.contains(&"New IMAP/SMTP Account".to_string()));
        assert!(labels.contains(&"Set Default Account".to_string()));
        assert!(labels.contains(&"Edit Config".to_string()));
    }

    #[test]
    fn local_label_changes_update_open_message() {
        let mut app = App::new();
        app.labels = make_test_labels();
        app.envelopes = make_test_envelopes(1);
        app.all_envelopes = app.envelopes.clone();
        app.apply(Action::OpenSelected);

        let user_label = app
            .labels
            .iter()
            .find(|label| label.name == "Work")
            .unwrap()
            .clone();
        let message_id = app.envelopes[0].id.clone();

        app.apply_local_label_refs(
            std::slice::from_ref(&message_id),
            std::slice::from_ref(&user_label.name),
            &[],
        );

        assert!(app
            .viewing_envelope
            .as_ref()
            .unwrap()
            .label_provider_ids
            .contains(&user_label.provider_id));
    }

    #[test]
    fn snooze_action_opens_modal_then_queues_request() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(1);
        app.all_envelopes = app.envelopes.clone();

        app.apply(Action::Snooze);
        assert!(app.snooze_panel.visible);

        app.apply(Action::Snooze);
        assert!(!app.snooze_panel.visible);
        assert_eq!(app.pending_mutation_queue.len(), 1);
        match &app.pending_mutation_queue[0].0 {
            Request::Snooze {
                message_id,
                wake_at,
            } => {
                assert_eq!(message_id, &app.envelopes[0].id);
                assert!(*wake_at > chrono::Utc::now());
            }
            other => panic!("expected snooze request, got {other:?}"),
        }
    }

    #[test]
    fn open_selected_cache_miss_enters_loading_with_snippet_preview() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(1);
        app.all_envelopes = app.envelopes.clone();

        app.apply(Action::OpenSelected);

        assert!(matches!(
            app.body_view_state,
            BodyViewState::Loading { ref preview }
                if preview.as_deref() == Some("Snippet 0")
        ));
        assert_eq!(app.queued_body_fetches, vec![app.envelopes[0].id.clone()]);
        assert!(app.in_flight_body_requests.contains(&app.envelopes[0].id));
    }

    #[test]
    fn cached_plain_body_resolves_ready_state() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(1);
        app.all_envelopes = app.envelopes.clone();
        let env = app.envelopes[0].clone();

        app.body_cache.insert(
            env.id.clone(),
            MessageBody {
                message_id: env.id.clone(),
                text_plain: Some("Plain body".into()),
                text_html: None,
                attachments: vec![],
                fetched_at: chrono::Utc::now(),
                metadata: Default::default(),
            },
        );

        app.apply(Action::OpenSelected);

        assert!(matches!(
            app.body_view_state,
            BodyViewState::Ready {
                ref raw,
                ref rendered,
                source: BodySource::Plain,
                ..
            } if raw == "Plain body" && rendered == "Plain body"
        ));
    }

    #[test]
    fn cached_html_only_body_resolves_ready_state() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(1);
        app.all_envelopes = app.envelopes.clone();
        let env = app.envelopes[0].clone();

        app.body_cache.insert(
            env.id.clone(),
            MessageBody {
                message_id: env.id.clone(),
                text_plain: None,
                text_html: Some("<p>Hello html</p>".into()),
                attachments: vec![],
                fetched_at: chrono::Utc::now(),
                metadata: Default::default(),
            },
        );

        app.apply(Action::OpenSelected);

        assert!(matches!(
            app.body_view_state,
            BodyViewState::Ready {
                ref raw,
                ref rendered,
                source: BodySource::Html,
                ..
            } if raw == "<p>Hello html</p>"
                && rendered.contains("Hello html")
                && !rendered.contains("<p>")
        ));
    }

    #[test]
    fn cached_empty_body_resolves_empty_not_loading() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(1);
        app.all_envelopes = app.envelopes.clone();
        let env = app.envelopes[0].clone();

        app.body_cache.insert(
            env.id.clone(),
            MessageBody {
                message_id: env.id.clone(),
                text_plain: None,
                text_html: None,
                attachments: vec![],
                fetched_at: chrono::Utc::now(),
                metadata: Default::default(),
            },
        );

        app.apply(Action::OpenSelected);

        assert!(matches!(
            app.body_view_state,
            BodyViewState::Empty { ref preview }
                if preview.as_deref() == Some("Snippet 0")
        ));
    }

    #[test]
    fn body_fetch_error_resolves_error_not_loading() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(1);
        app.all_envelopes = app.envelopes.clone();
        app.apply(Action::OpenSelected);
        let env = app.envelopes[0].clone();

        app.resolve_body_fetch_error(&env.id, "boom".into());

        assert!(matches!(
            app.body_view_state,
            BodyViewState::Error { ref message, ref preview }
                if message == "boom" && preview.as_deref() == Some("Snippet 0")
        ));
        assert!(!app.in_flight_body_requests.contains(&env.id));
    }

    #[test]
    fn stale_body_response_does_not_clobber_current_view() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(2);
        app.all_envelopes = app.envelopes.clone();

        app.apply(Action::OpenSelected);
        let first = app.envelopes[0].clone();
        app.active_pane = ActivePane::MailList;
        app.apply(Action::MoveDown);
        let second = app.envelopes[1].clone();

        app.resolve_body_success(MessageBody {
            message_id: first.id.clone(),
            text_plain: Some("Old body".into()),
            text_html: None,
            attachments: vec![],
            fetched_at: chrono::Utc::now(),
            metadata: Default::default(),
        });

        assert_eq!(
            app.viewing_envelope.as_ref().map(|env| env.id.clone()),
            Some(second.id)
        );
        assert!(matches!(
            app.body_view_state,
            BodyViewState::Loading { ref preview }
                if preview.as_deref() == Some("Snippet 1")
        ));
    }

    #[test]
    fn reader_mode_toggle_shows_raw_html_when_disabled() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(1);
        app.all_envelopes = app.envelopes.clone();
        let env = app.envelopes[0].clone();
        app.body_cache.insert(
            env.id.clone(),
            MessageBody {
                message_id: env.id.clone(),
                text_plain: None,
                text_html: Some("<p>Hello html</p>".into()),
                attachments: vec![],
                fetched_at: chrono::Utc::now(),
                metadata: Default::default(),
            },
        );

        app.apply(Action::OpenSelected);

        match &app.body_view_state {
            BodyViewState::Ready { raw, rendered, .. } => {
                assert_eq!(raw, "<p>Hello html</p>");
                assert_ne!(rendered, raw);
                assert!(rendered.contains("Hello html"));
            }
            other => panic!("expected ready state, got {other:?}"),
        }

        app.apply(Action::ToggleReaderMode);

        match &app.body_view_state {
            BodyViewState::Ready { raw, rendered, .. } => {
                assert_eq!(raw, "<p>Hello html</p>");
                assert_eq!(rendered, raw);
            }
            other => panic!("expected ready state, got {other:?}"),
        }

        app.apply(Action::ToggleReaderMode);

        match &app.body_view_state {
            BodyViewState::Ready { raw, rendered, .. } => {
                assert_eq!(raw, "<p>Hello html</p>");
                assert_ne!(rendered, raw);
                assert!(rendered.contains("Hello html"));
            }
            other => panic!("expected ready state, got {other:?}"),
        }
    }

    #[test]
    fn html_view_toggle_updates_mode_and_remote_content_status() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(1);
        app.all_envelopes = app.envelopes.clone();
        let env = app.envelopes[0].clone();
        app.body_cache.insert(
            env.id.clone(),
            MessageBody {
                message_id: env.id.clone(),
                text_plain: Some("Fallback plain".into()),
                text_html: Some(
                    "<p>Hello <img alt=\"Hero\" src=\"https://example.com/hero.png\"></p>".into(),
                ),
                attachments: vec![AttachmentMeta {
                    id: AttachmentId::new(),
                    message_id: env.id.clone(),
                    filename: "logo.png".into(),
                    mime_type: "image/png".into(),
                    disposition: AttachmentDisposition::Inline,
                    content_id: Some("logo@example.com".into()),
                    content_location: None,
                    size_bytes: 2048,
                    local_path: None,
                    provider_id: "att-inline".into(),
                }],
                fetched_at: chrono::Utc::now(),
                metadata: MessageMetadata {
                    text_plain_source: Some(BodyPartSource::Exact),
                    text_html_source: Some(BodyPartSource::Exact),
                    ..Default::default()
                },
            },
        );

        app.apply(Action::OpenSelected);

        match &app.body_view_state {
            BodyViewState::Ready {
                source: BodySource::Plain,
                metadata,
                ..
            } => {
                assert_eq!(metadata.mode, super::app::BodyViewMode::Text);
                assert!(metadata.inline_images);
                assert!(metadata.remote_content_available);
                assert!(metadata.remote_content_enabled);
            }
            other => panic!("expected text ready state, got {other:?}"),
        }

        app.apply(Action::ToggleHtmlView);

        match &app.body_view_state {
            BodyViewState::Ready {
                source: BodySource::Html,
                metadata,
                ..
            } => {
                assert_eq!(metadata.mode, super::app::BodyViewMode::Html);
                assert!(metadata.inline_images);
                assert!(metadata.remote_content_available);
                assert!(metadata.remote_content_enabled);
            }
            other => panic!("expected html ready state, got {other:?}"),
        }
        assert!(app
            .status_bar_state()
            .body_status
            .as_deref()
            .is_some_and(|status| status.contains("remote:on")));

        app.apply(Action::ToggleRemoteContent);

        match &app.body_view_state {
            BodyViewState::Ready { metadata, .. } => {
                assert_eq!(metadata.mode, super::app::BodyViewMode::Html);
                assert!(!metadata.remote_content_enabled);
            }
            other => panic!("expected html ready state, got {other:?}"),
        }
        assert!(app
            .status_bar_state()
            .body_status
            .as_deref()
            .is_some_and(|status| status.contains("remote:off")));
    }

    #[test]
    fn reader_mode_toggle_is_blocked_in_html_view() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(1);
        app.all_envelopes = app.envelopes.clone();
        let env = app.envelopes[0].clone();
        app.body_cache.insert(
            env.id.clone(),
            MessageBody {
                message_id: env.id.clone(),
                text_plain: None,
                text_html: Some("<p>Hello html</p>".into()),
                attachments: vec![],
                fetched_at: chrono::Utc::now(),
                metadata: MessageMetadata {
                    text_html_source: Some(BodyPartSource::Exact),
                    ..Default::default()
                },
            },
        );

        app.apply(Action::OpenSelected);
        app.apply(Action::ToggleHtmlView);
        let reader_mode_before = app.reader_mode;

        app.apply(Action::ToggleReaderMode);

        assert_eq!(app.reader_mode, reader_mode_before);
        assert_eq!(
            app.status_message.as_deref(),
            Some("Reader mode only applies in text view")
        );
    }

    #[test]
    fn reader_stats_visibility_respects_config() {
        let mut app = App::new();
        app.body_view_state = BodyViewState::Ready {
            raw: "Hello".into(),
            rendered: "Hello".into(),
            source: BodySource::Plain,
            metadata: BodyViewMetadata {
                mode: super::app::BodyViewMode::Text,
                provenance: Some(BodyPartSource::Exact),
                reader_applied: true,
                original_lines: Some(12),
                cleaned_lines: Some(7),
                ..BodyViewMetadata::default()
            },
        };

        app.show_reader_stats = false;
        assert!(app
            .status_bar_state()
            .body_status
            .as_deref()
            .is_some_and(|status| !status.contains("reader:7/12")));

        app.show_reader_stats = true;
        assert!(app
            .status_bar_state()
            .body_status
            .as_deref()
            .is_some_and(|status| status.contains("reader:7/12")));
    }
}
