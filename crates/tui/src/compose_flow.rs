use crate::app::{App, ComposeAction, PendingSend, PendingSendMode};
use crate::async_result::{ComposeReadyData, DraftEditReadyData};
use crate::ipc::{ipc_call, ipc_call_dedicated, IpcRequest};
use mxr_core::AccountId;
use mxr_core::Draft;
use mxr_core::MessageId;
use mxr_core::MxrError;
use mxr_protocol::{
    AccountSummaryData, ReplyContext, Request, Response, ResponseData, SignatureContextData,
};
use std::path::Path;
use tokio::sync::mpsc;

/// Fetch a reply context via the shared IPC worker. Used by the cold
/// path in `handle_compose_action` when no prewarm landed.
pub(crate) async fn fetch_reply_context(
    bg: &mpsc::UnboundedSender<IpcRequest>,
    message_id: MessageId,
    reply_all: bool,
) -> Result<ReplyContext, MxrError> {
    let resp = ipc_call(
        bg,
        Request::PrepareReply {
            message_id,
            reply_all,
        },
    )
    .await?;
    extract_reply_context(resp)
}

/// Fetch a reply context on a short-lived daemon connection. Used by
/// the prewarm task in `lib.rs` so it never blocks user actions on the
/// shared IPC worker. Mirrors `ipc_call_dedicated`, which the rest of
/// the codebase uses for slow LLM work for the same reason.
pub(crate) async fn fetch_reply_context_dedicated(
    socket_path: &Path,
    message_id: MessageId,
    reply_all: bool,
) -> Result<ReplyContext, MxrError> {
    let resp = ipc_call_dedicated(
        socket_path,
        Request::PrepareReply {
            message_id,
            reply_all,
        },
    )
    .await?;
    extract_reply_context(resp)
}

fn extract_reply_context(resp: Response) -> Result<ReplyContext, MxrError> {
    match resp {
        Response::Ok {
            data: ResponseData::ReplyContext { context },
        } => Ok(context),
        Response::Error { message, .. } => Err(MxrError::Ipc(message)),
        _ => Err(MxrError::Ipc("unexpected response to PrepareReply".into())),
    }
}

