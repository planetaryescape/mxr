use crate::app::{App, ComposeAction, PendingSend, PendingSendMode};
use crate::async_result::ComposeReadyData;
use crate::ipc::{ipc_call, IpcRequest};
use mxr_core::AccountId;
use mxr_core::MxrError;
use mxr_protocol::{AccountSummaryData, Request, Response, ResponseData, SignatureContextData};
use tokio::sync::mpsc;

pub(crate) async fn handle_compose_action(
    bg: &mpsc::UnboundedSender<IpcRequest>,
    action: ComposeAction,
) -> Result<ComposeReadyData, MxrError> {
    let (account_id, from, kind, signature_kind) = match action {
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
                SignatureContextData::New,
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
                    thread_id: context.thread_id,
                    to: context.reply_to,
                    cc: context.cc,
                    subject: context.subject,
                    thread_context: context.thread_context,
                },
                Response::Error { message, .. } => return Err(MxrError::Ipc(message)),
                _ => return Err(MxrError::Ipc("unexpected response".into())),
            };
            (account_id, account.email, kind, SignatureContextData::Reply)
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
                    thread_id: context.thread_id,
                    to: context.reply_to,
                    cc: context.cc,
                    subject: context.subject,
                    thread_context: context.thread_context,
                },
                Response::Error { message, .. } => return Err(MxrError::Ipc(message)),
                _ => return Err(MxrError::Ipc("unexpected response".into())),
            };
            (account_id, account.email, kind, SignatureContextData::Reply)
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
                Response::Error { message, .. } => return Err(MxrError::Ipc(message)),
                _ => return Err(MxrError::Ipc("unexpected response".into())),
            };
            (account_id, account.email, kind, SignatureContextData::Reply)
        }
    };

    let signature = resolve_default_signature(bg, &account_id, &from, signature_kind).await?;
    let (path, cursor_line) =
        mxr_compose::create_draft_file_async_with_signature(kind, &from, signature.as_ref())
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
                let account = accounts
                    .into_iter()
                    .find(|account| &account.account_id == account_id)
                    .ok_or_else(|| {
                        MxrError::Ipc(format!("Compose account not found: {account_id}"))
                    })?;
                if !compose_account_eligible(&account) {
                    return Err(MxrError::Ipc(format!(
                        "Compose account is not enabled for sending: {}",
                        account.email
                    )));
                }
                return Ok(account);
            }
            accounts.retain(compose_account_eligible);
            if let Some(index) = accounts.iter().position(|account| account.is_default) {
                Ok(accounts.remove(index))
            } else {
                accounts
                    .into_iter()
                    .next()
                    .ok_or_else(|| MxrError::Ipc("No enabled send account configured".into()))
            }
        }
        Response::Error { message, .. } => Err(MxrError::Ipc(message)),
        _ => Err(MxrError::Ipc("Unexpected account response".into())),
    }
}

fn compose_account_eligible(account: &AccountSummaryData) -> bool {
    account.enabled && account.send_kind.is_some()
}

async fn resolve_default_signature(
    bg: &mpsc::UnboundedSender<IpcRequest>,
    account_id: &AccountId,
    from_email: &str,
    kind: SignatureContextData,
) -> Result<Option<mxr_compose::ComposeSignature>, MxrError> {
    let resp = ipc_call(
        bg,
        Request::ResolveSignature {
            name: None,
            kind,
            account_id: Some(account_id.clone()),
            from_email: Some(from_email.to_string()),
        },
    )
    .await?;
    match resp {
        Response::Ok {
            data: ResponseData::ResolvedSignature { signature },
        } => Ok(signature.map(|signature| mxr_compose::ComposeSignature {
            name: signature.name,
            body: signature.body,
        })),
        Response::Error { message, .. } => Err(MxrError::Ipc(message)),
        _ => Err(MxrError::Ipc("Unexpected signature response".into())),
    }
}

/// Phase 3.2: structured error from compose validation. Carries the
/// full list of issues (and a category) so the caller can render
/// each on its own line in an `ErrorModalState` instead of jamming
/// them into a single status_message string the user can lose by
/// pressing any key.
#[derive(Debug, Clone)]
pub(crate) struct ComposeValidationError {
    pub kind: ComposeValidationKind,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ComposeValidationKind {
    /// Filesystem / parser error. Single-issue case but kept structured
    /// so the renderer doesn't need to special-case it.
    System,
    /// Per-field validation failures (missing To, missing subject, etc).
    DraftIssues,
}

impl ComposeValidationError {
    pub(crate) fn modal_title(&self) -> &'static str {
        match self.kind {
            ComposeValidationKind::System => "Compose Failed",
            ComposeValidationKind::DraftIssues => "Draft Has Errors",
        }
    }

