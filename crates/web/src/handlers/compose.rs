use super::super::*;

pub(crate) async fn start_compose_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<ComposeSessionStartRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let session = create_compose_session(&state.config.socket_path, request).await?;
    Ok(Json(json!({ "session": session })))
}

pub(crate) async fn refresh_compose_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<ComposeSessionPathRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let session = load_compose_session(Path::new(&request.draft_path))?;
    Ok(Json(json!({ "session": session })))
}

pub(crate) async fn update_compose_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<ComposeSessionUpdateRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let path = Path::new(&request.draft_path);
    let content =
        std::fs::read_to_string(path).map_err(|error| BridgeError::Ipc(error.to_string()))?;
    let (_existing_frontmatter, body) =
        parse_compose_file(&content).map_err(|error| BridgeError::Ipc(error.to_string()))?;
    let context = extract_compose_context(&content);
    let updated = ComposeFrontmatter {
        to: request.to,
        cc: request.cc,
        bcc: request.bcc,
        subject: request.subject,
        from: request.from,
        in_reply_to: extract_in_reply_to(&content)?,
        references: extract_references(&content)?,
        attach: request.attach,
    };
    let rendered = render_compose_file(&updated, &body, context.as_deref())
        .map_err(|error| BridgeError::Ipc(error.to_string()))?;
    std::fs::write(path, rendered).map_err(|error| BridgeError::Ipc(error.to_string()))?;
    let session = load_compose_session(path)?;
    Ok(Json(json!({ "session": session })))
}

pub(crate) async fn send_compose_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<ComposeSessionSendRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let draft = compose_draft_from_file(&request.draft_path, &request.account_id)?;
    let _ = ack_request(&state.config.socket_path, Request::SendDraft { draft }).await?;
    let _ = std::fs::remove_file(&request.draft_path);
    Ok(Json(json!({ "ok": true })))
}

pub(crate) async fn save_compose_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<ComposeSessionSendRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let draft = compose_draft_from_file(&request.draft_path, &request.account_id)?;
    let _ = ack_request(
        &state.config.socket_path,
        Request::SaveDraftToServer { draft },
    )
    .await?;
    Ok(Json(json!({ "ok": true })))
}

pub(crate) async fn discard_compose_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<ComposeSessionPathRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let _ = std::fs::remove_file(&request.draft_path);
    Ok(Json(json!({ "ok": true })))
}

pub(crate) async fn create_compose_session(
    socket_path: &Path,
    request: ComposeSessionStartRequest,
) -> Result<serde_json::Value, BridgeError> {
    let (account_id, from) = default_account(socket_path).await?;
    let (kind, account_id, cursor_line) = match request.kind {
        ComposeSessionKindRequest::New => (
            request
                .to
                .map_or(ComposeKind::New, |to| ComposeKind::NewWithTo { to }),
            account_id,
            None::<usize>,
        ),
        ComposeSessionKindRequest::Reply | ComposeSessionKindRequest::ReplyAll => {
            let message_id = request
                .message_id
                .as_deref()
                .ok_or_else(|| BridgeError::Ipc("compose reply missing message_id".into()))?;
            let envelope = envelope_for_message(socket_path, message_id).await?;
            let response = ipc_request(
                socket_path,
                Request::PrepareReply {
                    message_id: envelope.id.clone(),
                    reply_all: matches!(request.kind, ComposeSessionKindRequest::ReplyAll),
                },
            )
            .await?;
            let context = match response {
                ResponseData::ReplyContext { context } => context,
                _ => return Err(BridgeError::UnexpectedResponse),
            };
            (
                ComposeKind::Reply {
                    in_reply_to: context.in_reply_to,
                    references: context.references,
                    to: context.reply_to,
                    cc: context.cc,
                    subject: context.subject,
                    thread_context: context.thread_context,
                },
                envelope.account_id,
                None,
            )
        }
        ComposeSessionKindRequest::Forward => {
            let message_id = request
                .message_id
                .as_deref()
                .ok_or_else(|| BridgeError::Ipc("compose forward missing message_id".into()))?;
            let envelope = envelope_for_message(socket_path, message_id).await?;
            let response = ipc_request(
                socket_path,
                Request::PrepareForward {
                    message_id: envelope.id.clone(),
                },
            )
            .await?;
            let context = match response {
                ResponseData::ForwardContext { context } => context,
                _ => return Err(BridgeError::UnexpectedResponse),
            };
            (
                ComposeKind::Forward {
                    subject: context.subject,
                    original_context: context.forwarded_content,
                },
                envelope.account_id,
                None,
            )
        }
    };

    let account = account_summary(socket_path, &account_id).await?;
    let compose_from = if from.trim().is_empty() {
        account.email.clone()
    } else {
        from
    };
    let (draft_path, resolved_cursor_line) =
        crate::mxr_compose::create_draft_file(kind, &compose_from)
            .map_err(|error| BridgeError::Ipc(error.to_string()))?;
    let mut session = load_compose_session(&draft_path)?;
    if let Some(cursor_line) = cursor_line {
        session["cursorLine"] = json!(cursor_line);
    } else {
        session["cursorLine"] = json!(resolved_cursor_line);
    }
    session["accountId"] = json!(account.account_id);
    session["kind"] = json!(compose_kind_name(&request.kind));
    session["editorCommand"] = json!(resolved_editor_command());
    Ok(session)
}