pub(crate) async fn handle_compose_action(
    bg: &mpsc::UnboundedSender<IpcRequest>,
    action: ComposeAction,
) -> Result<ComposeReadyData, MxrError> {
    let mut invite_reply: Option<mxr_core::types::InlineCalendarReply> = None;
    let (account_id, intent, from, kind, signature_kind) = match action {
        ComposeAction::EditDraft { path, account_id } => {
            // Re-edit existing draft — skip creating a new file
            let cursor_line = 1;
            return Ok(ComposeReadyData {
                account_id,
                intent: mxr_core::DraftIntent::New,
                draft_path: path.clone(),
                cursor_line,
                initial_content: mxr_compose::read_draft_file_async(&path)
                    .await
                    .map_err(|e| MxrError::Ipc(e.to_string()))?,
                invite_reply: None,
            });
        }
        ComposeAction::New { to, subject } => {
            let account = resolve_compose_account(bg, None).await?;
            (
                account.account_id,
                mxr_core::DraftIntent::New,
                account.email,
                mxr_compose::ComposeKind::New { to, subject },
                SignatureContextData::New,
            )
        }
        ComposeAction::Reply {
            message_id,
            account_id,
            preloaded,
        } => {
            let account = resolve_compose_account(bg, Some(&account_id)).await?;
            let context = match preloaded {
                Some(ctx) => ctx,
                None => fetch_reply_context(bg, message_id, false).await?,
            };
            let kind = mxr_compose::ComposeKind::Reply {
                reply_all: false,
                in_reply_to: context.in_reply_to,
                references: context.references,
                thread_id: context.thread_id,
                to: context.reply_to,
                cc: context.cc,
                subject: context.subject,
                thread_context: context.thread_context,
            };
            (
                account_id,
                mxr_core::DraftIntent::Reply,
                account.email,
                kind,
                SignatureContextData::Reply,
            )
        }
        ComposeAction::ReplyAll {
            message_id,
            account_id,
            preloaded,
        } => {
            let account = resolve_compose_account(bg, Some(&account_id)).await?;
            let context = match preloaded {
                Some(ctx) => ctx,
                None => fetch_reply_context(bg, message_id, true).await?,
            };
            let kind = mxr_compose::ComposeKind::Reply {
                reply_all: true,
                in_reply_to: context.in_reply_to,
                references: context.references,
                thread_id: context.thread_id,
                to: context.reply_to,
                cc: context.cc,
                subject: context.subject,
                thread_context: context.thread_context,
            };
            (
                account_id,
                mxr_core::DraftIntent::ReplyAll,
                account.email,
                kind,
                SignatureContextData::Reply,
            )
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
                _ => {
                    return Err(MxrError::Ipc(
                        "unexpected response to PrepareForward".into(),
                    ))
                }
            };
            (
                account_id,
                mxr_core::DraftIntent::Forward,
                account.email,
                kind,
                SignatureContextData::Reply,
            )
        }
        ComposeAction::InviteReplyWithComment {
            message_id,
            account_id,
            action: invite_action,
        } => {
            let account = resolve_compose_account(bg, Some(&account_id)).await?;
            let preview =
                fetch_invite_response_preview(bg, message_id.clone(), invite_action).await?;
            invite_reply = Some(mxr_core::types::InlineCalendarReply {
                source_message_id: message_id,
                attendee_email: preview.attendee_email.clone(),
                partstat: invite_action_to_partstat(invite_action),
                ics_body: preview.ics.clone(),
            });
            let kind = mxr_compose::ComposeKind::Reply {
                reply_all: false,
                in_reply_to: String::new(),
                references: Vec::new(),
                thread_id: None,
                to: preview.organizer_email,
                cc: String::new(),
                subject: preview.subject,
                thread_context: String::new(),
            };
            (
                account_id,
                mxr_core::DraftIntent::Reply,
                account.email,
                kind,
                SignatureContextData::Reply,
            )
        }
    };

    let signature = resolve_default_signature(bg, &account_id, &from, signature_kind).await?;
    let (path, cursor_line) =
        mxr_compose::create_draft_file_async_with_signature(kind, &from, signature.as_ref())
            .await
            .map_err(|e| MxrError::Ipc(e.to_string()))?;

    let initial_content = match mxr_compose::read_draft_file_async(&path).await {
        Ok(content) => content,
        Err(e) => {
            let _ = mxr_compose::delete_draft_file_async(&path).await;
            return Err(MxrError::Ipc(e.to_string()));
        }
    };
    Ok(ComposeReadyData {
        account_id,
        intent,
        draft_path: path,
        cursor_line,
        initial_content,
        invite_reply,
    })
}

/// Render a locally-stored draft to an editor-ready compose file and
/// write it to the private scratch dir, ready for `$EDITOR`. Sibling of
/// `handle_compose_action`: unlike the regular compose flow, the result
/// carries the *existing* `Draft` so the post-edit save round-trips
/// through `Request::UpdateDraft` and keeps the same `DraftId` instead
/// of minting a new one.
pub(crate) async fn prepare_draft_edit(
    bg: &mpsc::UnboundedSender<IpcRequest>,
    draft: Draft,
) -> Result<DraftEditReadyData, MxrError> {
    let account = resolve_compose_account(bg, Some(&draft.account_id)).await?;
    let content = mxr_compose::draft_codec::draft_to_compose_file(&draft, &account.email)
        .map_err(|e| MxrError::Ipc(e.to_string()))?;

    let dir = mxr_compose::private_tmp::private_scratch_dir()
        .map_err(|e| MxrError::Ipc(e.to_string()))?;
    let path = dir.join(format!("mxr-draft-edit-{}.md", draft.id));
    // Best-effort: clear a stale file left behind by a previous
    // cancelled/failed edit of this same draft — `write_private` uses
    // O_EXCL and would otherwise refuse to write.
    let _ = tokio::fs::remove_file(&path).await;
    mxr_compose::private_tmp::write_private_async(&path, content.as_bytes())
        .await
        .map_err(|e| MxrError::Ipc(e.to_string()))?;

    Ok(DraftEditReadyData {
        existing: Box::new(draft),
        path,
    })
}

