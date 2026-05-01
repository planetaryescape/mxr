use crate::app::{App, ComposeAction, PendingSend, PendingSendMode};
use crate::async_result::ComposeReadyData;
use crate::ipc::{ipc_call, IpcRequest};
use mxr_core::AccountId;
use mxr_core::MxrError;
use mxr_protocol::{AccountSummaryData, Request, Response, ResponseData};
use tokio::sync::mpsc;

pub(crate) async fn handle_compose_action(
    bg: &mpsc::UnboundedSender<IpcRequest>,
    action: ComposeAction,
) -> Result<ComposeReadyData, MxrError> {
    let (account_id, from, kind) = match action {
        ComposeAction::EditDraft { path, account_id } => {
            // Re-edit existing draft — skip creating a new file
            let cursor_line = 1;
            return Ok(ComposeReadyData {
                account_id,
                draft_path: path.clone(),
                cursor_line,
                initial_content: mxr_compose::read_draft_file_async(&path)
                    .await
                    .map_err(|e| MxrError::Ipc(e.to_string()))?,
            });
        }
        ComposeAction::New { to, subject } => {
            let account = resolve_compose_account(bg, None).await?;
            (
                account.account_id,
                account.email,
                mxr_compose::ComposeKind::New { to, subject },
            )
        }
        ComposeAction::Reply {
            message_id,
            account_id,
        } => {
            let account = resolve_compose_account(bg, Some(&account_id)).await?;
            let resp = ipc_call(
                bg,
                Request::PrepareReply {
                    message_id,
                    reply_all: false,
                },
            )
            .await?;
            let kind = match resp {
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
            };
            (account_id, account.email, kind)
        }
        ComposeAction::ReplyAll {
            message_id,
            account_id,
        } => {
            let account = resolve_compose_account(bg, Some(&account_id)).await?;
            let resp = ipc_call(
                bg,
                Request::PrepareReply {
                    message_id,
                    reply_all: true,
                },
            )
            .await?;
            let kind = match resp {
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
            };
            (account_id, account.email, kind)
        }
        ComposeAction::Forward {
            message_id,
            account_id,
        } => {
            let account = resolve_compose_account(bg, Some(&account_id)).await?;
            let resp = ipc_call(bg, Request::PrepareForward { message_id }).await?;
            let kind = match resp {
                Response::Ok {
                    data: ResponseData::ForwardContext { context },
                } => mxr_compose::ComposeKind::Forward {
                    subject: context.subject,
                    original_context: context.forwarded_content,
                },
                Response::Error { message } => return Err(MxrError::Ipc(message)),
                _ => return Err(MxrError::Ipc("unexpected response".into())),
            };
            (account_id, account.email, kind)
        }
    };

    let (path, cursor_line) = mxr_compose::create_draft_file_async(kind, &from)
        .await
        .map_err(|e| MxrError::Ipc(e.to_string()))?;

    Ok(ComposeReadyData {
        account_id,
        draft_path: path.clone(),
        cursor_line,
        initial_content: mxr_compose::read_draft_file_async(&path)
            .await
            .map_err(|e| MxrError::Ipc(e.to_string()))?,
    })
}

pub(crate) async fn resolve_compose_account(
    bg: &mpsc::UnboundedSender<IpcRequest>,
    account_id: Option<&AccountId>,
) -> Result<AccountSummaryData, MxrError> {
    let resp = ipc_call(bg, Request::ListAccounts).await?;
    match resp {
        Response::Ok {
            data: ResponseData::Accounts { mut accounts },
        } => {
            if let Some(account_id) = account_id {
                return accounts
                    .into_iter()
                    .find(|account| &account.account_id == account_id)
                    .ok_or_else(|| {
                        MxrError::Ipc(format!("Compose account not found: {account_id}"))
                    });
            }
            if let Some(index) = accounts.iter().position(|account| account.is_default) {
                Ok(accounts.remove(index))
            } else {
                accounts
                    .into_iter()
                    .next()
                    .map(|account| account)
                    .ok_or_else(|| MxrError::Ipc("No runtime account configured".into()))
            }
        }
        Response::Error { message } => Err(MxrError::Ipc(message)),
        _ => Err(MxrError::Ipc("Unexpected account response".into())),
    }
}

pub(crate) async fn pending_send_from_edited_draft(
    data: &ComposeReadyData,
) -> Result<Option<PendingSend>, String> {
    let content = mxr_compose::read_draft_file_async(&data.draft_path)
        .await
        .map_err(|e| format!("Failed to read draft: {e}"))?;
    let unchanged = content == data.initial_content;

    let (fm, body) = mxr_compose::frontmatter::parse_compose_file(&content)
        .map_err(|e| format!("Parse error: {e}"))?;
    let save_issues = mxr_compose::validate_draft_for_save(&fm, &body);
    if save_issues.iter().any(|issue| issue.is_error()) {
        let msgs: Vec<String> = save_issues.iter().map(|issue| issue.to_string()).collect();
        return Err(format!("Draft errors: {}", msgs.join("; ")));
    }

    let send_issues = mxr_compose::validate_draft(&fm, &body);
    let mode = if unchanged {
        PendingSendMode::Unchanged
    } else if send_issues.iter().all(|issue| !issue.is_error()) {
        PendingSendMode::SendOrSave
    } else if send_issues
        .iter()
        .all(|issue| !issue.is_error() || issue.is_missing_recipients())
    {
        PendingSendMode::DraftOnlyNoRecipients
    } else {
        let msgs: Vec<String> = send_issues.iter().map(|issue| issue.to_string()).collect();
        return Err(format!("Draft errors: {}", msgs.join("; ")));
    };

    Ok(Some(PendingSend {
        account_id: data.account_id.clone(),
        fm,
        body,
        draft_path: data.draft_path.clone(),
        mode,
    }))
}

pub(crate) async fn handle_compose_editor_status(
    app: &mut App,
    data: &ComposeReadyData,
    status: std::io::Result<std::process::ExitStatus>,
) {
    match status {
        Ok(s) if s.success() => match pending_send_from_edited_draft(data).await {
            Ok(Some(pending)) => {
                app.pending_send_confirm = Some(pending);
            }
            Ok(None) => {}
            Err(message) => {
                app.status_message = Some(message);
            }
        },
        Ok(_) => {
            app.status_message = Some("Draft discarded".into());
            let _ = mxr_compose::delete_draft_file_async(&data.draft_path).await;
        }
        Err(error) => {
            app.status_message = Some(format!("Failed to launch editor: {error}"));
        }
    }
}
