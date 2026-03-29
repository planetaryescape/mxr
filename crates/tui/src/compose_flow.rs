use mxr_core::MxrError;
use mxr_protocol::{Request, Response, ResponseData};
use crate::app::{ComposeAction, PendingSend};
use crate::async_result::ComposeReadyData;
use crate::ipc::{ipc_call, IpcRequest};
use tokio::sync::mpsc;

pub(crate) async fn handle_compose_action(
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

    let (path, cursor_line) = mxr_compose::create_draft_file(kind, &from)
        .map_err(|e| MxrError::Ipc(e.to_string()))?;

    Ok(ComposeReadyData {
        draft_path: path.clone(),
        cursor_line,
        initial_content: std::fs::read_to_string(&path)
            .map_err(|e| MxrError::Ipc(e.to_string()))?,
    })
}

pub(crate) async fn get_account_email(bg: &mpsc::UnboundedSender<IpcRequest>) -> Result<String, MxrError> {
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

pub(crate) fn pending_send_from_edited_draft(data: &ComposeReadyData) -> Result<Option<PendingSend>, String> {
    let content = std::fs::read_to_string(&data.draft_path)
        .map_err(|e| format!("Failed to read draft: {e}"))?;
    let unchanged = content == data.initial_content;

    let (fm, body) = mxr_compose::frontmatter::parse_compose_file(&content)
        .map_err(|e| format!("Parse error: {e}"))?;
    let issues = mxr_compose::validate_draft(&fm, &body);
    let has_errors = issues.iter().any(mxr_compose::ComposeValidation::is_error);
    if has_errors {
        let msgs: Vec<String> = issues.iter().map(ToString::to_string).collect();
        return Err(format!("Draft errors: {}", msgs.join("; ")));
    }

    Ok(Some(PendingSend {
        fm,
        body,
        draft_path: data.draft_path.clone(),
        allow_send: !unchanged,
    }))
}