/// Handle the `$EDITOR` exit status for an in-place draft edit. Mirrors
/// `handle_compose_editor_status`, but the edited content is applied
/// onto the existing draft (preserving id/account_id/created_at) and
/// saved via `Request::UpdateDraft` rather than entering the
/// send/save-as-new-draft confirmation flow.
pub(crate) async fn handle_draft_edit_status(
    app: &mut App,
    data: &DraftEditReadyData,
    status: std::io::Result<std::process::ExitStatus>,
    bg: &mpsc::UnboundedSender<IpcRequest>,
) {
    match status {
        Ok(s) if s.success() => {
            let content = match mxr_compose::read_draft_file_async(&data.path).await {
                Ok(content) => content,
                Err(error) => {
                    app.report_error(
                        "Edit Draft Failed",
                        format!("Failed to read draft: {error}"),
                    );
                    return;
                }
            };
            let updated = match mxr_compose::draft_codec::apply_edited_compose_file(
                &data.existing,
                &content,
                chrono::Utc::now(),
            ) {
                Ok(updated) => updated,
                Err(error) => {
                    // Keep the temp file so the user's edits aren't lost —
                    // there's no in-app retry for this path, so surfacing
                    // the path lets them recover the content by hand.
                    app.report_error(
                        "Edit Draft Failed",
                        format!(
                            "Could not parse the edited draft: {error}\n\nYour edits are still at {}",
                            data.path.display()
                        ),
                    );
                    return;
                }
            };

            match ipc_call(bg, Request::UpdateDraft { draft: updated }).await {
                Ok(Response::Ok {
                    data: ResponseData::Ack,
                }) => {
                    app.status_message = Some("Draft updated".into());
                    app.schedule_draft_cleanup(data.path.clone());
                }
                Ok(Response::Error { message, .. }) => {
                    app.report_error(
                        "Update Draft Failed",
                        format!(
                            "{message}\n\nYour edits are still at {}",
                            data.path.display()
                        ),
                    );
                }
                Ok(_) => {
                    app.report_error(
                        "Update Draft Failed",
                        "Unexpected daemon response to UpdateDraft".to_string(),
                    );
                }
                Err(error) => {
                    app.report_error(
                        "Update Draft Failed",
                        format!("{error}\n\nYour edits are still at {}", data.path.display()),
                    );
                }
            }
        }
        Ok(_) => {
            app.status_message = Some("Edit cancelled".into());
            app.schedule_draft_cleanup(data.path.clone());
        }
        Err(error) => {
            app.report_error(
                "Edit Draft Failed",
                format!("Failed to launch editor: {error}"),
            );
        }
    }
}

fn invite_action_to_partstat(
    action: mxr_protocol::CalendarInviteActionData,
) -> mxr_core::types::CalendarPartstat {
    use mxr_core::types::CalendarPartstat;
    use mxr_protocol::CalendarInviteActionData;
    match action {
        CalendarInviteActionData::Accept => CalendarPartstat::Accepted,
        CalendarInviteActionData::Tentative => CalendarPartstat::Tentative,
        CalendarInviteActionData::Decline => CalendarPartstat::Declined,
    }
}

async fn fetch_invite_response_preview(
    bg: &mpsc::UnboundedSender<IpcRequest>,
    message_id: MessageId,
    action: mxr_protocol::CalendarInviteActionData,
) -> Result<mxr_protocol::CalendarInviteResponsePreview, MxrError> {
    let resp = ipc_call(bg, Request::PrepareInviteResponse { message_id, action }).await?;
    match resp {
        Response::Ok {
            data: ResponseData::InviteResponsePreview { preview },
        } => Ok(preview),
        Response::Error { message, .. } => Err(MxrError::Ipc(message)),
        _ => Err(MxrError::Ipc(
            "unexpected response to PrepareInviteResponse".into(),
        )),
    }
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
) -> Result<PendingSend, ComposeValidationError> {
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
    if save_issues
        .iter()
        .any(mxr_compose::ComposeValidation::is_error)
    {
        return Err(ComposeValidationError {
            kind: ComposeValidationKind::DraftIssues,
            issues: save_issues
                .iter()
                .map(std::string::ToString::to_string)
                .collect(),
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
            issues: send_issues
                .iter()
                .map(std::string::ToString::to_string)
                .collect(),
        });
    };

    Ok(PendingSend {
        account_id: data.account_id.clone(),
        intent: if fm.intent == mxr_core::DraftIntent::New {
            data.intent
        } else {
            fm.intent
        },
        fm,
        body,
        draft_path: data.draft_path.clone(),
        mode,
        safety_report: None,
        safety_check_failed: None,
        override_token: None,
        suggested_collaborators: vec![],
        invite_reply: data.invite_reply.clone(),
    })
}