pub(crate) fn compose_kind_name(kind: &ComposeSessionKindRequest) -> &'static str {
    match kind {
        ComposeSessionKindRequest::New => "new",
        ComposeSessionKindRequest::Reply => "reply",
        ComposeSessionKindRequest::ReplyAll => "reply_all",
        ComposeSessionKindRequest::Forward => "forward",
    }
}

pub(crate) fn load_compose_session(path: &Path) -> Result<serde_json::Value, BridgeError> {
    let raw_content =
        std::fs::read_to_string(path).map_err(|error| BridgeError::Ipc(error.to_string()))?;
    let (frontmatter, body) =
        parse_compose_file(&raw_content).map_err(|error| BridgeError::Ipc(error.to_string()))?;
    let rendered = render_markdown(&body);
    let issues = validate_draft(&frontmatter, &body)
        .into_iter()
        .map(compose_issue_view)
        .collect::<Vec<_>>();
    Ok(json!({
        "draftPath": path.display().to_string(),
        "rawContent": raw_content,
        "frontmatter": frontmatter,
        "bodyMarkdown": body,
        "previewHtml": rendered.html,
        "issues": issues,
    }))
}

pub(crate) fn compose_issue_view(issue: ComposeValidation) -> ComposeIssueView {
    match issue {
        ComposeValidation::Error(message) => ComposeIssueView {
            severity: "error",
            message,
        },
        ComposeValidation::Warning(message) => ComposeIssueView {
            severity: "warning",
            message,
        },
    }
}

pub(crate) fn extract_compose_context(content: &str) -> Option<String> {
    const CONTEXT_MARKER: &str = "# --- context (stripped before sending) ---";
    let marker_index = content.find(CONTEXT_MARKER)?;
    let lines = content[marker_index + CONTEXT_MARKER.len()..]
        .lines()
        .map(|line| {
            line.strip_prefix("# ")
                .or_else(|| line.strip_prefix('#'))
                .unwrap_or(line)
        })
        .map(str::trim_end)
        .collect::<Vec<_>>();
    let context = lines.join("\n").trim().to_string();
    if context.is_empty() {
        None
    } else {
        Some(context)
    }
}

pub(crate) fn extract_in_reply_to(content: &str) -> Result<Option<String>, BridgeError> {
    let (frontmatter, _) =
        parse_compose_file(content).map_err(|error| BridgeError::Ipc(error.to_string()))?;
    Ok(frontmatter.in_reply_to)
}

pub(crate) fn extract_references(content: &str) -> Result<Vec<String>, BridgeError> {
    let (frontmatter, _) =
        parse_compose_file(content).map_err(|error| BridgeError::Ipc(error.to_string()))?;
    Ok(frontmatter.references)
}

pub(crate) fn compose_draft_from_file(
    draft_path: &str,
    account_id: &str,
) -> Result<Draft, BridgeError> {
    let raw_content =
        std::fs::read_to_string(draft_path).map_err(|error| BridgeError::Ipc(error.to_string()))?;
    let (frontmatter, body) =
        parse_compose_file(&raw_content).map_err(|error| BridgeError::Ipc(error.to_string()))?;
    let issues = validate_draft(&frontmatter, &body);
    if issues.iter().any(ComposeValidation::is_error) {
        let message = issues
            .into_iter()
            .map(|issue| issue.to_string())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(BridgeError::Ipc(format!("Draft errors: {message}")));
    }

    let now = Utc::now();
    Ok(Draft {
        id: DraftId::new(),
        account_id: parse_account_id(account_id)?,
        reply_headers: frontmatter
            .in_reply_to
            .as_ref()
            .map(|in_reply_to| ReplyHeaders {
                in_reply_to: in_reply_to.clone(),
                references: frontmatter.references.clone(),
            }),
        to: parse_address_list(&frontmatter.to),
        cc: parse_address_list(&frontmatter.cc),
        bcc: parse_address_list(&frontmatter.bcc),
        subject: frontmatter.subject,
        body_markdown: body,
        attachments: frontmatter.attach.into_iter().map(PathBuf::from).collect(),
        created_at: now,
        updated_at: now,
    })
}
