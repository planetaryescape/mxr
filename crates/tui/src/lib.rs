pub mod action;
pub mod app;
pub mod client;
pub mod input;
pub mod keybindings;
pub mod ui;

use app::{App, ComposeAction, PendingSend};
use client::Client;
use crossterm::event::{Event, EventStream};
use futures::StreamExt;
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
        let mut client = match Client::connect(&socket_path).await {
            Ok(c) => c.with_event_channel(event_tx),
            Err(_) => return,
        };

        loop {
            tokio::select! {
                req = rx.recv() => {
                    match req {
                        Some(req) => {
                            let result = client.raw_request(req.request).await;
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

    let mut app = App::new();
    app.load(&mut client).await?;
    app.apply(action::Action::GoToInbox);

    let mut terminal = ratatui::init();
    let mut events = EventStream::new();

    // Channels for async results
    let (result_tx, mut result_rx) = mpsc::unbounded_channel::<AsyncResult>();

    // Background IPC worker — also forwards daemon events to result_tx
    let bg = spawn_ipc_worker(socket_path, result_tx.clone());

    loop {
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
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::Envelopes { messages },
                    }) => Ok(messages),
                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                    Err(e) => Err(e),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                };
                let _ = tx.send(AsyncResult::LabelEnvelopes(result));
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

        // Handle thread export
        if let Some(thread_id) = app.pending_export_thread.take() {
            let bg = bg.clone();
            let tx = result_tx.clone();
            tokio::spawn(async move {
                let resp = ipc_call(&bg, Request::GetThread { thread_id }).await;
                let result = match resp {
                    Ok(Response::Ok {
                        data: ResponseData::Thread { thread, messages },
                    }) => {
                        // Format as markdown
                        let mut md = format!("# {}\n\n", thread.subject);
                        for msg in &messages {
                            md.push_str(&format!(
                                "## From: {} <{}>\n**Date:** {}\n\n",
                                msg.from.name.as_deref().unwrap_or(""),
                                msg.from.email,
                                msg.date.format("%Y-%m-%d %H:%M"),
                            ));
                            md.push_str(&format!("{}\n\n---\n\n", msg.snippet));
                        }
                        // Write to temp file
                        let sanitized: String = thread
                            .subject
                            .chars()
                            .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-')
                            .take(50)
                            .collect();
                        let filename =
                            format!("mxr-export-{}.md", sanitized.trim().replace(' ', "-"));
                        let path = std::env::temp_dir().join(&filename);
                        match std::fs::write(&path, &md) {
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
                            app.envelopes = app
                                .all_envelopes
                                .iter()
                                .filter(|e| matched_ids.contains(&e.id))
                                .cloned()
                                .collect();
                            app.selected_index = 0;
                            app.scroll_offset = 0;
                        }
                        AsyncResult::Search(Err(_)) => {
                            app.envelopes = app.all_envelopes.clone();
                        }
                        AsyncResult::LabelEnvelopes(Ok(messages)) => {
                            // Cache bodies and extract envelopes
                            for sm in &messages {
                                app.body_cache.insert(sm.envelope.id.clone(), sm.body.clone());
                            }
                            let envelopes: Vec<_> = messages.into_iter().map(|sm| sm.envelope).collect();
                            // Preserve user's position by tracking selected message ID
                            let selected_id = app.envelopes.get(app.selected_index).map(|e| e.id.clone());
                            app.envelopes = envelopes;
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
                        AsyncResult::LabelEnvelopes(Err(_)) => {
                            app.envelopes = app.all_envelopes.clone();
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
                                        app.message_body = None;
                                        app.raw_body = None;
                                        app.layout_mode = app::LayoutMode::TwoPane;
                                        app.active_pane = app::ActivePane::MailList;
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
                                    match std::fs::read_to_string(&data.draft_path) {
                                        Ok(content) => {
                                            match mxr_compose::frontmatter::parse_compose_file(
                                                &content,
                                            ) {
                                                Ok((fm, body)) => {
                                                    let issues =
                                                        mxr_compose::validate_draft(&fm, &body);
                                                    let has_errors =
                                                        issues.iter().any(|i| i.is_error());
                                                    if has_errors {
                                                        let msgs: Vec<String> = issues
                                                            .iter()
                                                            .map(|i| i.to_string())
                                                            .collect();
                                                        app.status_message = Some(format!(
                                                            "Draft errors: {}",
                                                            msgs.join("; ")
                                                        ));
                                                    } else {
                                                        // Store parsed draft for confirmation
                                                        app.pending_send_confirm = Some(PendingSend {
                                                            fm,
                                                            body,
                                                            draft_path: data.draft_path.clone(),
                                                        });
                                                        app.status_message = Some(
                                                            "[s]end  [d]raft  [e]dit again  [Esc] discard".into(),
                                                        );
                                                    }
                                                }
                                                Err(e) => {
                                                    app.status_message =
                                                        Some(format!("Parse error: {e}"));
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            app.status_message =
                                                Some(format!("Failed to read draft: {e}"));
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
    LabelEnvelopes(Result<Vec<mxr_core::types::SyncedMessage>, MxrError>),
    MutationResult(Result<app::MutationEffect, MxrError>),
    ComposeReady(Result<ComposeReadyData, MxrError>),
    ExportResult(Result<String, MxrError>),
    DaemonEvent(DaemonEvent),
}

struct ComposeReadyData {
    draft_path: std::path::PathBuf,
    cursor_line: usize,
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
                draft_path: path,
                cursor_line,
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
        draft_path: path,
        cursor_line,
    })
}

async fn get_account_email(bg: &mpsc::UnboundedSender<IpcRequest>) -> Result<String, MxrError> {
    let resp = ipc_call(bg, Request::GetStatus).await?;
    match resp {
        Response::Ok {
            data: ResponseData::Status { accounts, .. },
        } => Ok(accounts
            .into_iter()
            .next()
            .unwrap_or_else(|| "user@example.com".to_string())),
        _ => Ok("user@example.com".to_string()),
    }
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
    use super::app::{ActivePane, App, LayoutMode, MutationEffect};
    use super::input::InputHandler;
    use super::ui::command_palette::CommandPalette;
    use super::ui::search_bar::SearchBar;
    use super::ui::status_bar;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use mxr_core::id::*;
    use mxr_core::types::*;

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
    fn back_in_message_view_returns_to_mail_list() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(3);
        app.all_envelopes = app.envelopes.clone();
        app.apply(Action::OpenSelected);
        assert_eq!(app.active_pane, ActivePane::MessageView);
        assert_eq!(app.layout_mode, LayoutMode::ThreePane);
        // Esc should move to MailList but keep ThreePane
        app.apply(Action::Back);
        assert_eq!(app.active_pane, ActivePane::MailList);
        assert_eq!(
            app.layout_mode,
            LayoutMode::ThreePane,
            "Esc in MessageView should keep ThreePane"
        );
        assert!(
            app.viewing_envelope.is_some(),
            "Message should still be visible"
        );
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
        assert!(
            app.active_label.is_some(),
            "GoToInbox should set active_label"
        );
        let label = app.labels.iter().find(|l| l.name == "INBOX").unwrap();
        assert_eq!(app.active_label.as_ref().unwrap(), &label.id);
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
        assert!(
            title.contains("Messages"),
            "Default title should say Messages"
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
        app.apply(Action::OpenMessageView);
        assert_eq!(app.layout_mode, LayoutMode::ThreePane);
        app.message_body = Some("Hello".into());
        assert_eq!(app.message_body.as_deref(), Some("Hello"));
        app.apply(Action::CloseMessageView);
        assert!(app.message_body.is_none());
    }
}