pub(crate) async fn handle_compose_editor_status(
    app: &mut App,
    data: &ComposeReadyData,
    status: std::io::Result<std::process::ExitStatus>,
    bg: &mpsc::UnboundedSender<IpcRequest>,
) {
    match status {
        Ok(s) if s.success() => match pending_send_from_edited_draft(data).await {
            Ok(mut pending) => {
                // Run the pre-send safety check before showing the
                // modal. A failed IPC (daemon down, worker dropped)
                // is non-fatal: the modal still opens with
                // `safety_report = None` so the user is never blocked
                // from seeing their draft, just from the safety hint.
                stamp_safety_report(&mut pending, bg).await;
                stamp_suggestions(&mut pending, bg).await;
                app.compose.pending_send_confirm = Some(pending);
            }
            Err(error) => {
                app.report_error(error.modal_title(), error.modal_detail());
            }
        },
        Ok(_) => {
            app.status_message = Some("Draft discarded".into());
            let _ = mxr_compose::delete_draft_file_async(&data.draft_path).await;
        }
        Err(error) => {
            app.schedule_draft_cleanup(data.draft_path.clone());
            app.report_error(
                "Compose Failed",
                format!("Failed to launch editor: {error}"),
            );
        }
    }
}

async fn stamp_safety_report(pending: &mut PendingSend, bg: &mpsc::UnboundedSender<IpcRequest>) {
    let draft = draft_from_pending(pending);
    let context = mxr_protocol::DraftSafetyContextData {
        mode: mxr_protocol::DraftSafetyModeData::Check,
        reply_all: matches!(pending.intent, mxr_core::DraftIntent::ReplyAll),
        original_message_id: None,
        thread_id: pending.fm.thread_id.as_ref().and_then(|s| s.parse().ok()),
        allow_llm: true,
        // Compose-flow doesn't pre-schedule; the user can send-now or
        // send-at via the modal. Pass `now` so the timing check fires
        // for immediate sends; send-at is handled by the daemon's
        // schedule path which calls CheckDraftSafety with its own
        // proposed_send_at.
        proposed_send_at: Some(chrono::Utc::now()),
    };
    match ipc_call(bg, Request::CheckDraftSafety { draft, context }).await {
        Ok(Response::Ok {
            data: ResponseData::DraftSafetyReportResponse { report },
        }) => {
            // The daemon mints a single-use override token and
            // stamps it onto each Blocker issue when the verdict is
            // Blocked. Surface the first one to the modal.
            pending.override_token = report.issues.iter().find_map(|i| i.override_token.clone());
            pending.safety_report = Some(report);
        }
        Ok(Response::Error { message, .. }) => {
            // Daemon error: non-fatal, but record why so the modal can
            // say "Safety check unavailable" instead of silently
            // looking like a clean run.
            pending.safety_check_failed = Some(message);
        }
        Ok(Response::Ok { .. }) => {
            pending.safety_check_failed =
                Some("unexpected daemon response to CheckDraftSafety".into());
        }
        Err(error) => {
            // IPC worker dropped or daemon unreachable. The user can
            // still see the modal; they just won't have safety hints.
            pending.safety_check_failed = Some(error.to_string());
        }
    }
}

/// Slice 5.3 (C2.7 cont): fetch "maybe include" suggestions and
/// stamp them onto `pending.suggested_collaborators`. Failure is
/// silent — the modal just renders without the suggestions block.
async fn stamp_suggestions(pending: &mut PendingSend, bg: &mpsc::UnboundedSender<IpcRequest>) {
    let draft = draft_from_pending(pending);
    let req = Request::SuggestCollaborators { draft, limit: 5 };
    if let Ok(Response::Ok {
        data: ResponseData::SuggestedCollaborators { suggestions },
    }) = ipc_call(bg, req).await
    {
        pending.suggested_collaborators = suggestions;
    }
}

