#![cfg_attr(test, allow(clippy::panic, clippy::unwrap_used))]

mod account_workflow;
mod accounts_helpers;
pub mod action;
pub mod app;
mod async_result;
pub mod client;
mod compose_flow;
pub mod desktop_manifest;
mod editor;
pub mod input;
mod ipc;
pub mod keybindings;
mod local_io;
pub mod local_state;
mod runtime;
pub mod terminal_images;
#[cfg(test)]
mod test_fixtures;
pub mod theme;
pub mod ui;

use app::{App, AttachmentOperation};
use client::Client;
use crossterm::event::EventStream;
use futures::StreamExt;
use mxr_config::load_config;
use mxr_core::MxrError;
use mxr_protocol::{DaemonEvent, Request, Response, ResponseData};
use ratatui::crossterm::event::Event;
use tokio::sync::mpsc;
use tokio::time::Instant;

use crate::account_workflow::{
    daemon_socket_path, request_account_operation, run_account_save_workflow,
};
use crate::accounts_helpers::load_accounts_page_accounts;
use crate::async_result::{AsyncResult, UnsubscribeResultData};
use crate::compose_flow::{handle_compose_action, handle_compose_editor_status};
use crate::editor::{edit_tui_config, open_diagnostics_pane_details, open_tui_log_file};
use crate::ipc::{ipc_call, spawn_ipc_worker};
use crate::local_io::{handle_result as handle_local_io_result, submit_bug_report_write};
use crate::runtime::{
    spawn_replaceable_request_worker, spawn_task_worker, submit_task, ReplaceableRequest,
    ReplaceableRequestKey,
};

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
    let mut events = Some(EventStream::new());

    // Channels for async results
    let (result_tx, mut result_rx) = mpsc::unbounded_channel::<AsyncResult>();

    // Background IPC worker — also forwards daemon events to result_tx
    let bg = spawn_ipc_worker(socket_path, result_tx.clone());
    let html_assets =
        crate::terminal_images::spawn_html_image_asset_worker(bg.clone(), result_tx.clone());
    let html_decodes = crate::terminal_images::spawn_html_image_decode_worker(result_tx.clone());
    let replaceable = spawn_replaceable_request_worker(bg.clone(), result_tx.clone());
    let queued = spawn_task_worker(result_tx.clone());
    let local_io = spawn_task_worker(result_tx.clone());

    loop {
        crate::local_io::submit_pending_work(&mut app, &local_io);

        if app.pending_config_edit {
            app.pending_config_edit = false;
            let result = run_with_terminal_suspended(&mut terminal, &mut events, || {
                edit_tui_config(&mut app)
            });
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
            let result = run_with_terminal_suspended(&mut terminal, &mut events, open_tui_log_file);
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
            let result = run_with_terminal_suspended(&mut terminal, &mut events, || {
                open_diagnostics_pane_details(&app.diagnostics_page, pane)
            });
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
                        data: ResponseData::Bodies { bodies },
                    }) => Ok(bodies),
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
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
                let allow_remote = app.remote_content_enabled;
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

        if let Some(thread_id) = app.pending_thread_fetch.take() {
            app.in_flight_thread_fetch = Some(thread_id.clone());
            app.thread_request_id = app.thread_request_id.wrapping_add(1);
            let _ = replaceable.send(ReplaceableRequest::Thread {
                thread_id,
                request_id: app.thread_request_id,
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
        if let Some(pending) = app.pending_search.take() {
            let _ = replaceable.send(ReplaceableRequest::Search(pending));
        }

        if let Some(pending) = app.pending_search_count.take() {
            let _ = replaceable.send(ReplaceableRequest::SearchCount(pending));
        }

        if let Some(pending) = app.pending_unsubscribe_action.take() {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
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
                AsyncResult::Unsubscribe(result)
            });
        }

        if app.rules_page.refresh_pending {
            app.rules_page.refresh_pending = false;
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let resp = ipc_call(&bg, Request::ListRules).await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::Rules { rules },
                    }) => Ok(rules),
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                AsyncResult::Rules(result)
            });
        }

        if let Some(rule) = app.pending_rule_detail.take() {
            app.rule_detail_request_id = app.rule_detail_request_id.wrapping_add(1);
            let _ = replaceable.send(ReplaceableRequest::RuleDetail {
                rule,
                request_id: app.rule_detail_request_id,
                enqueued_at: Instant::now(),
            });
        }

        if let Some(rule) = app.pending_rule_history.take() {
            app.rule_history_request_id = app.rule_history_request_id.wrapping_add(1);
            let _ = replaceable.send(ReplaceableRequest::RuleHistory {
                rule,
                request_id: app.rule_history_request_id,
                enqueued_at: Instant::now(),
            });
        }

        if let Some(rule) = app.pending_rule_dry_run.take() {
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
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                AsyncResult::RuleDryRun(result)
            });
        }

        if let Some(rule) = app.pending_rule_form_load.take() {
            app.rule_form_request_id = app.rule_form_request_id.wrapping_add(1);
            let _ = replaceable.send(ReplaceableRequest::RuleForm {
                rule,
                request_id: app.rule_form_request_id,
                enqueued_at: Instant::now(),
            });
        }

        if let Some(rule) = app.pending_rule_delete.take() {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let resp = ipc_call(&bg, Request::DeleteRule { rule }).await;
                let result = match resp {
                    Ok(Response::Ok { .. }) => Ok(()),
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                };
                AsyncResult::RuleDeleted(result)
            });
        }

        if let Some(rule) = app.pending_rule_upsert.take() {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let resp = ipc_call(&bg, Request::UpsertRule { rule }).await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::RuleData { rule },
                    }) => Ok(rule),
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                AsyncResult::RuleUpsert(result)
            });
        }

        if app.pending_rule_form_save {
            app.pending_rule_form_save = false;
            let bg = bg.clone();
            let existing_rule = app.rules_page.form.existing_rule.clone();
            let name = app.rules_page.form.name.clone();
            let condition = app.rules_page.form.condition.clone();
            let action = app.rules_page.form.action.clone();
            let priority = app.rules_page.form.priority.parse::<i32>().unwrap_or(100);
            let enabled = app.rules_page.form.enabled;
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
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                AsyncResult::RuleUpsert(result)
            });
        }

        if app.diagnostics_page.refresh_pending {
            app.diagnostics_page.refresh_pending = false;
            app.pending_status_refresh = false;
            app.diagnostics_page.pending_requests = 4;
            app.diagnostics_request_id = app.diagnostics_request_id.wrapping_add(1);
            let request_id = app.diagnostics_request_id;
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
                    },
                ),
                (
                    ReplaceableRequestKey::DiagnosticsLogs,
                    Request::GetLogs {
                        limit: 50,
                        level: None,
                    },
                ),
            ] {
                let _ = replaceable.send(ReplaceableRequest::Diagnostics {
                    kind,
                    request,
                    request_id,
                    enqueued_at: Instant::now(),
                });
            }
        }

        if app.pending_status_refresh {
            app.pending_status_refresh = false;
            app.status_request_id = app.status_request_id.wrapping_add(1);
            let _ = replaceable.send(ReplaceableRequest::Status {
                request_id: app.status_request_id,
                enqueued_at: Instant::now(),
            });
        }

        if app.accounts_page.refresh_pending {
            app.accounts_page.refresh_pending = false;
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let result = load_accounts_page_accounts(&bg).await;
                AsyncResult::Accounts(result)
            });
        }

        if app.pending_labels_refresh {
            app.pending_labels_refresh = false;
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let resp = ipc_call(&bg, Request::ListLabels { account_id: None }).await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::Labels { labels },
                    }) => Ok(labels),
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                AsyncResult::Labels(result)
            });
        }

        if app.pending_all_envelopes_refresh {
            app.pending_all_envelopes_refresh = false;
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
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                AsyncResult::AllEnvelopes(result)
            });
        }

        if app.pending_subscriptions_refresh {
            app.pending_subscriptions_refresh = false;
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
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                AsyncResult::Subscriptions(result)
            });
        }

        if let Some(account) = app.pending_account_save.take() {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let result = run_account_save_workflow(&bg, account).await;
                AsyncResult::AccountOperation(result)
            });
        }

        if let Some(account) = app.pending_account_test.take() {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let result =
                    request_account_operation(&bg, Request::TestAccountConfig { account }).await;
                AsyncResult::AccountOperation(result)
            });
        }

        if let Some((account, reauthorize)) = app.pending_account_authorize.take() {
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

        if let Some(key) = app.pending_account_set_default.take() {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let result =
                    request_account_operation(&bg, Request::SetDefaultAccount { key }).await;
                AsyncResult::AccountOperation(result)
            });
        }

        if app.pending_bug_report {
            app.pending_bug_report = false;
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
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                AsyncResult::BugReport(result)
            });
        }

        if let Some(pending) = app.pending_attachment_action.take() {
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
                AsyncResult::AttachmentFile {
                    operation: pending.operation,
                    result,
                }
            });
        }

        // Spawn non-blocking label envelope fetch
        if let Some(label_id) = app.pending_label_fetch.take() {
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
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                AsyncResult::LabelEnvelopes(envelopes)
            });
        }

        // Drain pending mutations
        for (req, effect) in app.pending_mutation_queue.drain(..) {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let resp = ipc_call(&bg, req).await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::Ack,
                    }) => Ok(effect),
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                AsyncResult::MutationResult(result)
            });
        }

        // Handle thread export (uses daemon ExportThread which runs mxr-export)
        if let Some(thread_id) = app.pending_export_thread.take() {
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
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                AsyncResult::ExportResult(result)
            });
        }

        // Handle compose actions
        if let Some(compose_action) = app.pending_compose.take() {
            let bg = bg.clone();
            let _ = submit_task(&queued, async move {
                let result = handle_compose_action(&bg, compose_action).await;
                AsyncResult::ComposeReady(result)
            });
        }

        tokio::select! {
            event = events.as_mut().expect("event stream").next() => {
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
                                app.apply_search_page_results(append, results);
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
                        AsyncResult::RuleDetail {
                            request_id,
                            result: Ok(rule),
                        } => {
                            if request_id != app.rule_detail_request_id {
                                tracing::trace!(request_id, current_id = app.rule_detail_request_id, "tui stale rule detail dropped");
                                continue;
                            }
                            app.rules_page.detail = Some(rule);
                            app.rules_page.panel = app::RulesPanel::Details;
                        }
                        AsyncResult::RuleDetail {
                            request_id,
                            result: Err(e),
                        } => {
                            if request_id != app.rule_detail_request_id {
                                tracing::trace!(request_id, current_id = app.rule_detail_request_id, "tui stale rule detail dropped");
                                continue;
                            }
                            app.rules_page.status = Some(format!("Rule error: {e}"));
                        }
                        AsyncResult::RuleHistory {
                            request_id,
                            result: Ok(entries),
                        } => {
                            if request_id != app.rule_history_request_id {
                                tracing::trace!(request_id, current_id = app.rule_history_request_id, "tui stale rule history dropped");
                                continue;
                            }
                            app.rules_page.history = entries;
                        }
                        AsyncResult::RuleHistory {
                            request_id,
                            result: Err(e),
                        } => {
                            if request_id != app.rule_history_request_id {
                                tracing::trace!(request_id, current_id = app.rule_history_request_id, "tui stale rule history dropped");
                                continue;
                            }
                            app.rules_page.status = Some(format!("History error: {e}"));
                        }
                        AsyncResult::RuleDryRun(Ok(results)) => {
                            app.rules_page.dry_run = results;
                        }
                        AsyncResult::RuleDryRun(Err(e)) => {
                            app.rules_page.status = Some(format!("Dry-run error: {e}"));
                        }
                        AsyncResult::RuleForm {
                            request_id,
                            result: Ok(form),
                        } => {
                            if request_id != app.rule_form_request_id {
                                tracing::trace!(request_id, current_id = app.rule_form_request_id, "tui stale rule form dropped");
                                continue;
                            }
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
                        AsyncResult::RuleForm {
                            request_id,
                            result: Err(e),
                        } => {
                            if request_id != app.rule_form_request_id {
                                tracing::trace!(request_id, current_id = app.rule_form_request_id, "tui stale rule form dropped");
                                continue;
                            }
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
                        AsyncResult::Diagnostics { request_id, result } => {
                            if request_id != app.diagnostics_request_id {
                                tracing::trace!(request_id, current_id = app.diagnostics_request_id, "tui stale diagnostics dropped");
                                continue;
                            }
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
                        AsyncResult::Status {
                            request_id,
                            result: Ok(snapshot),
                        } => {
                            if request_id != app.status_request_id {
                                tracing::trace!(request_id, current_id = app.status_request_id, "tui stale status dropped");
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
                            if request_id != app.status_request_id {
                                tracing::trace!(request_id, current_id = app.status_request_id, "tui stale status dropped");
                                continue;
                            }
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
                            submit_bug_report_write(&local_io, content);
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
                            request_id,
                            result: Ok((thread, messages)),
                        } => {
                            if request_id != app.thread_request_id {
                                tracing::trace!(request_id, current_id = app.thread_request_id, "tui stale thread dropped");
                                continue;
                            }
                            app.resolve_thread_success(thread, messages);
                            let _ = thread_id;
                        }
                        AsyncResult::Thread {
                            thread_id,
                            request_id,
                            result: Err(_),
                        } => {
                            if request_id != app.thread_request_id {
                                tracing::trace!(request_id, current_id = app.thread_request_id, "tui stale thread dropped");
                                continue;
                            }
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
                            let status = run_with_terminal_suspended(&mut terminal, &mut events, || {
                                let editor = mxr_compose::editor::resolve_editor(None);
                                std::process::Command::new(&editor)
                                    .arg(format!("+{}", data.cursor_line))
                                    .arg(&data.draft_path)
                                    .status()
                            });
                            handle_compose_editor_status(&mut app, &data, status).await;
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
                        other => {
                            let Some(other) = handle_local_io_result(&mut app, other) else {
                                continue;
                            };
                            match other {
                                AsyncResult::DaemonEvent(event) => {
                                    handle_daemon_event(&mut app, event)
                                }
                                _ => {}
                            }
                        }
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

#[cfg(test)]
mod tests {
    use super::action::Action;
    use super::app::{
        ActivePane, App, BodySource, BodyViewMetadata, BodyViewState, LayoutMode, MutationEffect,
        PendingSearchRequest, PendingSendMode, Screen, SearchPane, SearchTarget, SidebarItem,
        SEARCH_PAGE_SIZE,
    };
    use super::input::InputHandler;
    use super::ui::command_palette::default_commands;
    use super::ui::command_palette::CommandPalette;
    use super::ui::search_bar::SearchBar;
    use super::ui::status_bar;
    use super::{
        app::MailListMode, apply_all_envelopes_refresh, handle_daemon_event,
        run_with_terminal_suspended_with,
    };
    use crate::app::PendingSend;
    use crate::async_result::{ComposeReadyData, SearchResultData};
    use crate::compose_flow::{handle_compose_editor_status, pending_send_from_edited_draft};
    use crate::runtime::{enqueue_replaceable_request, ReplaceableRequest};
    use crate::test_fixtures::TestEnvelopeBuilder;
    use mxr_config::RenderConfig;
    use mxr_core::id::*;
    use mxr_core::types::*;
    use mxr_core::MxrError;
    use mxr_protocol::{DaemonEvent, LabelCount, MutationCommand, Request};
    use mxr_test_support::render_to_string;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::collections::VecDeque;
    use std::os::unix::process::ExitStatusExt;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use tokio::time::Instant;

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

    struct TestEventSource {
        id: usize,
        dropped: Arc<AtomicBool>,
    }

    impl Drop for TestEventSource {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::SeqCst);
        }
    }

    fn exit_status(code: i32) -> std::process::ExitStatus {
        std::process::ExitStatus::from_raw(code)
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
    fn suspended_handoff_drops_old_event_source_before_running_action() {
        let old_dropped = Arc::new(AtomicBool::new(false));
        let new_created = Arc::new(AtomicBool::new(false));
        let order = Arc::new(Mutex::new(Vec::new()));
        let mut terminal = 1usize;
        let mut events = Some(TestEventSource {
            id: 1,
            dropped: old_dropped.clone(),
        });

        let result = run_with_terminal_suspended_with(
            &mut terminal,
            &mut events,
            {
                let order = order.clone();
                move || order.lock().unwrap().push("restore")
            },
            {
                let order = order.clone();
                move || {
                    order.lock().unwrap().push("init");
                    2usize
                }
            },
            {
                let order = order.clone();
                let new_created = new_created.clone();
                move || {
                    order.lock().unwrap().push("events");
                    new_created.store(true, Ordering::SeqCst);
                    TestEventSource {
                        id: 2,
                        dropped: Arc::new(AtomicBool::new(false)),
                    }
                }
            },
            {
                let order = order.clone();
                let old_dropped = old_dropped.clone();
                let new_created = new_created.clone();
                move || {
                    assert!(old_dropped.load(Ordering::SeqCst));
                    assert!(!new_created.load(Ordering::SeqCst));
                    order.lock().unwrap().push("run");
                    "done"
                }
            },
        );

        assert_eq!(result, "done");
        assert_eq!(terminal, 2);
        assert_eq!(events.as_ref().map(|event| event.id), Some(2));
        assert_eq!(
            order.lock().unwrap().as_slice(),
            ["restore", "run", "init", "events"]
        );
    }

    #[tokio::test]
    async fn compose_editor_success_opens_send_confirmation() {
        let temp = std::env::temp_dir().join(format!(
            "mxr-compose-editor-success-{}-{}.md",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let content = "---\nto: a@example.com\ncc: \"\"\nbcc: \"\"\nsubject: Hello\nfrom: me@example.com\nattach: []\n---\n\nBody\n";
        std::fs::write(&temp, content).unwrap();

        let data = ComposeReadyData {
            draft_path: temp.clone(),
            cursor_line: 1,
            initial_content: String::new(),
        };
        let mut app = App::new();

        handle_compose_editor_status(&mut app, &data, Ok(exit_status(0))).await;

        assert_eq!(
            app.pending_send_confirm
                .as_ref()
                .map(|pending| pending.mode),
            Some(PendingSendMode::SendOrSave)
        );
        assert!(app.status_message.is_none());

        let _ = std::fs::remove_file(temp);
    }

    #[tokio::test]
    async fn compose_editor_cancel_discards_draft() {
        let temp = std::env::temp_dir().join(format!(
            "mxr-compose-editor-cancel-{}-{}.md",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::write(&temp, "---\n").unwrap();

        let data = ComposeReadyData {
            draft_path: temp.clone(),
            cursor_line: 1,
            initial_content: String::new(),
        };
        let mut app = App::new();

        handle_compose_editor_status(&mut app, &data, Ok(exit_status(1))).await;

        assert_eq!(app.status_message.as_deref(), Some("Draft discarded"));
        assert!(app.pending_send_confirm.is_none());
        assert!(!temp.exists());
    }

    #[test]
    fn suspended_handoff_preserves_non_compose_results() {
        let old_dropped = Arc::new(AtomicBool::new(false));
        let mut terminal = 1usize;
        let mut events = Some(TestEventSource {
            id: 1,
            dropped: old_dropped.clone(),
        });

        let result: Result<String, MxrError> = run_with_terminal_suspended_with(
            &mut terminal,
            &mut events,
            || {},
            || 2usize,
            || TestEventSource {
                id: 2,
                dropped: Arc::new(AtomicBool::new(false)),
            },
            || {
                assert!(old_dropped.load(Ordering::SeqCst));
                Ok("Log open cancelled".into())
            },
        );

        assert_eq!(result.unwrap(), "Log open cancelled");
        assert_eq!(terminal, 2);
        assert_eq!(events.as_ref().map(|event| event.id), Some(2));
    }

    #[test]
    fn replaceable_request_queue_supersedes_older_status_refresh() {
        let mut pending = VecDeque::new();
        enqueue_replaceable_request(
            &mut pending,
            ReplaceableRequest::Status {
                request_id: 1,
                enqueued_at: Instant::now(),
            },
        );
        enqueue_replaceable_request(
            &mut pending,
            ReplaceableRequest::Status {
                request_id: 2,
                enqueued_at: Instant::now(),
            },
        );

        assert_eq!(pending.len(), 1);
        match pending.pop_front() {
            Some(ReplaceableRequest::Status { request_id, .. }) => assert_eq!(request_id, 2),
            other => panic!("expected status request, got {other:?}"),
        }
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
    fn input_uppercase_shortcuts_work_without_explicit_shift_modifier() {
        let mut h = InputHandler::new();
        assert_eq!(
            h.handle_key(KeyEvent::new(KeyCode::Char('H'), KeyModifiers::NONE)),
            Some(Action::ViewportTop)
        );
        assert_eq!(
            h.handle_key(KeyEvent::new(KeyCode::Char('A'), KeyModifiers::NONE)),
            Some(Action::AttachmentList)
        );
        assert_eq!(
            h.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE)),
            None
        );
        assert_eq!(
            h.handle_key(KeyEvent::new(KeyCode::Char('L'), KeyModifiers::NONE)),
            Some(Action::OpenLogs)
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
    fn open_in_browser_action_queues_html_body_open() {
        let mut app = App::new();
        let env = make_test_envelopes(1).remove(0);
        app.viewing_envelope = Some(env.clone());
        app.body_cache.insert(
            env.id.clone(),
            MessageBody {
                message_id: env.id.clone(),
                text_plain: Some("Plain body".into()),
                text_html: Some("<p>Hello html</p>".into()),
                attachments: vec![],
                fetched_at: chrono::Utc::now(),
                metadata: Default::default(),
            },
        );

        app.apply(Action::OpenInBrowser);

        let pending = app
            .pending_browser_open
            .as_ref()
            .expect("browser open should be queued");
        assert_eq!(pending.message_id, env.id);
        assert_eq!(pending.document, "<p>Hello html</p>");
        assert_eq!(app.status_message.as_deref(), Some("Opening in browser..."));
    }

    #[test]
    fn open_in_browser_action_wraps_plain_text_when_html_is_missing() {
        let mut app = App::new();
        let env = make_test_envelopes(1).remove(0);
        app.viewing_envelope = Some(env.clone());
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

        app.apply(Action::OpenInBrowser);

        let pending = app
            .pending_browser_open
            .as_ref()
            .expect("plain text should still open in browser");
        assert_eq!(pending.message_id, env.id);
        assert!(pending.document.contains("<pre>Plain body</pre>"));
        assert!(pending.document.contains("<!doctype html>"));
        assert_eq!(app.status_message.as_deref(), Some("Opening in browser..."));
    }

    #[test]
    fn open_in_browser_action_rejects_messages_without_readable_body() {
        let mut app = App::new();
        let env = make_test_envelopes(1).remove(0);
        app.viewing_envelope = Some(env.clone());
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

        app.apply(Action::OpenInBrowser);

        assert!(app.pending_browser_open.is_none());
        assert_eq!(
            app.status_message.as_deref(),
            Some("No readable body available")
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
    fn search_open_message_follows_cursor_after_returning_to_results() {
        let mut app = App::new();
        let results = make_test_envelopes(3);
        app.screen = Screen::Search;
        app.search_page.query = "deploy".into();
        app.search_page.results = results.clone();
        app.search_page.session_active = true;
        app.all_envelopes = results.clone();

        app.apply(Action::OpenSelected);
        assert_eq!(
            app.viewing_envelope.as_ref().map(|env| env.id.clone()),
            Some(results[0].id.clone())
        );

        assert!(app
            .handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE))
            .is_none());
        assert_eq!(app.search_page.active_pane, SearchPane::Results);
        assert_eq!(
            app.viewing_envelope.as_ref().map(|env| env.id.clone()),
            Some(results[0].id.clone())
        );

        assert!(app
            .handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
            .is_none());
        assert_eq!(app.search_page.selected_index, 1);
        assert_eq!(
            app.viewing_envelope.as_ref().map(|env| env.id.clone()),
            Some(results[1].id.clone())
        );
    }

    #[test]
    fn search_results_allow_mail_actions_without_preview_focus() {
        let mut app = App::new();
        let results = make_test_envelopes(2);
        app.screen = Screen::Search;
        app.search_page.query = "deploy".into();
        app.search_page.results = results.clone();
        app.search_page.session_active = true;
        app.search_page.selected_index = 1;
        app.search_page.active_pane = SearchPane::Results;

        let action = app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
        assert_eq!(action, Some(Action::Star));

        app.apply(action.expect("star action should be available from search results"));

        assert!(app.search_page.results[1]
            .flags
            .contains(MessageFlags::STARRED));
        assert_eq!(app.pending_mutation_queue.len(), 1);
        match &app.pending_mutation_queue[0].0 {
            Request::Mutation(MutationCommand::Star {
                message_ids,
                starred,
            }) => {
                assert_eq!(message_ids, &vec![results[1].id.clone()]);
                assert!(*starred);
            }
            other => panic!("expected star mutation, got {other:?}"),
        }
    }

    #[test]
    fn search_results_follow_mail_list_mode_and_open_thread_rows() {
        let mut app = App::new();
        let thread_id = ThreadId::new();
        let now = chrono::Utc::now();
        let older = TestEnvelopeBuilder::new()
            .provider_id("thread-old")
            .thread_id(thread_id.clone())
            .subject("Older hit")
            .date(now - chrono::Duration::minutes(5))
            .build();
        let newer = TestEnvelopeBuilder::new()
            .provider_id("thread-new")
            .thread_id(thread_id)
            .subject("Newer hit")
            .date(now)
            .build();
        let other = TestEnvelopeBuilder::new()
            .provider_id("other-thread")
            .subject("Other thread")
            .date(now - chrono::Duration::minutes(1))
            .build();
        let results = vec![older, newer.clone(), other];
        app.mail_list_mode = MailListMode::Messages;
        app.screen = Screen::Search;
        app.search_page.query = "deploy".into();
        app.search_page.results = results.clone();
        app.search_page.session_active = true;
        app.all_envelopes = results;

        app.apply(Action::ToggleMailListMode);

        assert_eq!(app.search_row_count(), 2);
        assert_eq!(
            app.selected_search_envelope().map(|env| env.id.clone()),
            Some(newer.id.clone())
        );

        app.apply(Action::OpenSelected);

        assert_eq!(app.search_page.active_pane, SearchPane::Preview);
        assert_eq!(
            app.viewing_envelope.as_ref().map(|env| env.id.clone()),
            Some(newer.id.clone())
        );
    }

    #[test]
    fn search_results_refresh_preserves_open_row_when_it_still_exists() {
        let mut app = App::new();
        let results = make_test_envelopes(3);
        app.screen = Screen::Search;
        app.search_page.query = "deploy".into();
        app.search_page.results = results.clone();
        app.search_page.session_active = true;
        app.search_page.selected_index = 1;
        app.all_envelopes = results.clone();

        app.apply(Action::OpenSelected);
        app.apply_search_page_results(
            false,
            SearchResultData {
                envelopes: vec![results[0].clone(), results[1].clone()],
                scores: std::collections::HashMap::new(),
                has_more: false,
            },
        );

        assert_eq!(app.search_page.selected_index, 1);
        assert!(app.search_page.result_selected);
        assert_eq!(
            app.viewing_envelope.as_ref().map(|env| env.id.clone()),
            Some(results[1].id.clone())
        );
    }

    #[test]
    fn search_results_refresh_clears_open_message_when_selected_row_disappears() {
        let mut app = App::new();
        let results = make_test_envelopes(3);
        app.screen = Screen::Search;
        app.search_page.query = "deploy".into();
        app.search_page.results = results.clone();
        app.search_page.session_active = true;
        app.search_page.selected_index = 1;
        app.all_envelopes = results.clone();

        app.apply(Action::OpenSelected);
        app.apply_search_page_results(
            false,
            SearchResultData {
                envelopes: vec![results[0].clone()],
                scores: std::collections::HashMap::new(),
                has_more: false,
            },
        );

        assert_eq!(app.search_page.selected_index, 0);
        assert!(!app.search_page.result_selected);
        assert_eq!(app.search_page.active_pane, SearchPane::Results);
        assert!(app.viewing_envelope.is_none());
        assert!(app.viewed_thread_messages.is_empty());
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
    fn attachment_list_sorts_file_attachments_before_inline_images() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(1);
        app.all_envelopes = app.envelopes.clone();
        let env = app.envelopes[0].clone();
        app.body_cache.insert(
            env.id.clone(),
            MessageBody {
                message_id: env.id.clone(),
                text_plain: Some("hello".into()),
                text_html: Some("<img src=\"cid:inline-1\">".into()),
                attachments: vec![
                    AttachmentMeta {
                        id: AttachmentId::new(),
                        message_id: env.id.clone(),
                        filename: "inline-1.png".into(),
                        mime_type: "image/png".into(),
                        disposition: mxr_core::types::AttachmentDisposition::Inline,
                        content_id: Some("inline-1".into()),
                        content_location: None,
                        size_bytes: 10,
                        local_path: None,
                        provider_id: "att-inline-1".into(),
                    },
                    AttachmentMeta {
                        id: AttachmentId::new(),
                        message_id: env.id.clone(),
                        filename: "budget.xlsx".into(),
                        mime_type:
                            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
                                .into(),
                        disposition: mxr_core::types::AttachmentDisposition::Attachment,
                        content_id: None,
                        content_location: None,
                        size_bytes: 20,
                        local_path: None,
                        provider_id: "att-xlsx".into(),
                    },
                    AttachmentMeta {
                        id: AttachmentId::new(),
                        message_id: env.id.clone(),
                        filename: "inline-2.png".into(),
                        mime_type: "image/png".into(),
                        disposition: mxr_core::types::AttachmentDisposition::Inline,
                        content_id: Some("inline-2".into()),
                        content_location: None,
                        size_bytes: 30,
                        local_path: None,
                        provider_id: "att-inline-2".into(),
                    },
                    AttachmentMeta {
                        id: AttachmentId::new(),
                        message_id: env.id.clone(),
                        filename: "report.pdf".into(),
                        mime_type: "application/pdf".into(),
                        disposition: mxr_core::types::AttachmentDisposition::Attachment,
                        content_id: None,
                        content_location: None,
                        size_bytes: 40,
                        local_path: None,
                        provider_id: "att-pdf".into(),
                    },
                ],
                fetched_at: chrono::Utc::now(),
                metadata: Default::default(),
            },
        );

        app.apply(Action::OpenSelected);
        app.apply(Action::AttachmentList);

        assert!(app.attachment_panel.visible);
        assert_eq!(
            app.attachment_panel
                .attachments
                .iter()
                .map(|attachment| attachment.filename.as_str())
                .collect::<Vec<_>>(),
            vec!["budget.xlsx", "report.pdf", "inline-1.png", "inline-2.png"]
        );
        assert_eq!(app.attachment_panel.selected_index, 0);
        assert_eq!(
            app.selected_attachment()
                .map(|attachment| attachment.filename.as_str()),
            Some("budget.xlsx")
        );
    }

    #[test]
    fn attachment_list_navigation_follows_sorted_attachment_order() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(1);
        app.all_envelopes = app.envelopes.clone();
        let env = app.envelopes[0].clone();
        app.body_cache.insert(
            env.id.clone(),
            MessageBody {
                message_id: env.id.clone(),
                text_plain: Some("hello".into()),
                text_html: Some("<img src=\"cid:inline-1\">".into()),
                attachments: vec![
                    AttachmentMeta {
                        id: AttachmentId::new(),
                        message_id: env.id.clone(),
                        filename: "inline-1.png".into(),
                        mime_type: "image/png".into(),
                        disposition: mxr_core::types::AttachmentDisposition::Inline,
                        content_id: Some("inline-1".into()),
                        content_location: None,
                        size_bytes: 10,
                        local_path: None,
                        provider_id: "att-inline-1".into(),
                    },
                    AttachmentMeta {
                        id: AttachmentId::new(),
                        message_id: env.id.clone(),
                        filename: "budget.xlsx".into(),
                        mime_type:
                            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
                                .into(),
                        disposition: mxr_core::types::AttachmentDisposition::Attachment,
                        content_id: None,
                        content_location: None,
                        size_bytes: 20,
                        local_path: None,
                        provider_id: "att-xlsx".into(),
                    },
                    AttachmentMeta {
                        id: AttachmentId::new(),
                        message_id: env.id.clone(),
                        filename: "report.pdf".into(),
                        mime_type: "application/pdf".into(),
                        disposition: mxr_core::types::AttachmentDisposition::Attachment,
                        content_id: None,
                        content_location: None,
                        size_bytes: 40,
                        local_path: None,
                        provider_id: "att-pdf".into(),
                    },
                ],
                fetched_at: chrono::Utc::now(),
                metadata: Default::default(),
            },
        );

        app.apply(Action::OpenSelected);
        app.apply(Action::AttachmentList);

        let _ = app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(
            app.selected_attachment()
                .map(|attachment| attachment.filename.as_str()),
            Some("report.pdf")
        );

        let _ = app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(
            app.selected_attachment()
                .map(|attachment| attachment.filename.as_str()),
            Some("inline-1.png")
        );

        let _ = app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
        assert_eq!(
            app.selected_attachment()
                .map(|attachment| attachment.filename.as_str()),
            Some("report.pdf")
        );
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
    fn search_preview_o_opens_in_browser() {
        let mut app = App::new();
        let results = make_test_envelopes(1);
        let env = results[0].clone();
        app.screen = Screen::Search;
        app.search_page.results = results;
        app.search_page.session_active = true;
        app.search_page.active_pane = SearchPane::Preview;
        app.viewed_thread_messages = vec![env.clone()];
        app.viewing_envelope = Some(env);

        let action = app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));

        assert_eq!(action, Some(Action::OpenInBrowser));
    }

    #[test]
    fn search_preview_r_toggles_reader_mode_without_shift_modifier() {
        let mut app = App::new();
        let results = make_test_envelopes(1);
        let env = results[0].clone();
        app.screen = Screen::Search;
        app.search_page.results = results;
        app.search_page.session_active = true;
        app.search_page.active_pane = SearchPane::Preview;
        app.viewed_thread_messages = vec![env.clone()];
        app.viewing_envelope = Some(env);

        let action = app.handle_key(KeyEvent::new(KeyCode::Char('R'), KeyModifiers::NONE));

        assert_eq!(action, Some(Action::ToggleReaderMode));
    }

    #[test]
    fn search_preview_h_and_m_toggle_html_controls_without_shift_modifier() {
        let mut app = App::new();
        let results = make_test_envelopes(1);
        let env = results[0].clone();
        app.screen = Screen::Search;
        app.search_page.results = results;
        app.search_page.session_active = true;
        app.search_page.active_pane = SearchPane::Preview;
        app.viewed_thread_messages = vec![env.clone()];
        app.viewing_envelope = Some(env);

        let html = app.handle_key(KeyEvent::new(KeyCode::Char('H'), KeyModifiers::NONE));
        let remote = app.handle_key(KeyEvent::new(KeyCode::Char('M'), KeyModifiers::NONE));

        assert_eq!(html, Some(Action::ToggleHtmlView));
        assert_eq!(remote, Some(Action::ToggleRemoteContent));
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

    #[tokio::test]
    async fn unchanged_editor_result_disables_send_actions() {
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
        .await
        .unwrap()
        .expect("pending send should exist");

        assert_eq!(pending.mode, PendingSendMode::Unchanged);

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
            mode: PendingSendMode::Unchanged,
        });

        let _ = app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));

        assert_eq!(
            app.pending_send_confirm
                .as_ref()
                .map(|pending| pending.mode),
            Some(PendingSendMode::Unchanged)
        );
        assert!(app.pending_mutation_queue.is_empty());
    }

    #[test]
    fn compose_blank_recipient_advances_to_subject_modal() {
        let mut app = App::new();
        app.all_envelopes = make_test_envelopes(1);
        app.apply(Action::Compose);

        assert!(app.compose_picker.visible);
        assert_eq!(
            app.compose_picker.mode,
            crate::ui::compose_picker::ComposePickerMode::To
        );

        let _ = app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert!(app.compose_picker.visible);
        assert_eq!(
            app.compose_picker.mode,
            crate::ui::compose_picker::ComposePickerMode::Subject
        );
    }

    #[test]
    fn compose_blank_subject_starts_new_compose_with_empty_fields() {
        let mut app = App::new();
        app.all_envelopes = make_test_envelopes(1);
        app.apply(Action::Compose);

        let _ = app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let _ = app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert_eq!(
            app.pending_compose,
            Some(super::app::ComposeAction::New {
                to: String::new(),
                subject: String::new(),
            })
        );
        assert!(!app.compose_picker.visible);
    }

    #[test]
    fn escape_closes_recipient_modal_without_starting_compose() {
        let mut app = App::new();
        app.all_envelopes = make_test_envelopes(1);
        app.apply(Action::Compose);

        let _ = app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

        assert!(!app.compose_picker.visible);
        assert!(app.pending_compose.is_none());
        assert!(app.compose_picker.pending_to.is_empty());
    }

    #[test]
    fn escape_closes_subject_modal_without_starting_compose() {
        let mut app = App::new();
        app.all_envelopes = make_test_envelopes(1);
        app.apply(Action::Compose);
        let _ = app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        let _ = app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

        assert!(!app.compose_picker.visible);
        assert!(app.pending_compose.is_none());
        assert!(app.compose_picker.pending_to.is_empty());
    }

    #[tokio::test]
    async fn blank_recipient_draft_opens_draft_only_confirmation() {
        let temp = std::env::temp_dir().join(format!(
            "mxr-compose-test-missing-to-{}-{}.md",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let content = "---\nto: \"\"\ncc: \"\"\nbcc: \"\"\nsubject: Hello\nfrom: me@example.com\nattach: []\n---\n\nBody\n";
        std::fs::write(&temp, content).unwrap();

        let pending = pending_send_from_edited_draft(&ComposeReadyData {
            draft_path: temp.clone(),
            cursor_line: 1,
            initial_content: String::new(),
        })
        .await
        .unwrap()
        .expect("pending send should exist");

        assert_eq!(pending.mode, PendingSendMode::DraftOnlyNoRecipients);

        let _ = std::fs::remove_file(temp);
    }

    #[test]
    fn send_key_is_ignored_for_missing_recipient_draft_confirmation() {
        let mut app = App::new();
        app.pending_send_confirm = Some(PendingSend {
            fm: mxr_compose::frontmatter::ComposeFrontmatter {
                to: String::new(),
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
            mode: PendingSendMode::DraftOnlyNoRecipients,
        });

        let _ = app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));

        assert_eq!(
            app.pending_send_confirm
                .as_ref()
                .map(|pending| pending.mode),
            Some(PendingSendMode::DraftOnlyNoRecipients)
        );
        assert!(app.pending_mutation_queue.is_empty());
    }

    #[test]
    fn save_key_saves_missing_recipient_draft_to_server() {
        let mut app = App::new();
        app.all_envelopes = make_test_envelopes(1);
        app.pending_send_confirm = Some(PendingSend {
            fm: mxr_compose::frontmatter::ComposeFrontmatter {
                to: String::new(),
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
            mode: PendingSendMode::DraftOnlyNoRecipients,
        });

        let _ = app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));

        assert!(app.pending_send_confirm.is_none());
        assert!(matches!(
            app.pending_mutation_queue.first(),
            Some((Request::SaveDraftToServer { .. }, _))
        ));
    }

    #[test]
    fn edit_key_reopens_missing_recipient_draft() {
        let temp = std::env::temp_dir().join(format!(
            "mxr-compose-edit-draft-{}-{}.md",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::write(&temp, "draft").unwrap();

        let mut app = App::new();
        app.pending_send_confirm = Some(PendingSend {
            fm: mxr_compose::frontmatter::ComposeFrontmatter {
                to: String::new(),
                cc: String::new(),
                bcc: String::new(),
                subject: "Hello".into(),
                from: "me@example.com".into(),
                in_reply_to: None,
                references: vec![],
                attach: vec![],
            },
            body: "Body".into(),
            draft_path: temp.clone(),
            mode: PendingSendMode::DraftOnlyNoRecipients,
        });

        let _ = app.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));

        assert!(app.pending_send_confirm.is_none());
        assert_eq!(
            app.pending_compose,
            Some(super::app::ComposeAction::EditDraft(temp.clone()))
        );

        let _ = std::fs::remove_file(temp);
    }

    #[test]
    fn escape_discards_missing_recipient_draft_confirmation_and_queues_cleanup() {
        let temp = std::env::temp_dir().join(format!(
            "mxr-compose-discard-draft-{}-{}.md",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::write(&temp, "draft").unwrap();

        let mut app = App::new();
        app.pending_send_confirm = Some(PendingSend {
            fm: mxr_compose::frontmatter::ComposeFrontmatter {
                to: String::new(),
                cc: String::new(),
                bcc: String::new(),
                subject: "Hello".into(),
                from: "me@example.com".into(),
                in_reply_to: None,
                references: vec![],
                attach: vec![],
            },
            body: "Body".into(),
            draft_path: temp.clone(),
            mode: PendingSendMode::DraftOnlyNoRecipients,
        });

        let _ = app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

        assert!(app.pending_send_confirm.is_none());
        assert!(temp.exists());
        assert_eq!(app.pending_draft_cleanup, vec![temp.clone()]);
        assert_eq!(app.status_message.as_deref(), Some("Discarded"));

        let _ = std::fs::remove_file(temp);
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

        assert!(crate::ipc::should_reconnect_ipc(&result));
    }

    #[test]
    fn autostart_detection_handles_refused_and_missing_socket() {
        let refused = std::io::Error::from(std::io::ErrorKind::ConnectionRefused);
        let missing = std::io::Error::from(std::io::ErrorKind::NotFound);
        let other = std::io::Error::from(std::io::ErrorKind::PermissionDenied);

        assert!(crate::ipc::should_autostart_daemon(&refused));
        assert!(crate::ipc::should_autostart_daemon(&missing));
        assert!(!crate::ipc::should_autostart_daemon(&other));
    }

    #[test]
    fn diagnostics_shift_l_opens_logs() {
        let mut app = App::new();
        app.screen = Screen::Diagnostics;

        let action = app.handle_key(KeyEvent::new(KeyCode::Char('L'), KeyModifiers::SHIFT));

        assert_eq!(action, Some(Action::OpenLogs));
    }

    #[test]
    fn diagnostics_uppercase_l_opens_logs_without_shift_modifier() {
        let mut app = App::new();
        app.screen = Screen::Diagnostics;

        let action = app.handle_key(KeyEvent::new(KeyCode::Char('L'), KeyModifiers::NONE));

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
