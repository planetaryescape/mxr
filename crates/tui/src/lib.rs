pub mod action;
pub mod app;
pub mod client;
pub mod input;
pub mod keybindings;
pub mod ui;

use app::{App, AttachmentOperation, ComposeAction, PendingSend};
use client::Client;
use crossterm::event::{Event, EventStream};
use futures::StreamExt;
use mxr_config::load_config;
use mxr_core::MxrError;
use mxr_protocol::{DaemonEvent, Request, Response, ResponseData, SearchResultItem};
use tokio::sync::{mpsc, oneshot};

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
    Client::connect(socket_path)
        .await
        .map(|client| client.with_event_channel(event_tx))
        .map_err(|e| MxrError::Ipc(e.to_string()))
}

fn should_reconnect_ipc(result: &Result<Response, MxrError>) -> bool {
    match result {
        Err(MxrError::Ipc(message)) => {
            let lower = message.to_lowercase();
            lower.contains("broken pipe") || lower.contains("connection closed")
        }
        _ => false,
    }
}

fn request_supports_retry(request: &Request) -> bool {
    matches!(
        request,
        Request::ListEnvelopes { .. }
            | Request::GetEnvelope { .. }
            | Request::GetBody { .. }
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

pub async fn run() -> anyhow::Result<()> {
    let socket_path = daemon_socket_path();
    let mut client = Client::connect(&socket_path).await?;
    let config = load_config()?;

    let mut app = App::from_config(&config);
    app.load(&mut client).await?;
    app.apply(action::Action::GoToInbox);

    let mut terminal = ratatui::init();
    let mut events = EventStream::new();

    // Channels for async results
    let (result_tx, mut result_rx) = mpsc::unbounded_channel::<AsyncResult>();

    // Background IPC worker — also forwards daemon events to result_tx
    let bg = spawn_ipc_worker(socket_path, result_tx.clone());

    loop {
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

        // Spawn non-blocking search
        if let Some(query) = app.pending_search.take() {
            let bg = bg.clone();
            let tx = result_tx.clone();
            tokio::spawn(async move {
                let resp = ipc_call(&bg, Request::Search { query, limit: 200 }).await;
                let results = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::SearchResults { results },
                    }) => Ok(results),
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                let _ = tx.send(AsyncResult::Search(results));
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
            let priority = app
                .rules_page
                .form
                .priority
                .parse::<i32>()
                .unwrap_or(100);
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

        if app.accounts_page.refresh_pending {
            app.accounts_page.refresh_pending = false;
            let bg = bg.clone();
            let tx = result_tx.clone();
            tokio::spawn(async move {
                let resp = ipc_call(&bg, Request::ListAccounts).await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::Accounts { accounts },
                    }) => Ok(accounts),
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                let _ = tx.send(AsyncResult::Accounts(result));
            });
        }

        if let Some(account) = app.pending_account_save.take() {
            let bg = bg.clone();
            let tx = result_tx.clone();
            tokio::spawn(async move {
                let resp = ipc_call(&bg, Request::UpsertAccountConfig { account }).await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::AccountStatus { message },
                    }) => Ok(message),
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                let _ = tx.send(AsyncResult::AccountOperation(result));
            });
        }

        if let Some(account) = app.pending_account_test.take() {
            let bg = bg.clone();
            let tx = result_tx.clone();
            tokio::spawn(async move {
                let resp = ipc_call(&bg, Request::TestAccountConfig { account }).await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::AccountStatus { message },
                    }) => Ok(message),
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                let _ = tx.send(AsyncResult::AccountOperation(result));
            });
        }

        if let Some(key) = app.pending_account_set_default.take() {
            let bg = bg.clone();
            let tx = result_tx.clone();
            tokio::spawn(async move {
                let resp = ipc_call(&bg, Request::SetDefaultAccount { key }).await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::AccountStatus { message },
                    }) => Ok(message),
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
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
                        AsyncResult::Search(Ok(results)) => {
                            let matched_ids: std::collections::HashSet<_> =
                                results.iter().map(|r| r.message_id.clone()).collect();
                            let matched: Vec<_> = app
                                .all_envelopes
                                .iter()
                                .filter(|e| matched_ids.contains(&e.id))
                                .cloned()
                                .collect();
                            if app.screen == app::Screen::Search {
                                app.search_page.results = matched;
                                app.search_page.selected_index = 0;
                                app.search_page.scroll_offset = 0;
                                app.auto_preview_search();
                            } else {
                                app.envelopes = matched;
                                app.selected_index = 0;
                                app.scroll_offset = 0;
                            }
                        }
                        AsyncResult::Search(Err(_)) => {
                            if app.screen == app::Screen::Search {
                                app.search_page.results.clear();
                            } else {
                                app.envelopes = app.all_envelopes.clone();
                            }
                        }
                        AsyncResult::Rules(Ok(rules)) => {
                            app.rules_page.rules = rules;
                            app.rules_page.selected_index = app
                                .rules_page
                                .selected_index
                                .min(app.rules_page.rules.len().saturating_sub(1));
                            if let Some(rule_id) = app
                                .selected_rule()
                                .and_then(|rule| rule["id"].as_str())
                                .map(ToString::to_string)
                            {
                                app.pending_rule_detail = Some(rule_id);
                            }
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
                            match *result {
                                Ok(response) => match response {
                                Response::Ok {
                                    data:
                                        ResponseData::Status {
                                            uptime_secs,
                                            accounts,
                                            total_messages,
                                        },
                                } => {
                                    app.diagnostics_page.uptime_secs = Some(uptime_secs);
                                    app.diagnostics_page.accounts = accounts;
                                    app.diagnostics_page.total_messages = Some(total_messages);
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
                        AsyncResult::Accounts(Ok(accounts)) => {
                            app.accounts_page.accounts = accounts;
                            app.accounts_page.selected_index = app
                                .accounts_page
                                .selected_index
                                .min(app.accounts_page.accounts.len().saturating_sub(1));
                        }
                        AsyncResult::Accounts(Err(e)) => {
                            app.accounts_page.status = Some(format!("Accounts error: {e}"));
                        }
                        AsyncResult::AccountOperation(Ok(message)) => {
                            app.accounts_page.status = Some(message);
                            app.accounts_page.form.visible = false;
                            app.accounts_page.refresh_pending = true;
                        }
                        AsyncResult::AccountOperation(Err(e)) => {
                            app.accounts_page.status = Some(format!("Account error: {e}"));
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
                            // Preserve user's position by tracking selected message ID
                            let selected_id = app.envelopes.get(app.selected_index).map(|e| e.id.clone());
                            app.envelopes = envelopes;
                            app.active_label = app.pending_active_label.take();
                            app.queue_body_window();
                            if let Some(ref id) = selected_id {
                                if let Some(pos) = app.envelopes.iter().position(|e| &e.id == id) {
                                    app.selected_index = pos;
                                    // Keep scroll offset valid
                                    if app.selected_index < app.scroll_offset {
                                        app.scroll_offset = app.selected_index;
                                    } else if app.selected_index >= app.scroll_offset + app.visible_height.max(1) {
                                        app.scroll_offset = app.selected_index + 1 - app.visible_height.max(1);
                                    }
                                } else {
                                    // Selected message no longer in list (deleted/moved)
                                    app.selected_index = app.selected_index.min(app.envelopes.len().saturating_sub(1));
                                }
                            } else {
                                app.selected_index = 0;
                                app.scroll_offset = 0;
                            }
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
                            match effect {
                                app::MutationEffect::RemoveFromList(id) => {
                                    app.envelopes.retain(|e| e.id != id);
                                    app.all_envelopes.retain(|e| e.id != id);
                                    if app.selected_index >= app.envelopes.len()
                                        && app.selected_index > 0
                                    {
                                        app.selected_index -= 1;
                                    }
                                    // Close message view if viewing the removed message
                                    if app.viewing_envelope.as_ref().map(|e| &e.id)
                                        == Some(&id)
                                    {
                                        app.viewing_envelope = None;
                                        app.body_view_state = app::BodyViewState::Empty { preview: None };
                                        app.layout_mode = app::LayoutMode::TwoPane;
                                        app.active_pane = app::ActivePane::MailList;
                                    }
                                    app.status_message = Some("Done".into());
                                }
                                app::MutationEffect::RemoveFromListMany(ids) => {
                                    app.envelopes.retain(|e| !ids.iter().any(|id| id == &e.id));
                                    app.all_envelopes
                                        .retain(|e| !ids.iter().any(|id| id == &e.id));
                                    if let Some(viewing_id) =
                                        app.viewing_envelope.as_ref().map(|e| e.id.clone())
                                    {
                                        if ids.iter().any(|id| id == &viewing_id) {
                                            app.viewing_envelope = None;
                                            app.body_view_state =
                                                app::BodyViewState::Empty { preview: None };
                                            app.layout_mode = app::LayoutMode::TwoPane;
                                            app.active_pane = app::ActivePane::MailList;
                                        }
                                    }
                                    if app.selected_index >= app.envelopes.len()
                                        && app.selected_index > 0
                                    {
                                        app.selected_index = app.envelopes.len() - 1;
                                    }
                                    app.status_message = Some("Done".into());
                                }
                                app::MutationEffect::UpdateFlags { message_id, flags } => {
                                    for e in app
                                        .envelopes
                                        .iter_mut()
                                        .chain(app.all_envelopes.iter_mut())
                                    {
                                        if e.id == message_id {
                                            e.flags = flags;
                                        }
                                    }
                                    if let Some(ref mut ve) = app.viewing_envelope {
                                        if ve.id == message_id {
                                            ve.flags = flags;
                                        }
                                    }
                                    app.status_message = Some("Done".into());
                                }
                                app::MutationEffect::RefreshList => {
                                    if let Some(label_id) = app.active_label.clone() {
                                        app.pending_label_fetch = Some(label_id);
                                    }
                                    app.status_message = Some("Synced".into());
                                }
                                app::MutationEffect::ModifyLabels {
                                    message_ids,
                                    add,
                                    remove,
                                    status,
                                } => {
                                    app.apply_local_label_refs(&message_ids, &add, &remove);
                                    app.status_message = Some(status);
                                }
                                app::MutationEffect::StatusOnly(msg) => {
                                    app.status_message = Some(msg);
                                }
                            }
                        }
                        AsyncResult::MutationResult(Err(e)) => {
                            app.status_message = Some(format!("Error: {e}"));
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
                        AsyncResult::DaemonEvent(event) => {
                            match event {
                                DaemonEvent::SyncCompleted { messages_synced, .. } => {
                                    // Re-fetch current view's envelopes
                                    if let Some(label_id) = app.active_label.clone() {
                                        app.pending_label_fetch = Some(label_id);
                                    }
                                    // Label counts are updated in-place via
                                    // LabelCountsUpdated events — no need to re-fetch
                                    // the full label list here.
                                    if messages_synced > 0 {
                                        app.status_message = Some(format!(
                                            "Synced {messages_synced} messages"
                                        ));
                                    }
                                }
                                DaemonEvent::LabelCountsUpdated { counts } => {
                                    // Update label counts in-place without re-fetching
                                    for count in &counts {
                                        if let Some(label) = app.labels.iter_mut().find(
                                            |l| l.id == count.label_id,
                                        ) {
                                            label.unread_count = count.unread_count;
                                            label.total_count = count.total_count;
                                        }
                                    }
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

enum AsyncResult {
    Search(Result<Vec<SearchResultItem>, MxrError>),
    Rules(Result<Vec<serde_json::Value>, MxrError>),
    RuleDetail(Result<serde_json::Value, MxrError>),
    RuleHistory(Result<Vec<serde_json::Value>, MxrError>),
    RuleDryRun(Result<Vec<serde_json::Value>, MxrError>),
    RuleForm(Result<mxr_protocol::RuleFormData, MxrError>),
    RuleDeleted(Result<(), MxrError>),
    RuleUpsert(Result<serde_json::Value, MxrError>),
    Diagnostics(Box<Result<Response, MxrError>>),
    Accounts(Result<Vec<mxr_protocol::AccountSummaryData>, MxrError>),
    AccountOperation(Result<String, MxrError>),
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
    Thread {
        thread_id: mxr_core::ThreadId,
        result: Result<(mxr_core::Thread, Vec<mxr_core::Envelope>), MxrError>,
    },
    MutationResult(Result<app::MutationEffect, MxrError>),
    ComposeReady(Result<ComposeReadyData, MxrError>),
    ExportResult(Result<String, MxrError>),
    DaemonEvent(DaemonEvent),
}

struct ComposeReadyData {
    draft_path: std::path::PathBuf,
    cursor_line: usize,
    initial_content: String,
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
        initial_content: std::fs::read_to_string(&path).map_err(|e| MxrError::Ipc(e.to_string()))?,
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

fn pending_send_from_edited_draft(
    data: &ComposeReadyData,
) -> Result<Option<PendingSend>, String> {
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
    if cfg!(target_os = "macos") {
        dirs::home_dir()
            .unwrap()
            .join("Library/Application Support/mxr/mxr.sock")
    } else {
        dirs::runtime_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
            .join("mxr/mxr.sock")
    }
}

#[cfg(test)]
mod tests {
    use super::action::Action;
    use super::{pending_send_from_edited_draft, ComposeReadyData, PendingSend};
    use super::app::{ActivePane, App, BodySource, BodyViewState, LayoutMode, MutationEffect, Screen};
    use super::input::InputHandler;
    use super::ui::command_palette::CommandPalette;
    use super::ui::command_palette::default_commands;
    use super::ui::search_bar::SearchBar;
    use super::ui::status_bar;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use mxr_config::RenderConfig;
    use mxr_core::id::*;
    use mxr_core::types::*;
    use mxr_protocol::Request;

    fn make_test_envelopes(count: usize) -> Vec<Envelope> {
        (0..count)
            .map(|i| Envelope {
                id: MessageId::new(),
                account_id: AccountId::new(),
                provider_id: format!("fake-{}", i),
                thread_id: ThreadId::new(),
                message_id_header: None,
                in_reply_to: None,
                references: vec![],
                from: Address {
                    name: Some(format!("User {}", i)),
                    email: format!("user{}@example.com", i),
                },
                to: vec![],
                cc: vec![],
                bcc: vec![],
                subject: format!("Subject {}", i),
                date: chrono::Utc::now(),
                flags: if i % 2 == 0 {
                    MessageFlags::READ
                } else {
                    MessageFlags::empty()
                },
                snippet: format!("Snippet {}", i),
                has_attachments: false,
                size_bytes: 1000,
                unsubscribe: UnsubscribeMethod::None,
                label_provider_ids: vec![],
            })
            .collect()
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
        p.toggle();
        assert!(p.visible);
        p.toggle();
        assert!(!p.visible);
    }

    #[test]
    fn command_palette_fuzzy_filter() {
        let mut p = CommandPalette::default();
        p.toggle();
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
            status_bar::format_sync_status(12, Some("2m ago")),
            "[INBOX] 12 unread | synced 2m ago"
        );
        assert_eq!(
            status_bar::format_sync_status(0, None),
            "[INBOX] 0 unread | not synced"
        );
    }

    fn make_test_labels() -> Vec<Label> {
        vec![
            Label {
                id: LabelId::from_provider_id("test", "INBOX"),
                account_id: AccountId::new(),
                name: "INBOX".to_string(),
                kind: LabelKind::System,
                color: None,
                provider_id: "INBOX".to_string(),
                unread_count: 3,
                total_count: 10,
            },
            Label {
                id: LabelId::from_provider_id("test", "STARRED"),
                account_id: AccountId::new(),
                name: "STARRED".to_string(),
                kind: LabelKind::System,
                color: None,
                provider_id: "STARRED".to_string(),
                unread_count: 0,
                total_count: 2,
            },
            Label {
                id: LabelId::from_provider_id("test", "SENT"),
                account_id: AccountId::new(),
                name: "SENT".to_string(),
                kind: LabelKind::System,
                color: None,
                provider_id: "SENT".to_string(),
                unread_count: 0,
                total_count: 5,
            },
            Label {
                id: LabelId::from_provider_id("test", "DRAFT"),
                account_id: AccountId::new(),
                name: "DRAFT".to_string(),
                kind: LabelKind::System,
                color: None,
                provider_id: "DRAFT".to_string(),
                unread_count: 0,
                total_count: 0,
            },
            Label {
                id: LabelId::from_provider_id("test", "SPAM"),
                account_id: AccountId::new(),
                name: "SPAM".to_string(),
                kind: LabelKind::System,
                color: None,
                provider_id: "SPAM".to_string(),
                unread_count: 0,
                total_count: 0,
            },
            Label {
                id: LabelId::from_provider_id("test", "TRASH"),
                account_id: AccountId::new(),
                name: "TRASH".to_string(),
                kind: LabelKind::System,
                color: None,
                provider_id: "TRASH".to_string(),
                unread_count: 0,
                total_count: 0,
            },
            // Hidden system labels
            Label {
                id: LabelId::from_provider_id("test", "CHAT"),
                account_id: AccountId::new(),
                name: "CHAT".to_string(),
                kind: LabelKind::System,
                color: None,
                provider_id: "CHAT".to_string(),
                unread_count: 0,
                total_count: 0,
            },
            Label {
                id: LabelId::from_provider_id("test", "IMPORTANT"),
                account_id: AccountId::new(),
                name: "IMPORTANT".to_string(),
                kind: LabelKind::System,
                color: None,
                provider_id: "IMPORTANT".to_string(),
                unread_count: 0,
                total_count: 5,
            },
            // User labels
            Label {
                id: LabelId::from_provider_id("test", "Work"),
                account_id: AccountId::new(),
                name: "Work".to_string(),
                kind: LabelKind::User,
                color: None,
                provider_id: "Label_1".to_string(),
                unread_count: 2,
                total_count: 10,
            },
            Label {
                id: LabelId::from_provider_id("test", "Personal"),
                account_id: AccountId::new(),
                name: "Personal".to_string(),
                kind: LabelKind::User,
                color: None,
                provider_id: "Label_2".to_string(),
                unread_count: 0,
                total_count: 3,
            },
            // Hidden Gmail category
            Label {
                id: LabelId::from_provider_id("test", "CATEGORY_UPDATES"),
                account_id: AccountId::new(),
                name: "CATEGORY_UPDATES".to_string(),
                kind: LabelKind::System,
                color: None,
                provider_id: "CATEGORY_UPDATES".to_string(),
                unread_count: 0,
                total_count: 50,
            },
        ]
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
        let _id = app.envelopes[2].id.clone();
        // Simulate archive action
        app.apply(Action::Archive);
        // The mutation queue should have the request
        assert!(!app.pending_mutation_queue.is_empty());
        // Simulate the mutation result
        let (_, effect) = app.pending_mutation_queue.remove(0);
        // Apply the effect as if it succeeded
        match effect {
            MutationEffect::RemoveFromList(remove_id) => {
                app.envelopes.retain(|e| e.id != remove_id);
                app.all_envelopes.retain(|e| e.id != remove_id);
            }
            _ => panic!("Expected RemoveFromList"),
        }
        assert_eq!(app.envelopes.len(), 4);
    }

    #[test]
    fn star_updates_flags_in_place() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(3);
        app.all_envelopes = app.envelopes.clone();
        // First envelope is READ (even index), not starred
        assert!(!app.envelopes[0].flags.contains(MessageFlags::STARRED));
        app.apply(Action::Star);
        assert!(!app.pending_mutation_queue.is_empty());
        let (_, effect) = app.pending_mutation_queue.remove(0);
        match effect {
            MutationEffect::UpdateFlags { message_id, flags } => {
                assert!(flags.contains(MessageFlags::STARRED));
                // Apply effect
                for e in app.envelopes.iter_mut() {
                    if e.id == message_id {
                        e.flags = flags;
                    }
                }
            }
            _ => panic!("Expected UpdateFlags"),
        }
        assert!(app.envelopes[0].flags.contains(MessageFlags::STARRED));
    }

    #[test]
    fn archive_viewing_message_effect() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(3);
        app.all_envelopes = app.envelopes.clone();
        // Open first message
        app.apply(Action::OpenSelected);
        assert!(app.viewing_envelope.is_some());
        let viewing_id = app.viewing_envelope.as_ref().unwrap().id.clone();
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

    // --- Mail list title tests ---

    #[test]
    fn mail_list_title_shows_message_count() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(5);
        app.all_envelopes = app.envelopes.clone();
        let title = app.mail_list_title();
        assert!(title.contains("5"), "Title should show message count");
        assert!(title.contains("Threads"), "Default title should say Threads");
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
        assert_eq!(app.selected_mail_row().map(|row| row.message_count), Some(2));
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
    fn help_action_toggles_modal_state() {
        let mut app = App::new();

        app.apply(Action::Help);
        assert!(app.help_modal_open);

        app.apply(Action::Help);
        assert!(!app.help_modal_open);
    }

    #[test]
    fn open_search_screen_activates_dedicated_search_workspace() {
        let mut app = App::new();
        app.apply(Action::OpenSearchScreen);
        assert_eq!(app.screen, Screen::Search);
        assert!(app.search_page.editing);
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
            crate::app::AccountFormMode::ImapSmtp
        );
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
            sort: SortOrder::DateDesc,
            icon: None,
            position: 0,
            created_at: chrono::Utc::now(),
        }];
        app.active_pane = ActivePane::Sidebar;

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
    fn opening_search_result_enters_mailbox_thread_view() {
        let mut app = App::new();
        app.screen = Screen::Search;
        app.search_page.results = make_test_envelopes(2);
        app.search_page.selected_index = 1;

        app.apply(Action::OpenSelected);

        assert_eq!(app.screen, Screen::Mailbox);
        assert_eq!(app.layout_mode, LayoutMode::ThreePane);
        assert_eq!(app.active_pane, ActivePane::MessageView);
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
                    size_bytes: 1024,
                    local_path: None,
                    provider_id: "att-1".into(),
                }],
                fetched_at: chrono::Utc::now(),
            },
        );

        app.apply(Action::OpenSelected);
        app.apply(Action::AttachmentList);

        assert!(app.attachment_panel.visible);
        assert_eq!(app.attachment_panel.attachments.len(), 1);
        assert_eq!(app.attachment_panel.attachments[0].filename, "report.pdf");
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
                attach: vec![],
            },
            body: "Body".into(),
            draft_path: std::path::PathBuf::from("/tmp/draft.md"),
            allow_send: false,
        });

        let _ = app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));

        assert!(app.pending_send_confirm.is_some());
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
        assert!(app.pending_bulk_confirm.is_some());
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
        let labels: Vec<String> = default_commands().into_iter().map(|cmd| cmd.label).collect();
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

        assert!(app.viewing_envelope.as_ref().unwrap().label_provider_ids.contains(&user_label.provider_id));
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
            Request::Snooze { message_id, wake_at } => {
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
            },
        );

        app.apply(Action::OpenSelected);

        assert!(matches!(
            app.body_view_state,
            BodyViewState::Ready {
                ref raw,
                ref rendered,
                source: BodySource::Plain,
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
            },
        );

        app.apply(Action::OpenSelected);

        assert!(matches!(
            app.body_view_state,
            BodyViewState::Ready {
                ref raw,
                ref rendered,
                source: BodySource::Html,
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
        });

        assert_eq!(app.viewing_envelope.as_ref().map(|env| env.id.clone()), Some(second.id));
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
}