fn draft_from_pending(pending: &PendingSend) -> mxr_core::Draft {
    let parse_addrs = |s: &str| mxr_mail_parse::parse_address_list(s);
    let reply_headers =
        pending
            .fm
            .in_reply_to
            .as_ref()
            .map(|in_reply_to| mxr_core::types::ReplyHeaders {
                in_reply_to: in_reply_to.clone(),
                references: pending.fm.references.clone(),
                thread_id: pending.fm.thread_id.clone(),
            });
    let now = chrono::Utc::now();
    mxr_core::Draft {
        id: mxr_core::id::DraftId::new(),
        account_id: pending.account_id.clone(),
        reply_headers,
        intent: pending.intent,
        to: parse_addrs(&pending.fm.to),
        cc: parse_addrs(&pending.fm.cc),
        bcc: parse_addrs(&pending.fm.bcc),
        subject: pending.fm.subject.clone(),
        body_markdown: pending.body.clone(),
        attachments: pending
            .fm
            .attach
            .iter()
            .map(std::path::PathBuf::from)
            .collect(),
        inline_calendar_reply: pending.invite_reply.clone(),
        created_at: now,
        updated_at: now,
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
            let request = rx.recv().await.expect("compose signature request");
            assert!(matches!(request.request, Request::ResolveSignature { .. }));
            let _ = request.reply.send(Ok(Response::Ok {
                data: ResponseData::ResolvedSignature { signature: None },
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

    /// Plan 004 / Step 4a: editor-launch failure schedules the draft
    /// file for cleanup so the file is not stranded on disk.
    #[tokio::test]
    async fn editor_launch_failure_schedules_draft_cleanup() {
        use crate::app::App;
        use crate::async_result::ComposeReadyData;

        let temp = std::env::temp_dir().join(format!(
            "mxr-compose-cleanup-test-{}.md",
            std::process::id()
        ));
        std::fs::write(&temp, "---\n").unwrap();

        let data = ComposeReadyData {
            account_id: mxr_core::AccountId::new(),
            intent: mxr_core::DraftIntent::New,
            draft_path: temp.clone(),
            cursor_line: 1,
            initial_content: String::new(),
            invite_reply: None,
        };
        let mut app = App::new();
        let (bg, _bg_rx) = mpsc::unbounded_channel::<IpcRequest>();

        super::handle_compose_editor_status(
            &mut app,
            &data,
            Err(std::io::Error::other("boom")),
            &bg,
        )
        .await;

        let cleanup_paths = app.take_pending_draft_cleanup();
        assert!(
            cleanup_paths.contains(&temp),
            "expected {temp:?} in cleanup queue, got: {cleanup_paths:?}"
        );

        let _ = std::fs::remove_file(&temp);
    }

    /// Plan 004 / Step 4b: editor non-zero exit (discard) deletes the
    /// draft file immediately.
    #[tokio::test]
    async fn editor_discard_deletes_draft_file() {
        use crate::app::App;
        use crate::async_result::ComposeReadyData;
        use std::os::unix::process::ExitStatusExt;

        let temp = std::env::temp_dir().join(format!(
            "mxr-compose-discard-test-{}.md",
            std::process::id()
        ));
        std::fs::write(&temp, "---\n").unwrap();
        assert!(temp.exists(), "test setup: file must exist before discard");

        let data = ComposeReadyData {
            account_id: mxr_core::AccountId::new(),
            intent: mxr_core::DraftIntent::New,
            draft_path: temp.clone(),
            cursor_line: 1,
            initial_content: String::new(),
            invite_reply: None,
        };
        let mut app = App::new();
        let (bg, _bg_rx) = mpsc::unbounded_channel::<IpcRequest>();

        // non-zero exit status = discard
        let discard_status = std::process::ExitStatus::from_raw(1 << 8);
        super::handle_compose_editor_status(&mut app, &data, Ok(discard_status), &bg).await;

        assert!(!temp.exists(), "draft file should be deleted after discard");
    }
}