    pub(crate) fn modal_detail(&self) -> String {
        // One issue per line so the user can scan; ErrorModal's Paragraph
        // renders newlines as separate lines.
        self.issues
            .iter()
            .map(|issue| format!("• {issue}"))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

pub(crate) async fn pending_send_from_edited_draft(
    data: &ComposeReadyData,
) -> Result<Option<PendingSend>, ComposeValidationError> {
    let content = mxr_compose::read_draft_file_async(&data.draft_path)
        .await
        .map_err(|e| ComposeValidationError {
            kind: ComposeValidationKind::System,
            issues: vec![format!("Failed to read draft: {e}")],
        })?;
    let unchanged = content == data.initial_content;

    let (fm, body) = mxr_compose::frontmatter::parse_compose_file(&content).map_err(|e| {
        ComposeValidationError {
            kind: ComposeValidationKind::System,
            issues: vec![format!("Parse error: {e}")],
        }
    })?;
    let save_issues = mxr_compose::validate_draft_for_save(&fm, &body);
    if save_issues.iter().any(|issue| issue.is_error()) {
        return Err(ComposeValidationError {
            kind: ComposeValidationKind::DraftIssues,
            issues: save_issues.iter().map(|issue| issue.to_string()).collect(),
        });
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
        return Err(ComposeValidationError {
            kind: ComposeValidationKind::DraftIssues,
            issues: send_issues.iter().map(|issue| issue.to_string()).collect(),
        });
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
                app.compose.pending_send_confirm = Some(pending);
            }
            Ok(None) => {}
            Err(error) => {
                app.report_error(error.modal_title(), error.modal_detail());
            }
        },
        Ok(_) => {
            app.status_message = Some("Draft discarded".into());
            let _ = mxr_compose::delete_draft_file_async(&data.draft_path).await;
        }
        Err(error) => {
            app.report_error(
                "Compose Failed",
                format!("Failed to launch editor: {error}"),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{resolve_compose_account, ComposeValidationError, ComposeValidationKind};
    use crate::ipc::IpcRequest;
    use mxr_protocol::{
        AccountEditModeData, AccountSourceData, AccountSummaryData, Request, Response, ResponseData,
    };
    use tokio::sync::mpsc;

    fn test_account_summary(email: &str, enabled: bool, is_default: bool) -> AccountSummaryData {
        AccountSummaryData {
            account_id: mxr_core::AccountId::new(),
            key: Some(email.to_string()),
            name: email.to_string(),
            email: email.to_string(),
            provider_kind: "imap".into(),
            sync_kind: Some("imap".into()),
            send_kind: Some("smtp".into()),
            enabled,
            is_default,
            source: AccountSourceData::Config,
            editable: AccountEditModeData::Full,
            sync: None,
            send: None,
            capabilities: Default::default(),
        }
    }

    /// Phase 3.2 / Behavior 3: multiple validation issues render as a
    /// bullet list so users see all of them at once instead of just
    /// the first jammed into a status_message string.
    #[test]
    fn multiple_issues_render_as_bullet_list() {
        let error = ComposeValidationError {
            kind: ComposeValidationKind::DraftIssues,
            issues: vec![
                "Missing To".into(),
                "Subject is empty".into(),
                "Body is blank".into(),
            ],
        };
        let detail = error.modal_detail();
        let lines: Vec<&str> = detail.lines().collect();
        assert_eq!(lines.len(), 3, "one bullet per issue: {detail}");
        assert!(lines.iter().all(|l| l.starts_with("• ")));
        assert!(lines[0].contains("Missing To"));
        assert!(lines[1].contains("Subject"));
        assert!(lines[2].contains("Body"));
    }

    /// Phase 3.2: the modal title differs by kind so users can tell
    /// system errors (filesystem / parser) apart from draft-content
    /// problems.
    #[test]
    fn modal_title_differs_by_validation_kind() {
        let system = ComposeValidationError {
            kind: ComposeValidationKind::System,
            issues: vec!["Failed to read draft".into()],
        };
        let draft = ComposeValidationError {
            kind: ComposeValidationKind::DraftIssues,
            issues: vec!["Missing To".into()],
        };
        assert_eq!(system.modal_title(), "Compose Failed");
        assert_eq!(draft.modal_title(), "Draft Has Errors");
    }

    #[tokio::test]
    async fn compose_account_resolution_skips_disabled_default_account() {
        let disabled_default = test_account_summary("disabled@example.com", false, true);
        let enabled_account = test_account_summary("enabled@example.com", true, false);
        let expected_id = enabled_account.account_id.clone();
        let (tx, mut rx) = mpsc::unbounded_channel::<IpcRequest>();

        tokio::spawn(async move {
            let request = rx.recv().await.expect("compose account request");
            assert!(matches!(request.request, Request::ListAccounts));
            let _ = request.reply.send(Ok(Response::Ok {
                data: ResponseData::Accounts {
                    accounts: vec![disabled_default, enabled_account],
                },
            }));
        });

        let resolved = resolve_compose_account(&tx, None).await.unwrap();

        assert_eq!(resolved.account_id, expected_id);
        assert_eq!(resolved.email, "enabled@example.com");
    }

    #[tokio::test]
    async fn new_compose_draft_includes_tui_recipient_and_subject_frontmatter() {
        let account = test_account_summary("me@example.com", true, true);
        let account_id = account.account_id.clone();
        let (tx, mut rx) = mpsc::unbounded_channel::<IpcRequest>();

        tokio::spawn(async move {
            let request = rx.recv().await.expect("compose account request");
            assert!(matches!(request.request, Request::ListAccounts));
            let _ = request.reply.send(Ok(Response::Ok {
                data: ResponseData::Accounts {
                    accounts: vec![account],
                },
            }));
        });

        let ready = super::handle_compose_action(
            &tx,
            crate::app::ComposeAction::New {
                to: "alice@example.com, bob@example.com".into(),
                subject: "Lunch".into(),
            },
        )
        .await
        .unwrap();

        assert_eq!(ready.account_id, account_id);
        assert!(ready
            .initial_content
            .contains("to: alice@example.com, bob@example.com"));
        assert!(ready.initial_content.contains("subject: Lunch"));

        let _ = std::fs::remove_file(ready.draft_path);
    }
}
